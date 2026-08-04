use super::{
    descriptor_set_utils, DescriptorBindingPlan, DescriptorGenerationDraft, DescriptorSetGeneration,
};
use crate::{
    Buffer, CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayout,
    DescriptorSetLayoutBinding, Device, FormatOverride, MergeWithEq, PipelineLayout, RenderPass,
    FrameRetirement, RenderPassDesc, ResourceContainer, ShaderModule,
    Viewport, WriteDescriptorSet,
};
use anyhow::{Context, Result};
use ash::vk;
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex},
    sync::atomic::{AtomicBool, Ordering},
};

struct GraphicsPipelineInner {
    device: Device,
    descriptor_pool: DescriptorPool,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    descriptor_sets: Mutex<DescriptorSetGeneration>,
    pending_descriptor_sets: Mutex<Option<DescriptorSetGeneration>>,
    manual_buffer_descriptor_sets: Mutex<ManualBufferDescriptorSets>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_binding_plan: DescriptorBindingPlan,
    descriptor_draft_active: Arc<AtomicBool>,
}

#[derive(Default)]
struct ManualBufferDescriptorSets {
    active_frame_slot: Option<usize>,
    next_slot: usize,
    frame_slots: Vec<ManualBufferDescriptorFrame>,
}

#[derive(Default)]
struct ManualBufferDescriptorFrame {
    slots: Vec<ManualBufferDescriptorSlot>,
}

struct ManualBufferDescriptorSlot {
    set_no: u32,
    descriptor_set: DescriptorSet,
}

impl ManualBufferDescriptorSets {
    fn begin_frame(&mut self, frame_slot: usize) {
        if self.frame_slots.len() <= frame_slot {
            self.frame_slots
                .resize_with(frame_slot + 1, ManualBufferDescriptorFrame::default);
        }
        self.active_frame_slot = Some(frame_slot);
        self.next_slot = 0;
    }

    fn next_descriptor_set(
        &mut self,
        set_no: u32,
        descriptor_pool: &DescriptorPool,
        layout: &DescriptorSetLayout,
    ) -> Result<DescriptorSet> {
        let frame_slot = self.active_frame_slot.expect(
            "GraphicsPipeline::begin_manual_buffer_frame must be called before record_indexed_with_manual_buffer",
        );
        let draw_slot = self.next_slot;
        self.next_slot += 1;

        let frame = self
            .frame_slots
            .get_mut(frame_slot)
            .expect("active manual descriptor frame slot was not initialized");
        if let Some(slot) = frame.slots.get(draw_slot) {
            if slot.set_no == set_no {
                return Ok(slot.descriptor_set.clone());
            }
        }

        let descriptor_set = descriptor_pool.allocate_set(layout).with_context(|| {
            format!(
                "failed to allocate manual buffer descriptor set for frame_slot={frame_slot} draw_slot={draw_slot} set={set_no}"
            )
        })?;
        let slot = ManualBufferDescriptorSlot {
            set_no,
            descriptor_set: descriptor_set.clone(),
        };
        if draw_slot == frame.slots.len() {
            frame.slots.push(slot);
        } else {
            frame.slots[draw_slot] = slot;
        }

        Ok(descriptor_set)
    }
}

impl Drop for GraphicsPipelineInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

#[derive(Clone)]
pub struct GraphicsPipeline(Arc<GraphicsPipelineInner>);

impl Deref for GraphicsPipeline {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.0.pipeline
    }
}

