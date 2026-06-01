use super::descriptor_set_utils;
use crate::{
    Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
    Extent3D, PipelineLayout, ResourceContainer, ResourceState, ResourceStateTracker, ShaderModule,
    Texture, WriteDescriptorSet,
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
    descriptor_sets: Mutex<Vec<DescriptorSet>>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    texture_bindings: Mutex<HashMap<String, ComputeTextureBinding>>,
    resource_state_tracker: Mutex<ResourceStateTracker>,
    auto_texture_transitions_enabled: Mutex<bool>,
}

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
            descriptor_sets: Mutex::new(vec![]),
            descriptor_sets_bindings,
            texture_bindings: Mutex::new(HashMap::new()),
            resource_state_tracker: Mutex::new(ResourceStateTracker::automatic()),
            auto_texture_transitions_enabled: Mutex::new(false),
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
        descriptor_set_utils::auto_update_descriptor_sets(
            resource_containers,
            &self.0.descriptor_sets_bindings,
            &self.0.descriptor_sets,
        )?;
        self.update_texture_bindings(resource_containers);
        Ok(())
    }

    pub fn write_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        let guard = self.0.descriptor_sets.lock().unwrap();
        guard[set_no as usize].perform_writes(&mut [write]);
    }

    pub fn set_resource_state_tracker(&self, tracker: ResourceStateTracker) {
        *self.0.resource_state_tracker.lock().unwrap() = tracker;
    }

    pub fn set_auto_texture_transitions_enabled(&self, enabled: bool) {
        *self.0.auto_texture_transitions_enabled.lock().unwrap() = enabled;
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
        *self.0.texture_bindings.lock().unwrap() = bindings;
    }

    fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        if !*self.0.auto_texture_transitions_enabled.lock().unwrap() {
            return;
        }
        let tracker = self.0.resource_state_tracker.lock().unwrap().clone();
        let bindings = self.0.texture_bindings.lock().unwrap().clone();
        for binding in bindings.values() {
            tracker.transition_image(cmdbuf, binding.texture.get_image(), 0, binding.state);
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
        vk::DescriptorType::STORAGE_IMAGE
        | vk::DescriptorType::COMBINED_IMAGE_SAMPLER
        | vk::DescriptorType::SAMPLED_IMAGE => Some(ResourceState::general_shader_read_write()),
        _ => None,
    }
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
