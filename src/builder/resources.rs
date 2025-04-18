use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};
use ash::vk;
use glam::UVec3;

pub struct InternalSharedResources {
    pub raw_atlas_tex: Texture,
    pub fragment_list: Buffer,
}

impl InternalSharedResources {
    fn new(
        device: Device,
        allocator: Allocator,
        voxel_dim: UVec3,
        chunk_dim: UVec3,
        frag_list_maker_sm: &ShaderModule,
    ) -> Self {
        let raw_atlas_dim: UVec3 = voxel_dim * chunk_dim;
        let tex_desc = TextureDesc {
            extent: raw_atlas_dim.to_array(),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let raw_atlas_tex = Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc);

        let max_possible_voxel_count = voxel_dim.x * voxel_dim.y * voxel_dim.z;
        let fragment_list_buf_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentList")
            .unwrap();
        let buf_size = fragment_list_buf_layout.get_size() * max_possible_voxel_count;
        log::debug!("Fragment list buffer size: {} MB", buf_size / 1024 / 1024);

        // uninitialized for now, but is guaranteed to be filled by shader before use
        let fragment_list = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            buf_size as _,
        );

        Self {
            raw_atlas_tex,
            fragment_list,
        }
    }
}

pub struct ExternalSharedResources {
    pub octree_data: Buffer,
}

impl ExternalSharedResources {
    fn new(device: Device, allocator: Allocator, octree_buffer_size: u64) -> Self {
        let octree_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            octree_buffer_size,
        );
        Self { octree_data }
    }
}

pub struct ChunkInitResources {
    pub chunk_init_info: Buffer,
}

impl ChunkInitResources {
    pub fn new(device: Device, allocator: Allocator, chunk_init_sm: &ShaderModule) -> Self {
        let chunk_init_info_layout = chunk_init_sm.get_buffer_layout("U_ChunkInitInfo").unwrap();
        let chunk_init_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            chunk_init_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self { chunk_init_info }
    }
}

pub struct FragListResources {
    pub voxel_dim_indirect: Buffer,
    pub frag_list_maker_info: Buffer,
    pub frag_list_build_result: Buffer,
}

impl FragListResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        frag_init_buffers_sm: &ShaderModule,
        frag_list_maker_sm: &ShaderModule,
    ) -> Self {
        let voxel_dim_indirect_layout = frag_init_buffers_sm
            .get_buffer_layout("B_VoxelDimIndirect")
            .unwrap();
        let voxel_dim_indirect = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            voxel_dim_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let frag_list_maker_info_layout = frag_list_maker_sm
            .get_buffer_layout("U_FragListMakerInfo")
            .unwrap();
        let frag_list_maker_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            frag_list_maker_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let frag_list_build_result = frag_init_buffers_sm
            .get_buffer_layout("B_FragListBuildResult")
            .unwrap();
        let frag_list_build_result = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            frag_list_build_result.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self {
            voxel_dim_indirect,
            frag_list_maker_info,
            frag_list_build_result,
        }
    }
}

pub struct OctreeResources {
    pub octree_build_info: Buffer,
    pub voxel_count_indirect: Buffer,
    pub alloc_number_indirect: Buffer,
    pub octree_alloc_info: Buffer,
    pub counter: Buffer,
    pub octree_build_result: Buffer,

    pub octree_data_single: Buffer,
}

impl OctreeResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        octree_init_buffers_sm: &ShaderModule,
    ) -> Self {
        let octree_build_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildInfo")
            .unwrap();
        let octree_build_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let voxel_count_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_VoxelCountIndirect")
            .unwrap();
        let voxel_count_indirect = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            voxel_count_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let alloc_number_indirect_layout = octree_init_buffers_sm
            .get_buffer_layout("B_AllocNumberIndirect")
            .unwrap();
        let alloc_number_indirect = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            alloc_number_indirect_layout.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::INDIRECT_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let counter_layout = octree_init_buffers_sm
            .get_buffer_layout("B_Counter")
            .unwrap();
        let counter = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            counter_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_alloc_info_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeAllocInfo")
            .unwrap();
        let octree_alloc_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_alloc_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let octree_build_result_layout = octree_init_buffers_sm
            .get_buffer_layout("B_OctreeBuildResult")
            .unwrap();
        let octree_build_result = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            octree_build_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuToCpu,
        );

        let single_octree_buffer_size = 50 * 1024 * 1024; // 50 MB
        let octree_data_single = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            single_octree_buffer_size as _,
        );

        Self {
            octree_build_info,
            voxel_count_indirect,
            alloc_number_indirect,
            counter,
            octree_alloc_info,
            octree_build_result,

            octree_data_single,
        }
    }
}

