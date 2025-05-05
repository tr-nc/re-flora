mod resources;
mod utils;

mod blas;
use ash::vk;
use blas::*;

mod tlas;
use resources::Resources;
pub use tlas::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        Allocator, Buffer, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet,
        PlainMemberTypeWithData, ShaderModule, StructMemberDataReader, VulkanContext,
        WriteDescriptorSet,
    },
};

pub struct AccelerationStructure {
    vert_maker_ppl: ComputePipeline,
    acc_device: ash::khr::acceleration_structure::Device,
    blas: Blas,

    resources: Resources,
    pub tlas: Tlas,
}

impl AccelerationStructure {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
    ) -> Self {
        let acc_device = ash::khr::acceleration_structure::Device::new(
            &vulkan_ctx.instance(),
            &vulkan_ctx.device(),
        );

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        //

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
        vert_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(1, &resources.indices),
            WriteDescriptorSet::new_buffer_write(2, &resources.vert_maker_result),
        ]);

        // 6 faces, 2 triangles per face, no sharing because we store voxel data inside the vertices
        const PRIMITIVE_COUNT_PER_VOXEL: u32 = 12;
        // 8 vertices per voxel
        const VERTICES_COUNT_PER_VOXEL: u32 = 8;

        // TODO: maybe cache this later
        let vert_maker_cmdbuf =
            create_vert_maker_cmdbuf(vulkan_ctx, &vert_maker_ppl, &vert_maker_ds);

        vert_maker_cmdbuf.submit(&vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_ctx.get_general_queue());

        let valid_voxel_count = read_back_valid_voxel_count(&resources);
        log::debug!("Valid voxel count: {}", valid_voxel_count);

        if valid_voxel_count == 0 {
            // TODO: handle this case properly
            panic!("No valid voxels found!");
        }

        let vertex_stride = get_vertex_stride(&resources);
        log::debug!("Vertex stride: {}", vertex_stride);

        let blas_geom = make_blas_geom(
            &resources,
            vertex_stride,
            VERTICES_COUNT_PER_VOXEL * valid_voxel_count - 1,
        );

        let primitive_count = valid_voxel_count * PRIMITIVE_COUNT_PER_VOXEL;
        let blas = Blas::new(
            vulkan_ctx,
            allocator.clone(),
            acc_device.clone(),
            blas_geom,
            primitive_count,
        );

        let tlas_geom = make_tlas_geom(&blas, &resources.tlas_instance_buffer);

        let tlas = Tlas::new(vulkan_ctx, allocator.clone(), acc_device.clone(), tlas_geom);

        log::debug!("blas address: {}", blas.get_device_address());
        log::debug!("tlas address: {}", tlas.get_device_address());

        return Self {
            vert_maker_ppl,
            acc_device,
            blas,
            tlas,
            resources,
        };

        fn read_back_valid_voxel_count(resources: &Resources) -> u32 {
            // read the reslt back
            let layout = &resources
                .vert_maker_result
                .get_layout()
                .unwrap()
                .root_member;
            let raw_data = resources.vert_maker_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);
            let field_val = reader.get_field("valid_voxel_count").unwrap();
            if let PlainMemberTypeWithData::UInt(val) = field_val {
                return val;
            } else {
                panic!("Invalid type for valid_voxel_count");
            }
        }

        fn get_vertex_stride(resources: &Resources) -> u64 {
            let layout = &resources.vertices.get_layout().unwrap().root_member;
            layout.get_size_bytes()
        }

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
            vert_maker_ppl.record_dispatch(&cmdbuf, [4, 4, 4]);

            cmdbuf.end();
            return cmdbuf;
        }

        fn make_blas_geom(
            resources: &Resources,
            vertex_stride: u64,
            max_vertex: u32,
        ) -> vk::AccelerationStructureGeometryKHR {
            let triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
                vertex_format: vk::Format::R32G32B32_SFLOAT,
                vertex_data: vk::DeviceOrHostAddressConstKHR {
                    device_address: resources.vertices.device_address(),
                },
                vertex_stride: vertex_stride, // the stride in bytes between each vertex
                max_vertex: max_vertex,       // the number of vertices in vertex_data minus one
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

        fn make_tlas_geom<'a>(
            blas: &'a Blas,
            instance_buffer: &'a Buffer,
        ) -> vk::AccelerationStructureGeometryKHR<'a> {
            let instance = vk::AccelerationStructureInstanceKHR {
                transform: vk::TransformMatrixKHR {
                    // matrix is a 3x4 row-major affine transformation matrix
                    matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                },
                // instanceCustomIndex is a 24-bit application-specified index value accessible to ray shaders in the InstanceCustomIndexKHR built-in
                // mask is an 8-bit visibility mask for the geometry. The instance may only be hit if Cull Mask & instance.mask != 0
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas.get_device_address(),
                },
            };

            instance_buffer
                .fill(&[instance])
                .expect("Failed to fill instance buffer");

            return vk::AccelerationStructureGeometryKHR {
                geometry_type: vk::GeometryTypeKHR::INSTANCES,
                flags: vk::GeometryFlagsKHR::OPAQUE,
                geometry: vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR {
                        array_of_pointers: vk::FALSE,
                        data: vk::DeviceOrHostAddressConstKHR {
                            device_address: instance_buffer.device_address(),
                        },
                        ..Default::default()
                    },
                },
                ..Default::default()
            };
        }
    }

    pub fn tlas(&self) -> &Tlas {
        &self.tlas
    }
}
