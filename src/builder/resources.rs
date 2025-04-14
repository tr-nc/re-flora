use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule};
use ash::vk;
use glam::UVec3;

pub struct ChunkInitResources {
    pub chunk_build_info: Buffer,
    pub raw_voxels: Buffer,
}

impl ChunkInitResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_init_sm: &ShaderModule,
        chunk_res: UVec3,
    ) -> Self {
        let chunk_build_info_layout = chunk_init_sm.get_buffer_layout("U_ChunkBuildInfo").unwrap();
        let chunk_build_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            chunk_build_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let raw_voxels_size: u64 = chunk_res.x as u64 * chunk_res.y as u64 * chunk_res.z as u64;
        let raw_voxels = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            raw_voxels_size,
        );

        Self {
            chunk_build_info,
            raw_voxels,
        }
    }
}

pub struct FragListResources {
    pub fragment_list_info: Buffer,
    pub fragment_list: Buffer,
}

impl FragListResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        frag_list_maker_sm: &ShaderModule,
        chunk_res: UVec3,
    ) -> Self {
        let fragment_list_info_layout = frag_list_maker_sm
            .get_buffer_layout("B_FragmentListInfo")
            .unwrap();
        let fragment_list_info = Buffer::from_struct_layout(
            device.clone(),
            allocator.clone(),
            fragment_list_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let max_possible_voxel_count = chunk_res.x * chunk_res.y * chunk_res.z;
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
            fragment_list_info,
            fragment_list,
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
    pub octree_data: Buffer,
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
            gpu_allocator::MemoryLocation::GpuOnly,
        );

        let one_giga = 1 * 1024 * 1024 * 1024;
        log::debug!(
            "Octree data buffer size: {} GB",
            one_giga / 1024 / 1024 / 1024
        );
        let octree_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            gpu_allocator::MemoryLocation::GpuOnly,
            one_giga as _,
        );

        Self {
            octree_build_info,
            voxel_count_indirect,
            alloc_number_indirect,
            counter,
            octree_alloc_info,
            octree_build_result,
            octree_data,
        }
    }
}

pub struct Resources {
    pub chunk_init: ChunkInitResources,
    pub frag_list: FragListResources,
    pub octree: OctreeResources,
}

impl Resources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        shader_compiler: &crate::util::compiler::ShaderCompiler,
        chunk_res: UVec3,
    ) -> Self {
        // Load all needed shader modules for buffer layouts
        let chunk_init_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/chunk_init/chunk_init.comp",
            "main",
        )
        .unwrap();

        let frag_list_maker_sm = ShaderModule::from_glsl(
            &device,
            shader_compiler,
            "shader/builder/frag_list_maker/frag_list_maker.comp",
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

        // Create all resource containers
        let chunk_init =
            ChunkInitResources::new(device.clone(), allocator.clone(), &chunk_init_sm, chunk_res);

        let frag_list = FragListResources::new(
            device.clone(),
            allocator.clone(),
            &frag_list_maker_sm,
            chunk_res,
        );

        let octree =
            OctreeResources::new(device.clone(), allocator.clone(), &octree_init_buffers_sm);

        Self {
            chunk_init,
            frag_list,
            octree,
        }
    }

    // Convenience methods to get resources for external access
    pub fn raw_voxels(&self) -> &Buffer {
        &self.chunk_init.raw_voxels
    }

    pub fn chunk_build_info(&self) -> &Buffer {
        &self.chunk_init.chunk_build_info
    }

    pub fn fragment_list_info(&self) -> &Buffer {
        &self.frag_list.fragment_list_info
    }

    pub fn fragment_list(&self) -> &Buffer {
        &self.frag_list.fragment_list
    }

    pub fn octree_build_info(&self) -> &Buffer {
        &self.octree.octree_build_info
    }

    pub fn octree_data(&self) -> &Buffer {
        &self.octree.octree_data
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
