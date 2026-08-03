use super::Queue;
use super::{instance::Instance, physical_device::PhysicalDevice, queue::QueueFamilyIndices};
use crate::SubmitDesc;
use ash::vk;
use comfy_table::Table;
use std::{
    collections::HashSet,
    ffi::CStr,
    fmt::Debug,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone)]
struct DeviceExtensionRequirement {
    name: &'static CStr,
    reason: &'static str,
}

const GPU_JOB_FENCE_POOL_CAPACITY: usize = 64;

struct GpuJobFencePoolCounters {
    created: AtomicU64,
    reused: AtomicU64,
    recycled: AtomicU64,
    destroyed_uncompleted: AtomicU64,
    destroyed_on_cap: AtomicU64,
    destroyed_on_reset_failure: AtomicU64,
    max_pool_size: AtomicUsize,
}

impl GpuJobFencePoolCounters {
    fn new() -> Self {
        Self {
            created: AtomicU64::new(0),
            reused: AtomicU64::new(0),
            recycled: AtomicU64::new(0),
            destroyed_uncompleted: AtomicU64::new(0),
            destroyed_on_cap: AtomicU64::new(0),
            destroyed_on_reset_failure: AtomicU64::new(0),
            max_pool_size: AtomicUsize::new(0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobFencePoolStats {
    pub created: u64,
    pub reused: u64,
    pub recycled: u64,
    pub destroyed_uncompleted: u64,
    pub destroyed_on_cap: u64,
    pub destroyed_on_reset_failure: u64,
    pub current_pool_size: usize,
    pub max_pool_size: usize,
    pub capacity: usize,
}

struct DeviceInner {
    device: ash::Device,
    gpu_job_fence_pool: Mutex<Vec<vk::Fence>>,
    gpu_job_fence_pool_counters: GpuJobFencePoolCounters,
}

impl Drop for DeviceInner {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            let pooled_fences = self.gpu_job_fence_pool.get_mut().unwrap();
            let pooled_count = pooled_fences.len();
            for fence in pooled_fences.drain(..) {
                self.device.destroy_fence(fence, None);
            }
            log::debug!(
                "[VKN][GPU_JOB_FENCE_POOL] destroy pooled={} created={} reused={} recycled={} destroyed_uncompleted={} destroyed_on_cap={} destroyed_on_reset_failure={} max_pool_size={} capacity={}",
                pooled_count,
                self.gpu_job_fence_pool_counters.created.load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters.reused.load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters.recycled.load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters.destroyed_uncompleted.load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters.destroyed_on_cap.load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters
                    .destroyed_on_reset_failure
                    .load(Ordering::Relaxed),
                self.gpu_job_fence_pool_counters.max_pool_size.load(Ordering::Relaxed),
                GPU_JOB_FENCE_POOL_CAPACITY,
            );
            self.device.destroy_device(None);
        }
    }
}

#[derive(Clone)]
pub struct Device(Arc<DeviceInner>);

impl std::ops::Deref for Device {
    type Target = ash::Device;
    fn deref(&self) -> &Self::Target {
        &self.0.device
    }
}

impl Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Device({:?})", self.0.device.handle())
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.0.device.handle() == other.0.device.handle()
    }
}

impl Device {
    pub fn new(
        instance: &Instance,
        physical_device: &PhysicalDevice,
        queue_family_indices: &QueueFamilyIndices,
    ) -> Self {
        let physical_device_raw = physical_device.as_raw();
        let extension_requirements = device_extension_requirements();
        validate_device_capabilities(
            instance.as_raw(),
            physical_device_raw,
            &extension_requirements,
        );
        let device = create_device(
            instance.as_raw(),
            physical_device_raw,
            queue_family_indices,
            &extension_requirements,
        );
        Self(Arc::new(DeviceInner {
            device,
            gpu_job_fence_pool: Mutex::new(Vec::new()),
            gpu_job_fence_pool_counters: GpuJobFencePoolCounters::new(),
        }))
    }

    pub fn as_raw(&self) -> &ash::Device {
        &self.0.device
    }

    pub fn wait_queue_idle(&self, queue: &Queue) {
        unsafe { self.as_raw().queue_wait_idle(queue.as_raw()).unwrap() };
    }

