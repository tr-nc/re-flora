use super::{
    descriptor_set_utils, transient_descriptor_sets::TransientDescriptorSets,
    DescriptorBindingPlan, DescriptorGenerationDraft, DescriptorSetGeneration,
};
use crate::{
    Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
    Extent3D, FrameRetirement, PipelineLayout, ResourceContainer, ShaderModule,
};
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex},
    sync::atomic::{AtomicBool, Ordering},
};

struct ComputePipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    workgroup_size: [u32; 3],
    descriptor_pool: DescriptorPool,
    descriptor_sets: Mutex<DescriptorSetGeneration>,
    transient_descriptor_sets: Mutex<TransientDescriptorSets>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_binding_plan: DescriptorBindingPlan,
    descriptor_draft_active: Arc<AtomicBool>,
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
        let descriptor_binding_plan = DescriptorBindingPlan::from_reflection(
            format!("compute shader {}", shader_module.get_module_name()),
            &descriptor_sets_bindings,
        )
        .expect("shader reflection must produce a valid descriptor binding plan");

        let pipeline_instance = Self(Arc::new(ComputePipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout: pipeline_layout.clone(),
            workgroup_size,
            descriptor_pool: descriptor_pool.clone(),
            descriptor_sets: Mutex::new(
                descriptor_set_utils::allocate_descriptor_sets(
                    descriptor_pool,
                    &pipeline_layout,
                    &descriptor_sets_bindings,
                )
                .expect("descriptor sets must be allocatable from reflected layouts"),
            ),
            transient_descriptor_sets: Mutex::new(TransientDescriptorSets::default()),
            descriptor_sets_bindings,
            descriptor_binding_plan,
            descriptor_draft_active: Arc::new(AtomicBool::new(false)),
        }));

        if initialize {
            descriptor_set_utils::initialize_descriptor_sets(
                resource_containers,
                &pipeline_instance.0.descriptor_sets_bindings,
                &pipeline_instance.0.descriptor_sets,
            )
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
        descriptor_set_utils::initialize_descriptor_sets(
            resource_containers,
            &self.0.descriptor_sets_bindings,
            &self.0.descriptor_sets,
        )
    }

    pub fn initialize_descriptor_set_resources(
        &self,
        set_no: u32,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        descriptor_set_utils::initialize_descriptor_set(
            set_no,
            resource_containers,
            &self.0.descriptor_sets_bindings,
            &self.0.descriptor_sets,
        )
    }

    pub fn initialize_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        let write = self.0.descriptor_binding_plan.make_write(name, resource)?;
        let set_no = self.0.descriptor_binding_plan.binding(name)?.set_no();
        let mut write = write;
        let descriptor_sets = self.0.descriptor_sets.lock().unwrap();
        descriptor_sets
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?
            .perform_writes(std::slice::from_mut(&mut write));
        Ok(())
    }

    /// Starts an owned runtime descriptor draft cloned from the active generation.
    pub fn begin_descriptor_draft(&self) -> Result<DescriptorGenerationDraft> {
        self.0
            .descriptor_draft_active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| anyhow::anyhow!("descriptor draft already exists for {}", self.0.descriptor_binding_plan.pipeline_name()))?;

        let active = self.0.descriptor_sets.lock().unwrap();
        let mut set_nos = self.0.descriptor_sets_bindings.keys().copied().collect::<Vec<_>>();
        set_nos.sort_unstable();
        let result = (|| {
            let mut pending = DescriptorSetGeneration::empty();
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
            Ok(DescriptorGenerationDraft::new(
                pending,
                self.0.descriptor_binding_plan.clone(),
                self.0.descriptor_draft_active.clone(),
            ))
        })();
        if result.is_err() {
            self.0.descriptor_draft_active.store(false, Ordering::Release);
        }
        result
    }

    /// Publishes a prepared draft by infallibly swapping the active generation.
    pub fn publish_descriptor_draft(
        &self,
        name: &'static str,
        generation: u64,
        draft: DescriptorGenerationDraft,
    ) -> FrameRetirement {
        self.publish_descriptor_sets(name, generation, draft.into_generation())
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

    /// Starts a new frame for manually-bound buffer descriptor sets.
    pub fn begin_transient_descriptor_frame(&self, frame_slot: usize) {
        self.0
            .transient_descriptor_sets
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
            run.push(
                descriptor_sets
                    .get(&set_no)
                    .expect("descriptor set location was collected from the generation")
                    .as_raw(),
            );
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

    fn next_transient_descriptor_set(
        &self,
        descriptors: &[(&str, super::DescriptorResource<'_>)],
    ) -> Result<(u32, DescriptorSet)> {
        let first_name = descriptors
            .first()
            .ok_or_else(|| anyhow::anyhow!("transient descriptor set requires at least one resource"))?
            .0;
        let set_no = self.0.descriptor_binding_plan.binding(first_name)?.set_no();
        let layout = self
            .0
            .pipeline_layout
            .get_descriptor_set_layouts()
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?;
        let descriptor_set = self
            .0
            .transient_descriptor_sets
            .lock()
            .unwrap()
            .next_descriptor_set(set_no, &self.0.descriptor_pool, layout, "ComputePipeline")?;
        let mut writes = Vec::with_capacity(descriptors.len());
        for (name, resource) in descriptors {
            let binding = self.0.descriptor_binding_plan.binding(name)?;
            anyhow::ensure!(
                binding.set_no() == set_no,
                "transient descriptor resources must use one descriptor set; '{}' is in set {} but '{}' is in set {}",
                first_name,
                set_no,
                name,
                binding.set_no(),
            );
            writes.push(self.0.descriptor_binding_plan.make_write(name, *resource)?);
        }
        descriptor_set.perform_writes(&mut writes);
        Ok((set_no, descriptor_set))
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
        let (set_no, descriptor_set) = self.next_transient_descriptor_set(descriptors)?;
        let active = self.0.descriptor_sets.lock().unwrap();
        if !active.is_empty() {
            self.record_bind_descriptor_sets(cmdbuf, &active);
        }
        self.0.device.cmd_bind_descriptor_sets_compute_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            set_no,
            &[descriptor_set.as_raw()],
        );
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_dispatch(
            cmdbuf,
            [dispatch_extent.width, dispatch_extent.height, dispatch_extent.depth],
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
