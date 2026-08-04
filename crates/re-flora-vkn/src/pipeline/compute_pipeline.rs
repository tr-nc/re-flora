use super::{
    descriptor_set_utils, manual_buffer_descriptor_sets::ManualBufferDescriptorSets,
    DescriptorBindingPlan,
};
use crate::{
    Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
    Extent3D, FrameRetirement, PipelineLayout, ResourceContainer, ShaderModule,
    WriteDescriptorSet,
};
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex},
};

pub type DescriptorSetGeneration = HashMap<u32, DescriptorSet>;

struct ComputePipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    workgroup_size: [u32; 3],
    descriptor_pool: DescriptorPool,
    descriptor_sets: Mutex<DescriptorSetGeneration>,
    pending_descriptor_sets: Mutex<Option<DescriptorSetGeneration>>,
    manual_buffer_descriptor_sets: Mutex<ManualBufferDescriptorSets>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_binding_plan: DescriptorBindingPlan,
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
    pub fn descriptor_binding_plan(&self) -> &DescriptorBindingPlan {
        &self.0.descriptor_binding_plan
    }

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
        let descriptor_binding_plan = DescriptorBindingPlan::from_reflection(
            format!("compute shader {}", shader_module.get_module_name()),
            &descriptor_sets_bindings,
        )
        .expect("shader reflection must produce a valid descriptor binding plan");

        let pipeline_instance = Self(Arc::new(ComputePipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout,
            workgroup_size,
            descriptor_pool: descriptor_pool.clone(),
            descriptor_sets: Mutex::new(HashMap::new()),
            pending_descriptor_sets: Mutex::new(None),
            manual_buffer_descriptor_sets: Mutex::new(ManualBufferDescriptorSets::default()),
            descriptor_sets_bindings,
            descriptor_binding_plan,
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
        Ok(())
    }

    pub fn initialize_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        let write = self.0.descriptor_binding_plan.make_write(name, resource)?;
        let set_no = self.0.descriptor_binding_plan.binding(name)?.set_no();
        self.initialize_descriptor_set_checked(set_no, write)
    }

    pub fn write_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        let write = self.0.descriptor_binding_plan.make_write(name, resource)?;
        let set_no = self.0.descriptor_binding_plan.binding(name)?.set_no();
        self.write_descriptor_set_checked(set_no, write)
    }

    /// Fork the current descriptor generation so runtime writes cannot mutate an in-flight set.
    pub fn begin_descriptor_generation(&self) -> Result<()> {
        let active = self.0.descriptor_sets.lock().unwrap();
        let mut set_nos = self.0.descriptor_sets_bindings.keys().copied().collect::<Vec<_>>();
        set_nos.sort_unstable();
        let mut pending = HashMap::new();
        for set_no in set_nos {
            let set = active.get(&set_no).ok_or_else(|| {
                anyhow::anyhow!(
                    "descriptor generation for {} is missing reflected set {}",
                    self.0.descriptor_binding_plan.pipeline_name(),
                    set_no
                )
            })?;
            let layout = self
                .0
                .pipeline_layout
                .get_descriptor_set_layouts()
                .get(&set_no)
                .ok_or_else(|| anyhow::anyhow!("descriptor set layout {set_no} is not reflected"))?;
            pending.insert(set_no, set.fork(&self.0.descriptor_pool, layout)?);
        }
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
        let pending = self.take_staged_descriptor_sets();
        Some(self.publish_descriptor_sets(name, generation, pending))
    }

    /// Takes a fully written staged generation without making it active. Callers may prepare a
    /// generation while its resources are still private, then publish the owned set at the exact
    /// visibility boundary later.
    pub fn take_staged_descriptor_sets(&self) -> DescriptorSetGeneration {
        self.0
            .pending_descriptor_sets
            .lock()
            .unwrap()
            .take()
            .expect("taking a descriptor generation requires begin_descriptor_generation")
    }

    /// Publishes a previously staged generation and returns the old generation for completion-
    /// scoped retirement.
    pub fn publish_descriptor_sets(
        &self,
        name: &'static str,
        generation: u64,
        pending: DescriptorSetGeneration,
    ) -> FrameRetirement {
        let old = std::mem::replace(&mut *self.0.descriptor_sets.lock().unwrap(), pending);
        FrameRetirement::new(name, generation, old)
    }

    /// Initialize a descriptor set before any command can reference it.
    ///
    /// Runtime updates must use `begin_descriptor_generation` and
    /// `write_descriptor_set`; direct mutation of the active generation is
    /// intentionally unavailable.
    pub fn initialize_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        self.initialize_descriptor_set_checked(set_no, write)
        .expect("descriptor initialization must match shader reflection");
    }

    fn initialize_descriptor_set_checked(
        &self,
        set_no: u32,
        write: WriteDescriptorSet,
    ) -> Result<()> {
        assert!(
            self.0.pending_descriptor_sets.lock().unwrap().is_none(),
            "descriptor initialization cannot run while a generation is staged"
        );
        let mut write = write;
        self.0
            .descriptor_binding_plan
            .validate_write(set_no, &write)?;
        let guard = self.0.descriptor_sets.lock().unwrap();
        guard
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?
            .perform_writes(std::slice::from_mut(&mut write));
        Ok(())
    }

    /// Write a descriptor into the staged generation only.
    pub fn write_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        self.write_descriptor_set_checked(set_no, write)
        .expect("descriptor write must match shader reflection");
    }

    fn write_descriptor_set_checked(
        &self,
        set_no: u32,
        write: WriteDescriptorSet,
    ) -> Result<()> {
        let mut write = write;
        let pending = self.0.pending_descriptor_sets.lock().unwrap();
        let descriptor_sets = pending
            .as_ref()
            .expect("runtime descriptor writes require begin_descriptor_generation");
        self.0.descriptor_binding_plan.validate_write(set_no, &write)?;
        descriptor_sets
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?
            .perform_writes(std::slice::from_mut(&mut write));
        Ok(())
    }

    /// Starts a new frame for manually-bound buffer descriptor sets.
    pub fn begin_manual_buffer_frame(&self, frame_slot: usize) {
        self.0
            .manual_buffer_descriptor_sets
            .lock()
            .unwrap()
            .begin_frame(frame_slot);
    }

    fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        let descriptor_sets = self.0.descriptor_sets.lock().unwrap();
        for descriptor_set in descriptor_sets.values() {
            descriptor_set.record_image_uses(cmdbuf);
        }
    }

    pub fn tracked_texture_binding_count(&self) -> usize {
        self.0
            .descriptor_sets
            .lock()
            .unwrap()
            .values()
            .map(DescriptorSet::image_owner_count)
            .sum()
    }

    fn record_bind_descriptor_sets(
        &self,
        cmdbuf: &CommandBuffer,
        descriptor_sets: &DescriptorSetGeneration,
    ) {
        let mut set_nos = descriptor_sets.keys().copied().collect::<Vec<_>>();
        set_nos.sort_unstable();
        let mut run = Vec::new();
        let mut run_start = None;
        for set_no in set_nos {
            if let Some(start) = run_start {
                if set_no != start + run.len() as u32 {
                    self.0.device.cmd_bind_descriptor_sets_compute_raw(
                        cmdbuf.as_raw(),
                        self.0.pipeline_layout.as_raw(),
                        start,
                        &run,
                    );
                    run.clear();
                    run_start = Some(set_no);
                }
            } else {
                run_start = Some(set_no);
            }
            run.push(descriptor_sets[&set_no].as_raw());
        }
        if let Some(start) = run_start {
            self.0.device.cmd_bind_descriptor_sets_compute_raw(
                cmdbuf.as_raw(),
                self.0.pipeline_layout.as_raw(),
                start,
                &run,
            );
        }
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

    fn next_manual_buffer_descriptor_set_with_writes(
        &self,
        set_no: u32,
        buffers: &[(u32, &Buffer)],
    ) -> DescriptorSet {
        let layout = self
            .0
            .pipeline_layout
            .get_descriptor_set_layouts()
            .get(&set_no)
            .unwrap_or_else(|| panic!("Missing descriptor set layout {set_no}"));
        let descriptor_set = self
            .0
            .manual_buffer_descriptor_sets
            .lock()
            .unwrap()
            .next_descriptor_set(set_no, &self.0.descriptor_pool, layout, "ComputePipeline")
            .unwrap_or_else(|err| panic!("{err:#}"));
        let mut writes = buffers
            .iter()
            .map(|(binding, buffer)| WriteDescriptorSet::new_buffer_write(*binding, buffer))
            .collect::<Vec<_>>();
        descriptor_set.perform_writes(&mut writes);
        descriptor_set
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
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap());
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

    /// Records a compute dispatch with one descriptor set populated from per-dispatch buffers.
    pub fn record_with_manual_buffers(
        &self,
        cmdbuf: &CommandBuffer,
        manual_set_no: u32,
        manual_buffers: &[(u32, &Buffer)],
        dispatch_extent: Extent3D,
        push_constants: Option<&[u8]>,
    ) {
        self.record_texture_transitions(cmdbuf);
        self.record_bind(cmdbuf);
        let manual_set = self.next_manual_buffer_descriptor_set_with_writes(
            manual_set_no,
            manual_buffers,
        );
        {
            let descriptor_sets = self.0.descriptor_sets.lock().unwrap();
            if !descriptor_sets.is_empty() {
                self.record_bind_descriptor_sets(cmdbuf, &descriptor_sets);
            }
        }
        self.0.device.cmd_bind_descriptor_sets_compute_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            manual_set_no,
            &[manual_set.as_raw()],
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
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap());
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