    pub(crate) fn record_gpu_job_fence_created(&self) {
        self.0
            .gpu_job_fence_pool_counters
            .created
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn acquire_gpu_job_fence(&self) -> ash::prelude::VkResult<Option<vk::Fence>> {
        let Some(fence) = self.0.gpu_job_fence_pool.lock().unwrap().pop() else {
            return Ok(None);
        };
        let reset_result = unsafe { self.reset_fences(&[fence]) };
        if let Err(err) = reset_result {
            self.0
                .gpu_job_fence_pool_counters
                .destroyed_on_reset_failure
                .fetch_add(1, Ordering::Relaxed);
            log::warn!(
                "[VKN][GPU_JOB_FENCE_POOL] destroying pooled fence after reset failure: {err}"
            );
            unsafe {
                self.destroy_fence(fence, None);
            }
            return Err(err);
        }
        self.0
            .gpu_job_fence_pool_counters
            .reused
            .fetch_add(1, Ordering::Relaxed);
        Ok(Some(fence))
    }

    pub(crate) fn recycle_gpu_job_fence(&self, fence: vk::Fence) {
        let (should_destroy, pool_size) = {
            let mut pool = self.0.gpu_job_fence_pool.lock().unwrap();
            if pool.len() >= GPU_JOB_FENCE_POOL_CAPACITY {
                (true, pool.len())
            } else {
                pool.push(fence);
                let pool_size = pool.len();
                self.update_gpu_job_fence_pool_max(pool_size);
                (false, pool_size)
            }
        };

        if should_destroy {
            self.0
                .gpu_job_fence_pool_counters
                .destroyed_on_cap
                .fetch_add(1, Ordering::Relaxed);
            log::warn!(
                "[VKN][GPU_JOB_FENCE_POOL] capacity {} reached; destroying completed fence instead of recycling it (pool_size={})",
                GPU_JOB_FENCE_POOL_CAPACITY,
                pool_size,
            );
            unsafe {
                self.destroy_fence(fence, None);
            }
        } else {
            self.0
                .gpu_job_fence_pool_counters
                .recycled
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn destroy_uncompleted_gpu_job_fence(&self, fence: vk::Fence) {
        self.0
            .gpu_job_fence_pool_counters
            .destroyed_uncompleted
            .fetch_add(1, Ordering::Relaxed);
        unsafe {
            self.destroy_fence(fence, None);
        }
    }

    pub fn gpu_job_fence_pool_stats(&self) -> GpuJobFencePoolStats {
        GpuJobFencePoolStats {
            created: self
                .0
                .gpu_job_fence_pool_counters
                .created
                .load(Ordering::Relaxed),
            reused: self
                .0
                .gpu_job_fence_pool_counters
                .reused
                .load(Ordering::Relaxed),
            recycled: self
                .0
                .gpu_job_fence_pool_counters
                .recycled
                .load(Ordering::Relaxed),
            destroyed_uncompleted: self
                .0
                .gpu_job_fence_pool_counters
                .destroyed_uncompleted
                .load(Ordering::Relaxed),
            destroyed_on_cap: self
                .0
                .gpu_job_fence_pool_counters
                .destroyed_on_cap
                .load(Ordering::Relaxed),
            destroyed_on_reset_failure: self
                .0
                .gpu_job_fence_pool_counters
                .destroyed_on_reset_failure
                .load(Ordering::Relaxed),
            current_pool_size: self.0.gpu_job_fence_pool.lock().unwrap().len(),
            max_pool_size: self
                .0
                .gpu_job_fence_pool_counters
                .max_pool_size
                .load(Ordering::Relaxed),
            capacity: GPU_JOB_FENCE_POOL_CAPACITY,
        }
    }

    fn update_gpu_job_fence_pool_max(&self, pool_size: usize) {
        let max_pool_size = &self.0.gpu_job_fence_pool_counters.max_pool_size;
        let mut current = max_pool_size.load(Ordering::Relaxed);
        while pool_size > current {
            match max_pool_size.compare_exchange_weak(
                current,
                pool_size,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }

    pub fn submit_to_queue(&self, queue: &Queue, desc: SubmitDesc<'_>) -> ash::prelude::VkResult<()> {
        desc.assert_supported_sizes();
        crate::sync::diagnostics::record_submit(&desc);

        let (raw_command_buffers, command_buffer_count) = desc.raw_command_buffers();
        let (raw_wait_semaphores, raw_wait_stages, wait_count) = desc.raw_waits();
        let (raw_signal_semaphores, signal_count) = desc.raw_signals();
        let command_buffers = &raw_command_buffers[..command_buffer_count];
        let wait_semaphores = &raw_wait_semaphores[..wait_count];
        let wait_stages = &raw_wait_stages[..wait_count];
        let signal_semaphores = &raw_signal_semaphores[..signal_count];
        let submit_info = [vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores)];
        let fence = desc.fence.map_or(vk::Fence::null(), |fence| fence.as_raw());

        let result = unsafe { self.as_raw().queue_submit(queue.as_raw(), &submit_info, fence) };
        if result.is_ok() {
            for command_buffer in desc.command_buffers {
                command_buffer.commit_state_transaction();
            }
        }
        result
    }

    #[allow(unused)]
    pub fn wait_idle(&self) {
        unsafe { self.as_raw().device_wait_idle().unwrap() };
    }

    /// Get a queue from the device, only the first queue is returned in current implementation
    pub fn get_queue(&self, queue_family_index: u32) -> Queue {
        let queue = unsafe { self.as_raw().get_device_queue(queue_family_index, 0) };
        Queue::new(queue)
    }

    pub fn cmd_bind_pipeline_graphics_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        pipeline: vk::Pipeline,
    ) {
        unsafe {
            self.as_raw().cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }
    }

    pub fn cmd_bind_pipeline_compute_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        pipeline: vk::Pipeline,
    ) {
        unsafe {
            self.as_raw().cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                pipeline,
            );
        }
    }

    pub fn cmd_set_viewport_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        first_viewport: u32,
        viewports: &[vk::Viewport],
    ) {
        unsafe {
            self.as_raw()
                .cmd_set_viewport(command_buffer, first_viewport, viewports);
        }
    }