pub struct Resources {
    pub internal_shared_resources: InternalSharedResources,
    pub external_shared_resources: ExternalSharedResources,
    pub frag_list: FragListResources,
    pub chunk_init: ChunkInitResources,
    pub octree: OctreeResources,
}

impl Resources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        shader_compiler: &crate::util::ShaderCompiler,
        voxel_dim: UVec3,
        chunk_dim: UVec3,
        octree_buffer_size: u64,
    ) -> Self {
        // Load all needed shader modules for buffer layouts
        let chunk_init_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/chunk_init/chunk_init.comp",
            "main",
        )
        .unwrap();

        let frag_init_buffers_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/frag_list_builder/init_buffers.comp",
            "main",
        )
        .unwrap();

        let frag_list_maker_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/frag_list_builder/frag_list_maker.comp",
            "main",
        )
        .unwrap();

        let octree_init_buffers_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/octree/init_buffers.comp",
            "main",
        )
        .unwrap();

        let internal_shared_resources = InternalSharedResources::new(
            device.clone(),
            allocator.clone(),
            voxel_dim,
            chunk_dim,
            &frag_list_maker_sm,
        );

        let external_shared_resources =
            ExternalSharedResources::new(device.clone(), allocator.clone(), octree_buffer_size);

        let chunk_init = ChunkInitResources::new(device.clone(), allocator.clone(), &chunk_init_sm);

        let frag_list = FragListResources::new(
            device.clone(),
            allocator.clone(),
            &frag_init_buffers_sm,
            &frag_list_maker_sm,
        );

        let octree =
            OctreeResources::new(device.clone(), allocator.clone(), &octree_init_buffers_sm);

        Self {
            internal_shared_resources,
            external_shared_resources,
            chunk_init,
            frag_list,
            octree,
        }
    }

    pub fn chunk_init_info(&self) -> &Buffer {
        &self.chunk_init.chunk_init_info
    }

    pub fn voxel_dim_indirect(&self) -> &Buffer {
        &self.frag_list.voxel_dim_indirect
    }

    pub fn raw_atlas_tex(&self) -> &Texture {
        &self.internal_shared_resources.raw_atlas_tex
    }

    pub fn fragment_list(&self) -> &Buffer {
        &self.internal_shared_resources.fragment_list
    }

    pub fn frag_list_maker_info(&self) -> &Buffer {
        &self.frag_list.frag_list_maker_info
    }

    pub fn octree_data_single(&self) -> &Buffer {
        &self.octree.octree_data_single
    }

    pub fn octree_data(&self) -> &Buffer {
        &self.external_shared_resources.octree_data
    }

    pub fn frag_list_build_result(&self) -> &Buffer {
        &self.frag_list.frag_list_build_result
    }

    pub fn octree_build_info(&self) -> &Buffer {
        &self.octree.octree_build_info
    }

    pub fn voxel_count_indirect(&self) -> &Buffer {
        &self.octree.voxel_count_indirect
    }

    pub fn alloc_number_indirect(&self) -> &Buffer {
        &self.octree.alloc_number_indirect
    }

    pub fn counter(&self) -> &Buffer {
        &self.octree.counter
    }

    pub fn octree_alloc_info(&self) -> &Buffer {
        &self.octree.octree_alloc_info
    }

    pub fn octree_build_result(&self) -> &Buffer {
        &self.octree.octree_build_result
    }
}
