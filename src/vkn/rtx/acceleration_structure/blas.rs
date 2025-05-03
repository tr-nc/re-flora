use super::resources::Resources;
use crate::{
    util::ShaderCompiler,
    vkn::{
        rtx::acceleration_structure::utils::{build_acc, create_acc, query_properties},
        Allocator, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet, ShaderModule,
        VulkanContext, WriteDescriptorSet,
    },
};
use ash::{
    khr,
    vk::{self},
};

pub struct Blas {
    acc_device: khr::acceleration_structure::Device,
    vert_maker_ppl: ComputePipeline,

    blas: vk::AccelerationStructureKHR,
    resources: Resources,
}

// TODO: refactor this after testing
impl Drop for Blas {
    fn drop(&mut self) {
        unsafe {
            self.acc_device
                .destroy_acceleration_structure(self.blas, None);
        }
    }
}

impl Blas {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        descriptor_pool: DescriptorPool,
        acc_device: khr::acceleration_structure::Device,
        shader_compiler: &ShaderCompiler,
    ) -> Self {
        let device = vulkan_ctx.device();

        let vert_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/vert_maker.comp",
            "main",
        )
        .unwrap();

        let resources = Resources::new(device.clone(), allocator.clone(), &vert_maker_sm);

        let vert_maker_ppl = ComputePipeline::from_shader_module(device, &vert_maker_sm);
        let vert_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &vert_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        vert_maker_ds.perform_writes(&mut[
            WriteDescriptorSet::new_buffer_write(0, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(1, &resources.indices),
        ]);

        // TODO: maybe cache this later
        let vert_maker_cmdbuf =
            create_vert_maker_cmdbuf(vulkan_ctx, &vert_maker_ppl, &vert_maker_ds);

        vert_maker_cmdbuf.submit(&vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_ctx.get_general_queue());

        let geom = make_geom(&resources);

        const PRIMITIVE_COUNT: u32 = 12; // TODO: this should be read back later

        let (blas_size, scratch_buf_size) = query_properties(
            &acc_device,
            geom,
            &[PRIMITIVE_COUNT],
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1,
        );

        let blas = create_acc(
            device,
            &allocator,
            &acc_device,
            blas_size,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        );

        build_acc(
            vulkan_ctx,
            allocator,
            scratch_buf_size,
            geom,
            &acc_device,
            blas,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1,
            PRIMITIVE_COUNT,
        );

        return Self {
            acc_device,
            vert_maker_ppl,
            blas,
            resources,
        };

        fn create_vert_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            vert_maker_ppl: &ComputePipeline,
            vert_maker_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(true);

            vert_maker_ppl.record_bind(&cmdbuf);
            vert_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(vert_maker_ds),
                0,
            );
            vert_maker_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            cmdbuf.end();
            return cmdbuf;
        }

        fn make_geom(resources: &Resources) -> vk::AccelerationStructureGeometryKHR {
            let triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
                vertex_format: vk::Format::R32G32B32_SFLOAT,
                vertex_data: vk::DeviceOrHostAddressConstKHR {
                    device_address: resources.vertices.device_address(),
                },
                vertex_stride: 4 * 3, // TODO: or 4*4?
                max_vertex: 7, // maxVertex is the number of vertices in vertexData minus one.
                index_type: vk::IndexType::UINT32,
                index_data: vk::DeviceOrHostAddressConstKHR {
                    device_address: resources.indices.device_address(),
                },
                transform_data: vk::DeviceOrHostAddressConstKHR { device_address: 0 },
                ..Default::default()
            };

            return vk::AccelerationStructureGeometryKHR {
                geometry_type: vk::GeometryTypeKHR::TRIANGLES,
                geometry: vk::AccelerationStructureGeometryDataKHR {
                    triangles: triangles_data,
                },
                flags: vk::GeometryFlagsKHR::OPAQUE,
                ..Default::default()
            };
        }
    }

    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        self.blas
    }
}