    pub fn cmd_set_scissor_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        first_scissor: u32,
        scissors: &[vk::Rect2D],
    ) {
        unsafe {
            self.as_raw()
                .cmd_set_scissor(command_buffer, first_scissor, scissors);
        }
    }

    pub fn cmd_push_constants_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        stage_flags: vk::ShaderStageFlags,
        offset: u32,
        constants: &[u8],
    ) {
        unsafe {
            self.as_raw().cmd_push_constants(
                command_buffer,
                layout,
                stage_flags,
                offset,
                constants,
            );
        }
    }

    pub fn cmd_bind_index_buffer_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        index_type: vk::IndexType,
    ) {
        unsafe {
            self.as_raw()
                .cmd_bind_index_buffer(command_buffer, buffer, offset, index_type);
        }
    }

    pub fn cmd_bind_vertex_buffers_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        first_binding: u32,
        buffers: &[vk::Buffer],
        offsets: &[vk::DeviceSize],
    ) {
        unsafe {
            self.as_raw()
                .cmd_bind_vertex_buffers(command_buffer, first_binding, buffers, offsets);
        }
    }

    pub fn cmd_bind_descriptor_sets_graphics_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        first_set: u32,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        unsafe {
            self.as_raw().cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                first_set,
                descriptor_sets,
                &[],
            );
        }
    }

    pub fn cmd_bind_descriptor_sets_compute_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        first_set: u32,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        unsafe {
            self.as_raw().cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                first_set,
                descriptor_sets,
                &[],
            );
        }
    }

    pub fn cmd_draw_indexed_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.as_raw().cmd_draw_indexed(
                command_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    pub fn cmd_draw_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.as_raw().cmd_draw(
                command_buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
    }

    pub fn cmd_dispatch_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    ) {
        unsafe {
            self.as_raw()
                .cmd_dispatch(command_buffer, group_count_x, group_count_y, group_count_z);
        }
    }

    pub fn cmd_dispatch_indirect_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
    ) {
        unsafe {
            self.as_raw()
                .cmd_dispatch_indirect(command_buffer, buffer, offset);
        }
    }

    pub fn cmd_begin_render_pass_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        begin_info: &vk::RenderPassBeginInfo,
        contents: vk::SubpassContents,
    ) {
        unsafe {
            self.as_raw()
                .cmd_begin_render_pass(command_buffer, begin_info, contents);
        }
    }

    pub fn cmd_end_render_pass_raw(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            self.as_raw().cmd_end_render_pass(command_buffer);
        }
    }
}

fn create_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_indices: &QueueFamilyIndices,
    extension_requirements: &[DeviceExtensionRequirement],
) -> ash::Device {
    let queue_priorities = [1.0f32];
    let queue_create_infos = {
        let mut indices = HashSet::new();
        for idx in queue_family_indices.get_all_indices() {
            indices.insert(idx);
        }
        indices
            .into_iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(index)
                    .queue_priorities(&queue_priorities)
            })
            .collect::<Vec<_>>()
    };

    let extension_ptrs: Vec<*const i8> = extension_requirements
        .iter()
        .map(|req| req.name.as_ptr())
        .collect();

    let physical_device_features = vk::PhysicalDeviceFeatures {
        shader_int64: vk::TRUE,
        ..Default::default()
    };

    let mut buffer_device_address_features = vk::PhysicalDeviceBufferDeviceAddressFeatures {
        buffer_device_address: vk::TRUE,
        ..Default::default()
    };
    let mut maintenance4_features =
        vk::PhysicalDeviceMaintenance4Features::default().maintenance4(true);

    // Shader clock is debug-only and disabled by default for broader GPU compatibility.
    // let mut physical_device_shader_clock_features_khr = vk::PhysicalDeviceShaderClockFeaturesKHR {
    //     shader_subgroup_clock: vk::TRUE,
    //     ..Default::default()
    // };

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_ptrs)
        .enabled_features(&physical_device_features)
        .push_next(&mut maintenance4_features)
        .push_next(&mut buffer_device_address_features);
    // .push_next(&mut physical_device_shader_clock_features_khr);

    unsafe {
        instance
            .create_device(physical_device, &device_create_info, None)
            .expect("Failed to create logical device")
    }
}

