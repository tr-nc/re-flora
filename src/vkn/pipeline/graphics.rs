use crate::vkn::{CommandBuffer, DescriptorSetLayout, Device, PipelineLayout, ShaderModule};
use ash::vk;
use std::{ops::Deref, sync::Arc};

struct GraphicsPipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
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

impl GraphicsPipeline {
    pub fn new(
        device: &Device,
        create_info: vk::GraphicsPipelineCreateInfo,
        pipeline_layout: PipelineLayout,
    ) -> Self {
        let pipeline = Self::create_pipeline(device, &create_info);
        Self(Arc::new(GraphicsPipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout,
        }))
    }

    pub fn as_raw(&self) -> vk::Pipeline {
        self.0.pipeline
    }

    pub fn get_layout(&self) -> &PipelineLayout {
        &self.0.pipeline_layout
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

    pub fn from_shader_modules(
        device: &Device,
        vert_shader_module: &ShaderModule,
        frag_shader_module: &ShaderModule,
        render_pass: vk::RenderPass,
        descriptor_set_layouts: Option<&[DescriptorSetLayout]>,
        push_constant_ranges: Option<&[vk::PushConstantRange]>,
    ) -> Self {
        let pipeline_layout =
            PipelineLayout::new(device, descriptor_set_layouts, push_constant_ranges);

        let vert_state_info = vert_shader_module.get_shader_stage_create_info();
        let frag_state_info = frag_shader_module.get_shader_stage_create_info();
        let shader_states_infos = [vert_state_info, frag_state_info];

        let binding_desc = [vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(20)
            .input_rate(vk::VertexInputRate::VERTEX)];
        let attribute_desc = [
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(8),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R8G8B8A8_UNORM)
                .offset(16),
        ];
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_desc)
            .vertex_attribute_descriptions(&attribute_desc);

        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::CLOCKWISE)
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

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
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
            .alpha_blend_op(vk::BlendOp::ADD)];
        let color_blending_info = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let depth_stencil_state_create_info = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(false)
            .depth_write_enable(false)
            .depth_compare_op(vk::CompareOp::ALWAYS)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic_states_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_states_infos)
            .render_pass(render_pass)
            .layout(pipeline_layout.as_raw())
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .rasterization_state(&rasterizer_info)
            .viewport_state(&viewport_info)
            .multisample_state(&multisampling_info)
            .color_blend_state(&color_blending_info)
            .depth_stencil_state(&depth_stencil_state_create_info)
            .dynamic_state(&dynamic_states_info);

        Self::new(device, pipeline_info, pipeline_layout)
    }

    pub fn record_bind(&self, cmdbuf: &CommandBuffer) {
        unsafe {
            self.0.device.cmd_bind_pipeline(
                cmdbuf.as_raw(),
                vk::PipelineBindPoint::GRAPHICS,
                self.0.pipeline,
            );
        }
    }

    pub fn record_draw(
        &self,
        cmdbuf: &CommandBuffer,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.0.device.cmd_draw(
                cmdbuf.as_raw(),
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            );
        }
    }
}
