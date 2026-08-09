use super::{
    descriptor_runtime::ReflectedDescriptorRuntime, DescriptorGenerationDraft,
    DescriptorSetGeneration,
};
use crate::{
    Buffer, CommandBuffer, DescriptorPool, Device, Extent3D, FrameRetirement, PipelineLayout,
    ResourceContainer, ShaderModule,
};
use anyhow::Result;
use ash::vk;
use std::{ops::Deref, sync::Arc};

struct ComputePipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    workgroup_size: [u32; 3],
    descriptors: ReflectedDescriptorRuntime,
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
        Self::new_with_initialization(
            device,
            shader_module,
            descriptor_pool,
            resource_containers,
            true,
        )
    }

    /// Creates a pipeline and allocates its descriptor sets without writing any resources.
    /// Call an explicit initialization method before recording commands.
    pub fn new_uninitialized(
        device: &Device,
        shader_module: &ShaderModule,
        descriptor_pool: &DescriptorPool,
    ) -> Self {
        Self::new_with_initialization(device, shader_module, descriptor_pool, &[], false)
    }

    fn new_with_initialization(
        device: &Device,
        shader_module: &ShaderModule,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
        initialize: bool,
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
        let descriptors = ReflectedDescriptorRuntime::from_reflection(
            format!("compute shader {}", shader_module.get_module_name()),
            descriptor_pool,
            &pipeline_layout,
            &descriptor_sets_bindings,
        )
        .expect("shader reflection must produce a valid descriptor runtime");

        let pipeline_instance = Self(Arc::new(ComputePipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout: pipeline_layout.clone(),
            workgroup_size,
            descriptors,
        }));

        if initialize {
            pipeline_instance
                .0
                .descriptors
                .initialize_resources(resource_containers)
                .expect("automatic descriptor initialization must resolve reflected resources");
        }
        pipeline_instance
    }

    /// Completes creation-time descriptor initialization.
    ///
    /// This is intentionally separate from runtime generation preparation: every reflected
    /// binding must resolve here, and no prefixed or otherwise implicit exception is accepted.
    pub fn initialize_descriptor_resources(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        self.0.descriptors.initialize_resources(resource_containers)
    }

    /// Initializes the reflected descriptor set containing `binding_name`.
    ///
    /// The resource name is the stable semantic anchor; the Vulkan set number remains private to
    /// the pipeline/reflection implementation.
    pub fn initialize_descriptor_set_resources(
        &self,
        binding_name: &str,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        self.0
            .descriptors
            .initialize_set_resources(binding_name, resource_containers)
    }

    pub fn initialize_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        self.0.descriptors.initialize_descriptor(name, resource)
    }

    /// Starts an owned runtime descriptor draft cloned from the active generation.
    pub fn begin_descriptor_draft(&self) -> Result<DescriptorGenerationDraft> {
        self.0.descriptors.begin_draft()
    }

    /// Publishes a prepared draft by infallibly swapping the active generation.
    pub fn publish_descriptor_draft(
        &self,
        name: &'static str,
        generation: u64,
        draft: DescriptorGenerationDraft,
    ) -> FrameRetirement {
        self.0.descriptors.publish_draft(name, generation, draft)
    }

    /// Publishes a previously staged generation and returns the old generation for completion-
    /// scoped retirement.
    pub fn publish_descriptor_sets(
        &self,
        name: &'static str,
        generation: u64,
        pending: DescriptorSetGeneration,
    ) -> FrameRetirement {
        self.0
            .descriptors
            .publish_generation(name, generation, pending)
    }

    /// Starts a new frame for manually-bound buffer descriptor sets.
    pub fn begin_transient_descriptor_frame(&self, frame_slot: usize) {
        self.0.descriptors.begin_transient_frame(frame_slot);
    }

    fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        self.0.descriptors.record_texture_transitions(cmdbuf);
    }

    pub fn tracked_texture_binding_count(&self) -> usize {
        self.0.descriptors.tracked_texture_binding_count()
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

    fn bind_active_descriptor_sets(&self, cmdbuf: &CommandBuffer, excluded_set_no: Option<u32>) {
        self.0
            .descriptors
            .bind_active(excluded_set_no, |first_set, sets| {
                self.0.device.cmd_bind_descriptor_sets_compute_raw(
                    cmdbuf.as_raw(),
                    self.0.pipeline_layout.as_raw(),
                    first_set,
                    sets,
                );
            });
    }

    /// Records a dispatch using a per-dispatch descriptor set addressed by reflected names.
    pub fn record_with_descriptors(
        &self,
        cmdbuf: &CommandBuffer,
        descriptors: &[(&str, super::DescriptorResource<'_>)],
        dispatch_extent: Extent3D,
        push_constants: Option<&[u8]>,
    ) -> Result<()> {
        self.record_texture_transitions(cmdbuf);
        self.record_bind(cmdbuf);
        let descriptor_set = self.0.descriptors.prepare_transient_set(descriptors)?;
        self.bind_active_descriptor_sets(cmdbuf, Some(descriptor_set.set_no()));
        self.0.device.cmd_bind_descriptor_sets_compute_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            descriptor_set.set_no(),
            &[descriptor_set.as_raw()],
        );
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
        Ok(())
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
        self.bind_active_descriptor_sets(cmdbuf, None);
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
        self.bind_active_descriptor_sets(cmdbuf, None);
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
