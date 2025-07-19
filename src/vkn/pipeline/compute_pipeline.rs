use crate::{
    resource::ResourceContainer,
    vkn::{
        Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
        Extent3D, PipelineLayout, ShaderModule, WriteDescriptorSet,
    },
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
    pub fn new(device: &Device, shader_module: &ShaderModule) -> Self {
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

        Self(Arc::new(ComputePipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout,
            workgroup_size,
            descriptor_sets: Mutex::new(vec![]),
            descriptor_sets_bindings,
        }))
    }

    pub fn set_descriptor_sets(&self, descriptor_sets: Vec<DescriptorSet>) {
        let mut guard = self.0.descriptor_sets.lock().unwrap();
        *guard = descriptor_sets;
    }

    /// Creates descriptor sets for the compute pipeline.
    ///
    /// This function allocates descriptor sets from the descriptor pool based on the
    /// pipeline's descriptor set layout. After creation, it calls auto_update_descriptor_sets
    /// to populate the descriptor sets with the provided resources.
    ///
    /// # Arguments
    /// * `descriptor_pool` - The descriptor pool to allocate sets from
    /// * `resource_containers` - Array of resource containers containing buffers and textures
    ///
    /// # Returns
    /// Result indicating success or failure of descriptor set creation
    pub fn auto_create_descriptor_sets(
        &self,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        let mut descriptor_sets = Vec::new();
        let mut sorted_sets: Vec<_> = self.0.descriptor_sets_bindings.iter().collect();
        sorted_sets.sort_by_key(|(set_no, _)| *set_no);

        // Allocate descriptor sets from the pool
        for (set_no, _) in sorted_sets {
            let descriptor_set = descriptor_pool
                .allocate_set(&self.0.pipeline_layout.get_descriptor_set_layouts()[set_no])
                .unwrap();
            descriptor_sets.push(descriptor_set);
        }

        // Store the allocated descriptor sets
        self.set_descriptor_sets(descriptor_sets);

        // Update the descriptor sets with the provided resources
        self.auto_update_descriptor_sets(resource_containers)?;

        Ok(())
    }

    /// Updates existing descriptor sets with new resources.
    ///
    /// This function updates the descriptor sets that were previously created with
    /// auto_create_descriptor_sets. It finds the appropriate resources from the
    /// resource containers and writes them to the descriptor sets.
    ///
    /// # Arguments
    /// * `resource_containers` - Array of resource containers containing buffers and textures
    ///
    /// # Returns
    /// Result indicating success or failure of descriptor set update
    pub fn auto_update_descriptor_sets(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        let descriptor_sets = self.0.descriptor_sets.lock().unwrap();
        let mut sorted_sets: Vec<_> = self.0.descriptor_sets_bindings.iter().collect();
        sorted_sets.sort_by_key(|(set_no, _)| *set_no);

        for (set_idx, (_, bindings)) in sorted_sets.iter().enumerate() {
            let descriptor_set = &descriptor_sets[set_idx];

            for (_binding_idx, binding) in bindings.iter() {
                // Find the exact resource for this binding across all resource containers
                let mut found_buffer_containers = Vec::new();
                let mut found_texture_containers = Vec::new();

                for (i, container) in resource_containers.iter().enumerate() {
                    if let Some(_) = container.get_buffer(&binding.name) {
                        found_buffer_containers.push(i);
                    }
                    if let Some(_) = container.get_texture(&binding.name) {
                        found_texture_containers.push(i);
                    }
                }

                // Ensure that only one resource container has that resource
                let total_found = found_buffer_containers.len() + found_texture_containers.len();
                if total_found == 0 {
                    return Err(anyhow::anyhow!("Resource not found: {}", binding.name));
                } else if total_found > 1 {
                    return Err(anyhow::anyhow!(
                        "Resource '{}' found in multiple containers: {} buffer containers, {} texture containers",
                        binding.name,
                        found_buffer_containers.len(),
                        found_texture_containers.len()
                    ));
                }

                // Each resource may be Buffer or Texture, but not both
                if !found_buffer_containers.is_empty() && !found_texture_containers.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Resource '{}' found as both Buffer and Texture",
                        binding.name
                    ));
                }

                // Write the descriptor set based on the found resource
                if let Some(container_idx) = found_buffer_containers.first() {
                    let resource = resource_containers[*container_idx]
                        .get_buffer(&binding.name)
                        .unwrap();
                    descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
                        binding.no, resource,
                    )]);
                } else if let Some(container_idx) = found_texture_containers.first() {
                    let resource = resource_containers[*container_idx]
                        .get_texture(&binding.name)
                        .unwrap();
                    descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
                        binding.no,
                        binding.descriptor_type,
                        resource,
                        vk::ImageLayout::GENERAL,
                    )]);
                }
            }
        }
        Ok(())
    }

    pub fn get_layout(&self) -> &PipelineLayout {
        &self.0.pipeline_layout
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

        unsafe {
            self.0.device.cmd_bind_descriptor_sets(
                cmdbuf.as_raw(),
                vk::PipelineBindPoint::COMPUTE,
                self.0.pipeline_layout.as_raw(),
                first_set,
                &descriptor_sets,
                &[],
            );
        }
    }

    fn record_bind(&self, cmdbuf: &CommandBuffer) {
        unsafe {
            self.0.device.cmd_bind_pipeline(
                cmdbuf.as_raw(),
                vk::PipelineBindPoint::COMPUTE,
                self.0.pipeline,
            );
        }
    }

    fn record_push_constants(&self, cmdbuf: &CommandBuffer, push_constants: &[u8]) {
        unsafe {
            self.0.device.cmd_push_constants(
                cmdbuf.as_raw(),
                self.0.pipeline_layout.as_raw(),
                vk::ShaderStageFlags::COMPUTE,
                0,
                push_constants,
            );
        }
    }

    fn record_dispatch(&self, cmdbuf: &CommandBuffer, dispatch_size: [u32; 3]) {
        let x = (dispatch_size[0] as f32 / self.0.workgroup_size[0] as f32).ceil() as u32;
        let y = (dispatch_size[1] as f32 / self.0.workgroup_size[1] as f32).ceil() as u32;
        let z = (dispatch_size[2] as f32 / self.0.workgroup_size[2] as f32).ceil() as u32;
        unsafe {
            self.0.device.cmd_dispatch(cmdbuf.as_raw(), x, y, z);
        }
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
        unsafe {
            self.0
                .device
                .cmd_dispatch_indirect(cmdbuf.as_raw(), buffer.as_raw(), 0);
        }
    }
}