pub struct PushConstantInfo {
    pub shader_stage: vk::ShaderStageFlags,
    pub push_constants: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct GraphicsPipelineDesc {
    pub format_overrides: Vec<FormatOverride>,
    pub cull_mode: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
    pub depth_test_enable: bool,
    pub depth_write_enable: bool,
}

impl Default for GraphicsPipelineDesc {
    fn default() -> Self {
        Self {
            format_overrides: Vec::new(),
            cull_mode: vk::CullModeFlags::NONE,
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            depth_test_enable: false,
            depth_write_enable: false,
        }
    }
}

impl GraphicsPipeline {
    pub fn descriptor_binding_plan(&self) -> &DescriptorBindingPlan {
        &self.0.descriptor_binding_plan
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &Device,
        vert_shader_module: &ShaderModule,
        frag_shader_module: &ShaderModule,
        render_pass: &RenderPass,
        desc: &GraphicsPipelineDesc,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Self {
        Self::new_with_initialization(
            device,
            vert_shader_module,
            frag_shader_module,
            render_pass,
            desc,
            instance_rate_starting_location,
            descriptor_pool,
            resource_containers,
            true,
        )
    }

    /// Creates a pipeline and allocates its descriptor sets without writing any resources.
    /// Call an explicit initialization method before recording commands.
    #[allow(clippy::too_many_arguments)]
    pub fn new_uninitialized(
        device: &Device,
        vert_shader_module: &ShaderModule,
        frag_shader_module: &ShaderModule,
        render_pass: &RenderPass,
        desc: &GraphicsPipelineDesc,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
    ) -> Self {
        Self::new_with_initialization(
            device,
            vert_shader_module,
            frag_shader_module,
            render_pass,
            desc,
            instance_rate_starting_location,
            descriptor_pool,
            &[],
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_initialization(
        device: &Device,
        vert_shader_module: &ShaderModule,
        frag_shader_module: &ShaderModule,
        render_pass: &RenderPass,
        desc: &GraphicsPipelineDesc,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
        initialize: bool,
    ) -> Self {
        let vert_pipeline_layout = PipelineLayout::from_shader_module(device, vert_shader_module);
        let frag_pipeline_layout = PipelineLayout::from_shader_module(device, frag_shader_module);
        let pipeline_layout = vert_pipeline_layout.merge(&frag_pipeline_layout).unwrap();

        let vert_state_info = vert_shader_module.get_shader_stage_create_info();
        let frag_state_info = frag_shader_module.get_shader_stage_create_info();

        let shader_states_infos = [vert_state_info, frag_state_info];

        let (binding_descs, attribute_descs) = vert_shader_module
            .get_vertex_input_state(&desc.format_overrides, instance_rate_starting_location)
            .unwrap();

        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_descs)
            .vertex_attribute_descriptions(&attribute_descs);

        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(desc.cull_mode)
            .front_face(desc.front_face)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        let viewports = [Default::default()];
        let scissors = [Default::default()];
        let viewport_info = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);

        let multisampling_info = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_DST_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD);
        let color_blend_attachments =
            vec![
                color_blend_attachment;
                first_subpass_color_attachment_count(render_pass.get_desc())
            ];
        let color_blending_info = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let depth_stencil_state_create_info = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(desc.depth_test_enable)
            .depth_write_enable(desc.depth_write_enable)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic_states_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_states_infos)
            .render_pass(render_pass.as_raw())
            .layout(pipeline_layout.as_raw())
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .rasterization_state(&rasterizer_info)
            .viewport_state(&viewport_info)
            .multisample_state(&multisampling_info)
            .color_blend_state(&color_blending_info)
            .depth_stencil_state(&depth_stencil_state_create_info)
            .dynamic_state(&dynamic_states_info);

        let pipeline = Self::create_pipeline(device, &pipeline_info);

        let vert_descriptor_sets_bindings = vert_shader_module.get_descriptor_sets_bindings();
        let frag_descriptor_sets_bindings = frag_shader_module.get_descriptor_sets_bindings();
        let descriptor_sets_bindings = merge_descriptor_sets_bindings(
            &vert_descriptor_sets_bindings,
            &frag_descriptor_sets_bindings,
        )
        .unwrap();
        let descriptor_binding_plan = DescriptorBindingPlan::from_reflection(
            format!(
                "graphics shaders {} + {}",
                vert_shader_module.get_module_name(),
                frag_shader_module.get_module_name()
            ),
            &descriptor_sets_bindings,
        )
        .expect("shader reflection must produce a valid descriptor binding plan");

        let pipeline_instance = Self(Arc::new(GraphicsPipelineInner {
            device: device.clone(),
            descriptor_pool: descriptor_pool.clone(),
            pipeline,
            pipeline_layout: pipeline_layout.clone(),
            descriptor_sets: Mutex::new(
                descriptor_set_utils::allocate_descriptor_sets(
                    descriptor_pool,
                    &pipeline_layout,
                    &descriptor_sets_bindings,
                )
                .expect("descriptor sets must be allocatable from reflected layouts"),
            ),
            pending_descriptor_sets: Mutex::new(None),
            manual_buffer_descriptor_sets: Mutex::new(ManualBufferDescriptorSets::default()),
            descriptor_sets_bindings,
            descriptor_binding_plan,
            descriptor_draft_active: Arc::new(AtomicBool::new(false)),
        }));

        if initialize {
            descriptor_set_utils::auto_update_descriptor_sets(
                resource_containers,
                &pipeline_instance.0.descriptor_sets_bindings,
                &pipeline_instance.0.descriptor_sets,
            )
            .expect("automatic descriptor initialization must resolve reflected resources");
        }
        return pipeline_instance;

