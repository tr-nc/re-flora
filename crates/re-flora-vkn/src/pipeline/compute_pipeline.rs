use super::descriptor_set_utils;
use crate::{
    Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
    Extent3D, FrameRetirement, PipelineLayout, ResourceContainer, ResourceState,
    ResourceStatePolicy, ResourceStateTracker, ShaderModule, Texture, WriteDescriptorSet,
};
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex},
};

struct ComputePipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    workgroup_size: [u32; 3],
    descriptor_pool: DescriptorPool,
    descriptor_sets: Mutex<Vec<DescriptorSet>>,
    pending_descriptor_sets: Mutex<Option<Vec<DescriptorSet>>>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    texture_bindings: Mutex<HashMap<String, ComputeTextureBinding>>,
    resource_state_tracker: Mutex<ResourceStateTracker>,
    auto_texture_transitions_enabled: Mutex<bool>,
}

const MANUAL_TEXTURE_BINDING_PREFIX: &str = "manual:";

#[derive(Clone)]
struct ComputeTextureBinding {
    texture: Texture,
    state: ResourceState,
}

impl Drop for ComputePipelineInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

#[derive(Clone)]
pub struct ComputePipeline(Arc<ComputePipelineInner>);

impl Deref for ComputePipeline {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.0.pipeline
    }
}

impl ComputePipeline {
    pub fn new(
        device: &Device,
        shader_module: &ShaderModule,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Self {
        let stage_info = shader_module.get_shader_stage_create_info();
        let pipeline_layout = PipelineLayout::from_shader_module(device, shader_module);
        let workgroup_size = shader_module.get_workgroup_size().unwrap();

        let pipeline_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(pipeline_layout.as_raw());

        let pipeline = unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .map_err(|e| e.1)
                .unwrap()[0]
        };

        let descriptor_sets_bindings = shader_module.get_descriptor_sets_bindings();

        let pipeline_instance = Self(Arc::new(ComputePipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout,
            workgroup_size,
            descriptor_pool: descriptor_pool.clone(),
            descriptor_sets: Mutex::new(vec![]),
            pending_descriptor_sets: Mutex::new(None),
            descriptor_sets_bindings,
            texture_bindings: Mutex::new(HashMap::new()),
            resource_state_tracker: Mutex::new(ResourceStateTracker::automatic()),
            auto_texture_transitions_enabled: Mutex::new(true),
        }));

        // auto-create descriptor sets
        descriptor_set_utils::auto_create_descriptor_sets(
            descriptor_pool,
            resource_containers,
            &pipeline_instance.0.pipeline_layout,
            &pipeline_instance.0.descriptor_sets_bindings,
            &pipeline_instance.0.descriptor_sets,
        )
        .unwrap();
        pipeline_instance.update_texture_bindings(resource_containers);

