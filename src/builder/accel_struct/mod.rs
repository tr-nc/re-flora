mod resources;

use crate::{
    util::ShaderCompiler,
    vkn::{
        build_or_update_blas, build_tlas, execute_one_time_command, Allocator, Buffer,
        CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet, PlainMemberTypeWithData,
        ShaderModule, StructMemberDataBuilder, StructMemberDataReader, VulkanContext,
        WriteDescriptorSet,
    },
};
use ash::vk;
use glam::Vec2;
pub use resources::*;

use super::SurfaceResources;

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    allocator: Allocator,
    accel_struct_device: ash::khr::acceleration_structure::Device,

    resources: AccelStructResources,

    #[allow(dead_code)]
    make_unit_grass_ds: DescriptorSet,
    #[allow(dead_code)]
    make_unit_grass_ppl: ComputePipeline,

    instance_maker_ds: DescriptorSet,
    instance_maker_ppl: ComputePipeline,

    make_unit_grass_cmdbuf: CommandBuffer,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        tlas_instance_cap: u64,
        surface_resources: &SurfaceResources,
    ) -> Self {
        let accel_struct_device = ash::khr::acceleration_structure::Device::new(
            &vulkan_ctx.instance(),
            &vulkan_ctx.device(),
        );

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let device = vulkan_ctx.device();
        let make_unit_grass_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/make_unit_grass.comp",
            "main",
        )
        .unwrap();
        let instance_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/instance_maker.comp",
            "main",
        )
        .unwrap();

        let resources = AccelStructResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &make_unit_grass_sm,
            &instance_maker_sm,
            100000,
            100000 * 3,
            tlas_instance_cap,
        );

        let make_unit_grass_ppl = ComputePipeline::new(device, &make_unit_grass_sm);
        let instance_maker_ppl = ComputePipeline::new(device, &instance_maker_sm);

        let make_unit_grass_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &make_unit_grass_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool.clone(),
        );
        make_unit_grass_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.make_unit_grass_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(2, &resources.indices),
            WriteDescriptorSet::new_buffer_write(3, &resources.blas_build_result),
        ]);
        let instance_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &instance_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool.clone(),
        );
        instance_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.instance_info),
            WriteDescriptorSet::new_buffer_write(1, &surface_resources.grass_instances),
            WriteDescriptorSet::new_buffer_write(2, &resources.tlas_instances),
        ]);

        let make_unit_grass_cmdbuf =
            create_make_unit_grass_cmdbuf(&vulkan_ctx, &make_unit_grass_ppl, &make_unit_grass_ds);

        return Self {
            vulkan_ctx,
            allocator,
            accel_struct_device,

            resources,

            make_unit_grass_ppl,
            make_unit_grass_ds,

            instance_maker_ppl,
            instance_maker_ds,

            make_unit_grass_cmdbuf,
        };

        fn create_make_unit_grass_cmdbuf(
            vulkan_ctx: &VulkanContext,
            make_unit_grass_ppl: &ComputePipeline,
            make_unit_grass_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            make_unit_grass_ppl.record_bind(&cmdbuf);
            make_unit_grass_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(make_unit_grass_ds),
                0,
            );
            make_unit_grass_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    pub fn build(&mut self, bend_dir_and_strength: Vec2, grass_instance_len: u32) {
        self.build_or_update_grass_blas(bend_dir_and_strength, true);

        self.build_tlas_instances(
            self.resources
                .blas
                .as_ref()
                .expect("BLAS not found")
                .get_device_address(),
            grass_instance_len,
        );

        self.build_tlas(grass_instance_len);
    }

    pub fn update(&mut self, bend_dir_and_strength: Vec2, grass_instance_len: u32) {
        self.build_or_update_grass_blas(bend_dir_and_strength, false);

        self.build_tlas_instances(
            self.resources
                .blas
                .as_ref()
                .expect("BLAS not found")
                .get_device_address(),
            grass_instance_len,
        );
        self.build_tlas(grass_instance_len);
    }

    fn build_or_update_grass_blas(&mut self, bend_dir_and_strength: Vec2, is_building: bool) {
        update_buffers(&self.resources.make_unit_grass_info, bend_dir_and_strength);

        self.make_unit_grass_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let (vertices_len, indices_len) =
            get_vertices_and_indices_len(&self.resources.blas_build_result);
        let primitives_len = indices_len / 3;

        let blas = build_or_update_blas(
            &self.vulkan_ctx,
            self.allocator.clone(),
            self.accel_struct_device.clone(),
            &self.resources.vertices,
            &self.resources.indices,
            // this controls the culling mode, it can be overwritten by gl_RayFlagsNoneEXT in rayQuery
            vk::GeometryFlagsKHR::OPAQUE,
            vertices_len,
            primitives_len,
            &self.resources.blas,
            true,
            is_building,
        );

        self.resources.blas = Some(blas);

        fn update_buffers(make_unit_grass_info: &Buffer, bend_dir_and_strength: Vec2) {
            let data = StructMemberDataBuilder::from_buffer(make_unit_grass_info)
                .set_field(
                    "bend_dir_and_strength",
                    PlainMemberTypeWithData::Vec2(bend_dir_and_strength.to_array()),
                )
                .unwrap()
                .build();
            make_unit_grass_info.fill_with_raw_u8(&data).unwrap();
        }

        /// Returns: (vertices_len, indices_len)
        fn get_vertices_and_indices_len(blas_build_result: &Buffer) -> (u32, u32) {
            let layout = &blas_build_result.get_layout().unwrap().root_member;
            let raw_data = blas_build_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);

            let vertices_len = reader.get_field("vertices_len").unwrap();
            let vertices_len = if let PlainMemberTypeWithData::UInt(val) = vertices_len {
                val
            } else {
                panic!("vertices_len is not a UInt");
            };

            let indices_len = reader.get_field("indices_len").unwrap();
            let indices_len = if let PlainMemberTypeWithData::UInt(val) = indices_len {
                val
            } else {
                panic!("indices_len is not a UInt");
            };

            return (vertices_len, indices_len);
        }
    }

    fn build_tlas(&mut self, grass_instance_len: u32) {
        // then build the tlas using the buffer
        let tlas = build_tlas(
            &self.vulkan_ctx,
            &self.allocator,
            self.accel_struct_device.clone(),
            &self.resources.tlas_instances,
            grass_instance_len,
            vk::GeometryFlagsKHR::OPAQUE,
        );
        self.resources.tlas = Some(tlas);
    }

    fn build_tlas_instances(&mut self, blas_device_address: u64, grass_instance_len: u32) {
        update_instance_info(
            &self.resources.instance_info,
            grass_instance_len,
            blas_device_address,
        );

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.instance_maker_ppl.record_bind(&cmdbuf);
                self.instance_maker_ppl.record_bind_descriptor_sets(
                    &cmdbuf,
                    std::slice::from_ref(&self.instance_maker_ds),
                    0,
                );
                self.instance_maker_ppl
                    .record_dispatch(&cmdbuf, [grass_instance_len, 1, 1]);
            },
        );

        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_instance_info(
            instance_info_buf: &Buffer,
            instance_count: u32,
            blas_device_address: u64,
        ) {
            // TODO: just use u64 directly with that extension in glsl
            let lower = (blas_device_address & 0xFFFF_FFFF) as u32;
            let upper = (blas_device_address >> 32) as u32;

            // blas_device_address
            let data = StructMemberDataBuilder::from_buffer(instance_info_buf)
                .set_field(
                    "instance_count",
                    PlainMemberTypeWithData::UInt(instance_count),
                )
                .unwrap()
                .set_field(
                    "blas_device_address",
                    PlainMemberTypeWithData::UVec2([lower, upper]),
                )
                .unwrap()
                .build();
            instance_info_buf.fill_with_raw_u8(&data).unwrap();
        }
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
