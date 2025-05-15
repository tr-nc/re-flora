mod resources;

use std::time::Instant;

use ash::vk;
use glam::{Vec2, Vec3};
pub use resources::*;

use crate::{
    util::{ShaderCompiler, BENCH},
    vkn::{
        build_or_update_blas, build_tlas, execute_one_time_command, Allocator, Buffer,
        CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet, PlainMemberTypeWithData,
        ShaderModule, StructMemberDataBuilder, StructMemberDataReader, VulkanContext,
        WriteDescriptorSet,
    },
};

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    allocator: Allocator,
    accel_struct_device: ash::khr::acceleration_structure::Device,

    resources: AccelStructResources,
    descriptor_pool: DescriptorPool,

    _make_unit_grass_ds: DescriptorSet,
    _make_unit_grass_ppl: ComputePipeline,

    instance_maker_ds: DescriptorSet,
    instance_maker_ppl: ComputePipeline,

    make_unit_grass_cmdbuf: CommandBuffer,

    instances: Vec<(Vec3, u32)>,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        tlas_instance_cap: u64,
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
            2000,
            2000 * 3,
            tlas_instance_cap,
        );

        let make_unit_grass_ppl = ComputePipeline::from_shader_module(device, &make_unit_grass_sm);
        let instance_maker_ppl = ComputePipeline::from_shader_module(device, &instance_maker_sm);

        let make_unit_grass_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &make_unit_grass_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
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
            &instance_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        instance_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.instance_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.instance_descriptor),
            WriteDescriptorSet::new_buffer_write(2, &resources.tlas_instances),
        ]);

        let make_unit_grass_cmdbuf =
            create_make_unit_grass_cmdbuf(&vulkan_ctx, &make_unit_grass_ppl, &make_unit_grass_ds);

        return Self {
            vulkan_ctx,
            allocator,
            accel_struct_device,

            resources,
            descriptor_pool,

            _make_unit_grass_ppl: make_unit_grass_ppl,
            _make_unit_grass_ds: make_unit_grass_ds,

            instance_maker_ppl,
            instance_maker_ds,

            make_unit_grass_cmdbuf,

            instances: make_instances(),
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

        fn make_instances() -> Vec<(Vec3, u32)> {
            let mut instances = Vec::new();
            let range_min = Vec3::new(0.0, 0.5, 0.0);
            let range_max = Vec3::new(1.0, 0.5, 1.0);
            let generate_count = 5000;
            for _ in 0..generate_count {
                let x = rand::random::<f32>() * (range_max.x - range_min.x) + range_min.x;
                let y = rand::random::<f32>() * (range_max.y - range_min.y) + range_min.y;
                let z = rand::random::<f32>() * (range_max.z - range_min.z) + range_min.z;
                let pos = Vec3::new(x, y, z);
                instances.push((pos, 0));
            }
            instances
        }
    }

    pub fn build(&mut self, bend_dir_and_strength: Vec2) {
        self.build_or_update_grass_blas(bend_dir_and_strength, true);
        self.build_tlas();
    }

    pub fn update(&mut self, bend_dir_and_strength: Vec2) {
        self.build_or_update_grass_blas(bend_dir_and_strength, false);
        self.build_tlas();
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
                .get_data_u8();
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

    fn build_tlas(&mut self) {
        // build the buffer first
        // this step takes 90% of the time! optimize it later
        let t1 = Instant::now();
        self.build_tlas_instances(
            self.resources
                .blas
                .as_ref()
                .expect("BLAS not found")
                .get_device_address(),
        );
        BENCH
            .lock()
            .unwrap()
            .record("build_tlas_instances", t1.elapsed());

        let t2 = Instant::now();
        // then build the tlas using the buffer
        let tlas = build_tlas(
            &self.vulkan_ctx,
            &self.allocator,
            self.accel_struct_device.clone(),
            &self.resources.tlas_instances,
            self.instances.len() as u32,
            vk::GeometryFlagsKHR::OPAQUE,
        );
        BENCH.lock().unwrap().record("build_tlas", t2.elapsed());

        self.resources.tlas = Some(tlas);
    }

    fn build_tlas_instances(&mut self, blas_device_address: u64) {
        update_buffers(&self.resources, &self.instances, blas_device_address);

        let x_dispatch_count = self.instances.len() as u32;
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
                    .record_dispatch(&cmdbuf, [x_dispatch_count, 1, 1]);
            },
        );

        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &AccelStructResources,
            instances: &[(Vec3, u32)],
            blas_device_address: u64,
        ) {
            let instance_count = instances.len() as u32;

            update_instance_info(
                &resources.instance_info,
                instance_count,
                blas_device_address,
            );

            update_instance_descriptor(instances, &resources.instance_descriptor);

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
                    .get_data_u8();
                instance_info_buf.fill_with_raw_u8(&data).unwrap();
            }

            fn update_instance_descriptor(
                instances: &[(Vec3, u32)],
                instance_descriptor_buf: &Buffer,
            ) {
                const SCALE: f32 = 1.0 / 256.0;
                for (i, (pos, custom_idx)) in instances.iter().enumerate() {
                    let data = StructMemberDataBuilder::from_buffer(instance_descriptor_buf)
                        .set_field(
                            "data.position",
                            PlainMemberTypeWithData::Vec3(pos.to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.rotation",
                            PlainMemberTypeWithData::Vec3([0.0, 0.0, 0.0]),
                        )
                        .unwrap()
                        .set_field(
                            "data.scale",
                            PlainMemberTypeWithData::Vec3([SCALE, SCALE, SCALE]),
                        )
                        .unwrap()
                        .set_field(
                            "data.custom_idx",
                            PlainMemberTypeWithData::UInt(*custom_idx as u32),
                        )
                        .unwrap()
                        .get_data_u8();
                    instance_descriptor_buf
                        .fill_element_with_raw_u8(&data, i as u64)
                        .unwrap();
                }
            }
        }
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