        pipeline_instance
    }

    /// Updates existing descriptor sets with new resources.
    pub fn auto_update_descriptor_sets(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        if let Some(descriptor_sets) = self.0.pending_descriptor_sets.lock().unwrap().as_ref() {
            descriptor_set_utils::auto_update_descriptor_sets_on_sets(
                resource_containers,
                &self.0.descriptor_sets_bindings,
                descriptor_sets,
            )?;
        } else {
            descriptor_set_utils::auto_update_descriptor_sets(
                resource_containers,
                &self.0.descriptor_sets_bindings,
                &self.0.descriptor_sets,
            )?;
        }
        self.update_texture_bindings(resource_containers);
        Ok(())
    }

    /// Fork the current descriptor generation so runtime writes cannot mutate an in-flight set.
    pub fn begin_descriptor_generation(&self) -> Result<()> {
        let active = self.0.descriptor_sets.lock().unwrap();
        let mut set_nos = self.0.descriptor_sets_bindings.keys().copied().collect::<Vec<_>>();
        set_nos.sort_unstable();
        let pending = active
            .iter()
            .zip(set_nos)
            .map(|(set, set_no)| {
                let layout = self
                    .0
                    .pipeline_layout
                    .get_descriptor_set_layouts()
                    .get(&set_no)
                    .expect("descriptor set layout disappeared during generation fork");
                set.fork(&self.0.descriptor_pool, layout)
            })
            .collect::<Result<Vec<_>>>()?;
        let replaced = self
            .0
            .pending_descriptor_sets
            .lock()
            .unwrap()
            .replace(pending);
        assert!(replaced.is_none(), "descriptor generation already in progress");
        Ok(())
    }

    /// Publish the staged descriptor generation and return the previous one for completion-scoped
    /// retirement. Callers must schedule the returned retirement on the frame clock.
    pub fn publish_descriptor_generation(
        &self,
        name: &'static str,
        generation: u64,
    ) -> Option<FrameRetirement> {
        let pending = self
            .0
            .pending_descriptor_sets
            .lock()
            .unwrap()
            .take()
            .expect("descriptor generation publish requires begin_descriptor_generation");
        let old = std::mem::replace(&mut *self.0.descriptor_sets.lock().unwrap(), pending);
        Some(FrameRetirement::new(name, generation, old))
    }

    pub fn write_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        let mut write = write;
        self.update_texture_binding_from_write(set_no, &write);
        if let Some(descriptor_sets) = self.0.pending_descriptor_sets.lock().unwrap().as_ref() {
            descriptor_sets[set_no as usize].perform_writes(std::slice::from_mut(&mut write));
        } else {
            let guard = self.0.descriptor_sets.lock().unwrap();
            guard[set_no as usize].perform_writes(std::slice::from_mut(&mut write));
        }
    }

    pub fn set_resource_state_tracker(&self, tracker: ResourceStateTracker) {
        *self.0.resource_state_tracker.lock().unwrap() = tracker;
    }

    pub fn set_resource_state_policy(&self, policy: ResourceStatePolicy) {
        self.0
            .resource_state_tracker
            .lock()
            .unwrap()
            .set_policy(policy);
    }

    pub fn resource_state_policy(&self) -> ResourceStatePolicy {
        self.0.resource_state_tracker.lock().unwrap().policy()
    }

    pub fn set_auto_texture_transitions_enabled(&self, enabled: bool) {
        *self.0.auto_texture_transitions_enabled.lock().unwrap() = enabled;
    }

    pub fn auto_texture_transitions_enabled(&self) -> bool {
        *self.0.auto_texture_transitions_enabled.lock().unwrap()
    }

    pub fn tracked_texture_binding_count(&self) -> usize {
        self.0.texture_bindings.lock().unwrap().len()
    }

    fn update_texture_bindings(&self, resource_containers: &[&dyn ResourceContainer]) {
        let mut bindings = HashMap::new();
        for set_bindings in self.0.descriptor_sets_bindings.values() {
            for binding in set_bindings.values() {
                if let Some(state) = compute_texture_binding_state(binding.descriptor_type) {
                    if let Some(texture) = find_unique_texture(resource_containers, &binding.name) {
                        bindings.insert(
                            binding.name.clone(),
                            ComputeTextureBinding { texture, state },
                        );
                    }
                }
            }
        }
        let mut tracked_bindings = self.0.texture_bindings.lock().unwrap();
        tracked_bindings.retain(|name, _| name.starts_with(MANUAL_TEXTURE_BINDING_PREFIX));
        tracked_bindings.extend(bindings);
    }

    fn update_texture_binding_from_write(&self, set_no: u32, write: &WriteDescriptorSet<'_>) {
        let key = manual_texture_binding_key(set_no, write.binding(), write.array_element());
        let mut bindings = self.0.texture_bindings.lock().unwrap();
        if let (Some(texture), Some(state)) = (
            write.texture(),
            compute_texture_binding_state(write.descriptor_type()),
        ) {
            bindings.insert(
                key,
                ComputeTextureBinding {
                    texture: texture.clone(),
                    state,
                },
            );
        } else {
            bindings.remove(&key);
        }
    }

    fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        if !*self.0.auto_texture_transitions_enabled.lock().unwrap() {
            return;
        }
        let tracker = self.0.resource_state_tracker.lock().unwrap().clone();
        let bindings = self.0.texture_bindings.lock().unwrap().clone();
        for binding in bindings.values() {
            let image = binding.texture.get_image();
            tracker.transition_image_layers(
                cmdbuf,
                image,
                0,
                image.get_desc().array_len,
                binding.state,
            );
        }
    }

    fn record_bind_descriptor_sets(
        &self,
        cmdbuf: &CommandBuffer,
        descriptor_sets: &[DescriptorSet],
        first_set: u32,
    ) {
        let descriptor_sets = descriptor_sets
            .iter()
            .map(|s| s.as_raw())
            .collect::<Vec<_>>();

        self.0.device.cmd_bind_descriptor_sets_compute_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            first_set,
            &descriptor_sets,
        );
    }

    fn record_bind(&self, cmdbuf: &CommandBuffer) {
        self.0
            .device
            .cmd_bind_pipeline_compute_raw(cmdbuf.as_raw(), self.0.pipeline);
    }

    fn record_push_constants(&self, cmdbuf: &CommandBuffer, push_constants: &[u8]) {
        self.0.device.cmd_push_constants_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            vk::ShaderStageFlags::COMPUTE,
            0,
            push_constants,
        );
    }

    fn record_dispatch(&self, cmdbuf: &CommandBuffer, dispatch_size: [u32; 3]) {
        let x = (dispatch_size[0] as f32 / self.0.workgroup_size[0] as f32).ceil() as u32;
        let y = (dispatch_size[1] as f32 / self.0.workgroup_size[1] as f32).ceil() as u32;
        let z = (dispatch_size[2] as f32 / self.0.workgroup_size[2] as f32).ceil() as u32;
        self.0.device.cmd_dispatch_raw(cmdbuf.as_raw(), x, y, z);
    }

    /// Record the compute pipeline into the command buffer.
    ///
    /// This function will bind the pipeline, bind the descriptor sets, push the push constants, and dispatch the compute work.
    pub fn record(
        &self,
        cmdbuf: &CommandBuffer,
        dispatch_extent: Extent3D,
        push_constants: Option<&[u8]>,
    ) {
        self.record_texture_transitions(cmdbuf);
        self.record_bind(cmdbuf);
        if !self.0.descriptor_sets.lock().unwrap().is_empty() {
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap(), 0);
        }
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_dispatch(
            cmdbuf,
            [
                dispatch_extent.width,
                dispatch_extent.height,
                dispatch_extent.depth,
            ],
        );
    }

    /// Record the compute pipeline into the command buffer.
    ///
    /// This function will bind the pipeline, bind the descriptor sets, push the push constants, and dispatch the compute work.
    pub fn record_indirect(
        &self,
        cmdbuf: &CommandBuffer,
        buffer: &Buffer,
        push_constants: Option<&[u8]>,
    ) {
        self.record_texture_transitions(cmdbuf);
        self.record_bind(cmdbuf);
        if !self.0.descriptor_sets.lock().unwrap().is_empty() {
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap(), 0);
        }
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_dispatch_indirect(cmdbuf, buffer);
    }

    fn record_dispatch_indirect(&self, cmdbuf: &CommandBuffer, buffer: &Buffer) {
        self.0
            .device
            .cmd_dispatch_indirect_raw(cmdbuf.as_raw(), buffer.as_raw(), 0);
    }
}

fn compute_texture_binding_state(descriptor_type: vk::DescriptorType) -> Option<ResourceState> {
    match descriptor_type {
        vk::DescriptorType::STORAGE_IMAGE => Some(ResourceState::storage_image_read_write()),
        vk::DescriptorType::COMBINED_IMAGE_SAMPLER | vk::DescriptorType::SAMPLED_IMAGE => {
            Some(ResourceState::shader_read_only())
        }
        _ => None,
    }
}

fn manual_texture_binding_key(set_no: u32, binding: u32, array_element: u32) -> String {
    format!("{MANUAL_TEXTURE_BINDING_PREFIX}{set_no}:{binding}:{array_element}")
}

fn find_unique_texture(
    resource_containers: &[&dyn ResourceContainer],
    name: &str,
) -> Option<Texture> {
    let mut found = None;
    for container in resource_containers {
        if let Some(texture) = container.get_texture(name) {
            assert!(
                found.is_none(),
                "Resource '{}' found in multiple texture containers",
                name
            );
            found = Some(texture.clone());
        }
    }
    found
}
