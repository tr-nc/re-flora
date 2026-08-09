use super::{
    descriptor_runtime::ReflectedDescriptorRuntime, DescriptorGenerationDraft,
    DescriptorSetGeneration,
};
use crate::{
    CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
    FormatOverride, FrameRetirement, MergeWithEq, PipelineLayout, RenderPass, RenderPassDesc,
    ResourceContainer, ShaderModule, Viewport,
};
use anyhow::Result;
use ash::vk;
use std::{collections::HashMap, ops::Deref, sync::Arc};

struct GraphicsPipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    descriptors: ReflectedDescriptorRuntime,
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
        let descriptors = ReflectedDescriptorRuntime::from_reflection(
            format!(
                "graphics shaders {} + {}",
                vert_shader_module.get_module_name(),
                frag_shader_module.get_module_name()
            ),
            descriptor_pool,
            &pipeline_layout,
            &descriptor_sets_bindings,
        )
        .expect("shader reflection must produce a valid descriptor runtime");

        let pipeline_instance = Self(Arc::new(GraphicsPipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout: pipeline_layout.clone(),
            descriptors,
        }));

        if initialize {
            pipeline_instance
                .0
                .descriptors
                .initialize_resources(resource_containers)
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

    /// Completes creation-time descriptor initialization.
    ///
    /// Every reflected binding must resolve; runtime generation preparation is a separate API.
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

    /// Starts a new frame for transient descriptor sets.
    ///
    /// The graphics pipeline keeps one descriptor-set sequence per frame slot so
    /// descriptors can be updated and reused after that slot's fence has been
    /// waited. Call this once before recording any
    /// `record_indexed_with_descriptors` draws for the frame.
    pub fn begin_transient_descriptor_frame(&self, frame_slot: usize) {
        self.0.descriptors.begin_transient_frame(frame_slot);
    }

    /// Declare image uses for tracked texture descriptors used by this graphics pipeline.
    ///
    /// Call this before beginning the render pass that will draw with the pipeline;
    /// Vulkan image barriers cannot be recorded from arbitrary draw helpers once a
    /// render pass is active.
    pub fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        self.0.descriptors.record_texture_transitions(cmdbuf);
    }

    /// Binds an externally owned descriptor set at the reflected set containing `name`.
    ///
    /// This is the adapter for standalone resources such as egui textures. The caller supplies
    /// the semantic resource name; the numeric set location remains owned by the pipeline plan.
    pub fn bind_descriptor_set(
        &self,
        cmdbuf: &CommandBuffer,
        name: &str,
        descriptor_set: &DescriptorSet,
    ) -> Result<()> {
        self.0.descriptors.bind_standalone_descriptor(
            name,
            descriptor_set,
            |set_no, descriptor_set| {
                self.0.device.cmd_bind_descriptor_sets_graphics_raw(
                    cmdbuf.as_raw(),
                    self.0.pipeline_layout.as_raw(),
                    set_no,
                    &[descriptor_set],
                );
            },
        )
    }

    pub fn tracked_texture_binding_count(&self) -> usize {
        self.0.descriptors.tracked_texture_binding_count()
    }

    fn record_bind_descriptor_sets(&self, cmdbuf: &CommandBuffer, excluded_set_no: Option<u32>) {
        self.0
            .descriptors
            .bind_active(excluded_set_no, |first_set, sets| {
                self.0.device.cmd_bind_descriptor_sets_graphics_raw(
                    cmdbuf.as_raw(),
                    self.0.pipeline_layout.as_raw(),
                    first_set,
                    sets,
                );
            });
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
        self.record_bind_descriptor_sets(cmdbuf, None);
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
        self.record_bind_descriptor_sets(cmdbuf, None);
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

    /// Records an indexed draw using a per-draw descriptor set addressed by reflected names.
    #[allow(clippy::too_many_arguments)]
    pub fn record_indexed_with_descriptors(
        &self,
        cmdbuf: &CommandBuffer,
        descriptors: &[(&str, super::DescriptorResource<'_>)],
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
        push_constants: Option<&PushConstantInfo>,
    ) -> Result<()> {
        self.record_bind(cmdbuf);
        let descriptor_set = self.0.descriptors.prepare_transient_set(descriptors)?;
        self.record_bind_descriptor_sets(cmdbuf, Some(descriptor_set.set_no()));
        self.0.device.cmd_bind_descriptor_sets_graphics_raw(
            cmdbuf.as_raw(),
            self.0.pipeline_layout.as_raw(),
            descriptor_set.set_no(),
            &[descriptor_set.as_raw()],
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
        Ok(())
    }

    /// Allocates a standalone descriptor set for a reflected resource, for example an egui
    /// texture.  The set is owned by the returned `DescriptorSet` and is independent of the
    /// pipeline's active generation.
    pub fn allocate_transient_descriptor(
        &self,
        name: &str,
        resource: super::DescriptorResource<'_>,
    ) -> Result<DescriptorSet> {
        self.0
            .descriptors
            .allocate_standalone_descriptor(name, resource)
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
