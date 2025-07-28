use super::descriptor_set_utils;
use crate::util::MergeWithEq;
use crate::vkn::WriteDescriptorSet;
use crate::{
    resource::ResourceContainer,
    vkn::{
        CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, Device,
        FormatOverride, PipelineLayout, RenderPass, ShaderModule, Viewport,
    },
};
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex},
};

struct GraphicsPipelineInner {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    descriptor_sets: Mutex<Vec<DescriptorSet>>,
    descriptor_sets_bindings: HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
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

        let pipeline_instance = Self(Arc::new(GraphicsPipelineInner {
            device: device.clone(),
            pipeline,
            pipeline_layout,
            descriptor_sets: Mutex::new(Vec::new()),
            descriptor_sets_bindings,
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
                vk::PipelineBindPoint::GRAPHICS,
                self.0.pipeline_layout.as_raw(),
                first_set,
                &descriptor_sets,
                &[],
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
        unsafe {
            self.0.device.cmd_push_constants(
                cmdbuf.as_raw(),
                self.0.pipeline_layout.as_raw(),
                push_constants.shader_stage,
                0,
                &push_constants.push_constants,
            );
        }
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

    pub fn record_viewport_scissor(
        &self,
        cmdbuf: &CommandBuffer,
        viewport: Viewport,
        scissor: vk::Rect2D,
    ) {
        unsafe {
            self.0
                .device
                .cmd_set_viewport(cmdbuf.as_raw(), 0, &[viewport.as_raw()]);
            self.0
                .device
                .cmd_set_scissor(cmdbuf.as_raw(), 0, &[scissor]);
        }
    }

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
            self.record_bind_descriptor_sets(cmdbuf, &self.0.descriptor_sets.lock().unwrap(), 0);
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

    fn record_draw_indexed(
        &self,
        cmdbuf: &CommandBuffer,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.0.device.cmd_draw_indexed(
                cmdbuf.as_raw(),
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    pub fn write_descriptor_set(&self, set_no: u32, write: WriteDescriptorSet) {
        let guard = self.0.descriptor_sets.lock().unwrap();
        guard[set_no as usize].perform_writes(&mut [write]);
    }

    /// Updates existing descriptor sets with new resources.
    #[allow(dead_code)]
    pub fn auto_update_descriptor_sets(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        descriptor_set_utils::auto_update_descriptor_sets(
            resource_containers,
            &self.0.descriptor_sets_bindings,
            &self.0.descriptor_sets,
        )
    }
}