        fn merge_descriptor_sets_bindings(
            bindings_1: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
            bindings_2: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
        ) -> Result<HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>> {
            let mut merged = HashMap::new();
            // for unique set ids, just place the value inside the merged map
            for (set_id, bindings) in bindings_1 {
                if !bindings_2.contains_key(set_id) {
                    merged.insert(*set_id, bindings.clone());
                }
                // if the set id is present in both maps, merge the bindings
                else {
                    let set_bindings_merged = bindings.merge_with_eq(
                        bindings_2
                            .get(set_id)
                            .ok_or(anyhow::anyhow!("Set id not found"))?,
                    )?;
                    merged.insert(*set_id, set_bindings_merged);
                }
            }
            // for unique set ids in bindings_2, just place the value inside the merged map
            for (set_id, bindings) in bindings_2 {
                if !bindings_1.contains_key(set_id) {
                    merged.insert(*set_id, bindings.clone());
                }
            }
            Ok(merged)
        }
    }

    pub fn as_raw(&self) -> vk::Pipeline {
        self.0.pipeline
    }

    pub fn get_layout(&self) -> &PipelineLayout {
        &self.0.pipeline_layout
    }

    pub fn initialize_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        let write = self.0.descriptor_binding_plan.make_write(name, resource)?;
        let set_no = self.0.descriptor_binding_plan.binding(name)?.set_no();
        self.initialize_descriptor_set(set_no, write);
        Ok(())
    }

    pub fn write_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<()> {
        let write = self.0.descriptor_binding_plan.make_write(name, resource)?;
        let set_no = self.0.descriptor_binding_plan.binding(name)?.set_no();
        self.write_descriptor_set(set_no, write);
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

    /// Completes creation-time descriptor initialization.
    ///
    /// Every reflected binding must resolve; runtime generation preparation is a separate API.
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

    /// Starts a new frame for manually-bound buffer descriptor sets.
    ///
    /// The graphics pipeline keeps one descriptor-set sequence per frame slot so
    /// descriptors can be updated and reused after that slot's fence has been
    /// waited. Call this once before recording any
    /// `record_indexed_with_manual_buffer` draws for the frame.
    pub fn begin_manual_buffer_frame(&self, frame_slot: usize) {
        self.0
            .manual_buffer_descriptor_sets
            .lock()
            .unwrap()
            .begin_frame(frame_slot);
    }

    /// Declare image uses for tracked texture descriptors used by this graphics pipeline.
    ///
    /// Call this before beginning the render pass that will draw with the pipeline;
    /// Vulkan image barriers cannot be recorded from arbitrary draw helpers once a
    /// render pass is active.
    pub fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
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
                    self.0.device.cmd_bind_descriptor_sets_graphics_raw(
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
            self.0.device.cmd_bind_descriptor_sets_graphics_raw(
                cmdbuf.as_raw(),
                self.0.pipeline_layout.as_raw(),
                start,
                &run,
            );
        }
    }

    fn create_pipeline(
        device: &Device,
        create_info: &vk::GraphicsPipelineCreateInfo,
    ) -> vk::Pipeline {
        unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(create_info),
                    None,
                )
                .map_err(|e| e.1)
                .unwrap()[0]
        }
    }

    fn record_push_constants(&self, cmdbuf: &CommandBuffer, push_constants: &PushConstantInfo) {
        self.0.device.cmd_push_constants_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            push_constants.shader_stage,
            0,
            &push_constants.push_constants,
        );
    }

    pub fn record_bind(&self, cmdbuf: &CommandBuffer) {
        self.0
            .device
            .cmd_bind_pipeline_graphics_raw(cmdbuf.as_raw(), self.0.pipeline);
    }

    pub fn record_viewport_scissor(
        &self,
        cmdbuf: &CommandBuffer,
        viewport: Viewport,
        scissor: vk::Rect2D,
    ) {
        self.0
            .device
            .cmd_set_viewport_raw(cmdbuf.as_raw(), 0, &[viewport.as_raw()]);
        self.0
            .device
            .cmd_set_scissor_raw(cmdbuf.as_raw(), 0, &[scissor]);
    }

    pub fn record(
        &self,
        cmdbuf: &CommandBuffer,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
        push_constants: Option<&PushConstantInfo>,
    ) {
        self.record_bind(cmdbuf);
        if !self.0.descriptor_sets.lock().unwrap().is_empty() {
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap());
        }
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.0.device.cmd_draw_raw(
            cmdbuf.as_raw(),
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_indexed(
        &self,
        cmdbuf: &CommandBuffer,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
        push_constants: Option<&PushConstantInfo>,
    ) {
        self.record_bind(cmdbuf);
        if !self.0.descriptor_sets.lock().unwrap().is_empty() {
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap());
        }
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_draw_indexed(
            cmdbuf,
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_indexed_with_manual_buffer(
        &self,
        cmdbuf: &CommandBuffer,
        manual_set_no: u32,
        manual_binding: u32,
        manual_buffer: &Buffer,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
        push_constants: Option<&PushConstantInfo>,
    ) {
        self.record_bind(cmdbuf);
        let manual_set = self.next_manual_buffer_descriptor_set(
            manual_set_no,
            manual_binding,
            manual_buffer,
        );
        {
            let descriptor_sets = self.0.descriptor_sets.lock().unwrap();
            if !descriptor_sets.is_empty() {
                self.record_bind_descriptor_sets(cmdbuf, &descriptor_sets);
            }
        }
        self.0.device.cmd_bind_descriptor_sets_graphics_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            manual_set_no,
            &[manual_set.as_raw()],
        );
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_draw_indexed(
            cmdbuf,
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_indexed_with_manual_buffers(
        &self,
        cmdbuf: &CommandBuffer,
        manual_set_no: u32,
        manual_buffers: &[(u32, &Buffer)],
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
        push_constants: Option<&PushConstantInfo>,
    ) {
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
        self.0.device.cmd_bind_descriptor_sets_graphics_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            manual_set_no,
            &[manual_set.as_raw()],
        );
        if let Some(push_constants) = push_constants {
            self.record_push_constants(cmdbuf, push_constants);
        }
        self.record_draw_indexed(
            cmdbuf,
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        );
    }

    fn next_manual_buffer_descriptor_set(
        &self,
        set_no: u32,
        binding: u32,
        buffer: &Buffer,
    ) -> DescriptorSet {
        self.next_manual_buffer_descriptor_set_with_writes(set_no, &[(binding, buffer)])
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
            .unwrap_or_else(|| panic!("Missing descriptor set layout {}", set_no));
        let descriptor_set = self
            .0
            .manual_buffer_descriptor_sets
            .lock()
            .unwrap()
            .next_descriptor_set(set_no, &self.0.descriptor_pool, layout)
            .unwrap_or_else(|err| panic!("{err:#}"));
        let mut writes = buffers
            .iter()
            .map(|(binding, buffer)| WriteDescriptorSet::new_buffer_write(*binding, buffer))
            .collect::<Vec<_>>();
        descriptor_set.perform_writes(&mut writes);
        descriptor_set
    }

    fn record_draw_indexed(
        &self,
        cmdbuf: &CommandBuffer,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        self.0.device.cmd_draw_indexed_raw(
            cmdbuf.as_raw(),
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        );
    }

    /// Initialize a descriptor set before any command can reference it.
    ///
    /// Runtime updates must use `begin_descriptor_generation` and
    /// `write_descriptor_set`; direct mutation of the active generation is
    /// intentionally unavailable.
    pub fn initialize_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        assert!(
            self.0.pending_descriptor_sets.lock().unwrap().is_none(),
            "descriptor initialization cannot run while a generation is staged"
        );
        let mut write = write;
        self.0
            .descriptor_binding_plan
            .validate_write(set_no, &write)
            .expect("descriptor initialization must match shader reflection");
        let guard = self.0.descriptor_sets.lock().unwrap();
        guard
            .get(&set_no)
            .expect("descriptor set must exist for reflected set")
            .perform_writes(std::slice::from_mut(&mut write));
    }

    /// Write a descriptor into the staged generation only.
    pub fn write_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        let mut write = write;
        self.0
            .descriptor_binding_plan
            .validate_write(set_no, &write)
            .expect("descriptor write must match shader reflection");
        let pending = self.0.pending_descriptor_sets.lock().unwrap();
        let descriptor_sets = pending
            .as_ref()
            .expect("runtime descriptor writes require begin_descriptor_generation");
        descriptor_sets
            .get(&set_no)
            .expect("descriptor set must exist for reflected set")
            .perform_writes(std::slice::from_mut(&mut write));
    }

    /// Updates existing descriptor sets with new resources.
    #[allow(dead_code)]
    pub fn auto_update_descriptor_sets(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        if let Some(descriptor_sets) = self.0.pending_descriptor_sets.lock().unwrap().as_ref() {
            descriptor_set_utils::auto_update_descriptor_sets_on_sets(
                resource_containers,
                &self.0.descriptor_sets_bindings,
                descriptor_sets,
                true,
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

    /// Takes a fully written staged generation without making it active. This lets a caller
    /// prepare a private resource generation before the visibility boundary where it is published.
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
}

fn first_subpass_color_attachment_count(desc: &RenderPassDesc) -> usize {
    desc.subpasses
        .first()
        .map_or(0, |subpass| subpass.color_attachments.len())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttachmentReference, SubpassDesc, TextureLayout};

    #[test]
    fn color_blend_attachment_count_matches_first_subpass() {
        assert_eq!(
            first_subpass_color_attachment_count(&RenderPassDesc::default()),
            0
        );

        let desc = RenderPassDesc {
            subpasses: vec![SubpassDesc {
                color_attachments: vec![AttachmentReference {
                    attachment: 0,
                    layout: TextureLayout::COLOR_ATTACHMENT,
                }],
                depth_stencil_attachment: None,
            }],
            ..Default::default()
        };
        assert_eq!(first_subpass_color_attachment_count(&desc), 1);
    }
}