fn device_extension_requirements() -> Vec<DeviceExtensionRequirement> {
    let requirements = vec![
        DeviceExtensionRequirement {
            name: vk::KHR_SWAPCHAIN_NAME,
            reason: "Required to present rendered images to the window surface",
        },
        DeviceExtensionRequirement {
            name: vk::KHR_MAINTENANCE4_NAME,
            reason: "Needed because compute shaders rely on LocalSizeId execution mode",
        },
        DeviceExtensionRequirement {
            name: vk::KHR_DEFERRED_HOST_OPERATIONS_NAME,
            reason:
                "Needed for `VK_KHR_acceleration_structure` companion functionality (shader builds)",
        },
    ];

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let mut requirements = requirements;
        requirements.push(DeviceExtensionRequirement {
            name: ash::khr::portability_subset::NAME,
            reason: "macOS/iOS MoltenVK portability requirements",
        });
        return requirements;
    }

    #[allow(unreachable_code)]
    requirements
}

fn collect_missing_extension_rows(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    requirements: &[DeviceExtensionRequirement],
) -> Vec<(String, String)> {
    let properties = unsafe {
        instance
            .enumerate_device_extension_properties(physical_device)
            .expect("Failed to enumerate device extension properties")
    };

    requirements
        .iter()
        .filter(|req| {
            !properties.iter().any(|ext| {
                let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
                name == req.name
            })
        })
        .map(|req| {
            (
                req.name.to_string_lossy().into_owned(),
                req.reason.to_string(),
            )
        })
        .collect()
}

fn collect_missing_feature_rows(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();

    let mut buffer_device_address_features =
        vk::PhysicalDeviceBufferDeviceAddressFeatures::default();
    let mut maintenance4_features = vk::PhysicalDeviceMaintenance4Features::default();
    // let mut shader_clock_features = vk::PhysicalDeviceShaderClockFeaturesKHR::default();

    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut buffer_device_address_features)
        .push_next(&mut maintenance4_features);
    // .push_next(&mut shader_clock_features);

    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }

    if features2.features.shader_int64 != vk::TRUE {
        rows.push((
            "shaderInt64".to_string(),
            "Core Vulkan feature required for renderer compute passes".to_string(),
        ));
    }

    if buffer_device_address_features.buffer_device_address != vk::TRUE {
        rows.push((
            "bufferDeviceAddress".to_string(),
            "VK_KHR_buffer_device_address feature required for GPU pointers".to_string(),
        ));
    }

    if maintenance4_features.maintenance4 != vk::TRUE {
        rows.push((
            "maintenance4".to_string(),
            "VK_KHR_maintenance4 feature required for compute shaders that declare LocalSizeId"
                .to_string(),
        ));
    }

    // if shader_clock_features.shader_subgroup_clock != vk::TRUE {
    //     rows.push((
    //         "shader_subgroup_clock".to_string(),
    //         "VK_KHR_shader_clock feature required for GPU timing".to_string(),
    //     ));
    // }

    rows
}

fn validate_device_capabilities(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension_requirements: &[DeviceExtensionRequirement],
) {
    let missing_extensions =
        collect_missing_extension_rows(instance, physical_device, extension_requirements);
    let missing_features = collect_missing_feature_rows(instance, physical_device);

    if missing_extensions.is_empty() && missing_features.is_empty() {
        return;
    }

    let props = unsafe { instance.get_physical_device_properties(physical_device) };
    let device_name = unsafe {
        CStr::from_ptr(props.device_name.as_ptr())
            .to_string_lossy()
            .into_owned()
    };

    log::error!(
        "\n--- Device capability check failed for \"{}\" ---",
        device_name
    );
    let mut table = Table::new();
    table.set_header(vec!["Type", "Name", "Details"]);

    for (ext, detail) in missing_extensions {
        table.add_row(vec![
            "Extension".to_string(),
            ext,
            format!("{detail} (not reported by the selected physical device)"),
        ]);
    }

    for (name, details) in missing_features {
        table.add_row(vec!["Feature".to_string(), name, details]);
    }

    log::error!("{table}");

    panic!(
        "Selected GPU \"{}\" lacks required Vulkan capabilities. Please choose a device that provides the extensions/features listed above.",
        device_name
    );
}
