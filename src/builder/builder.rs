use std::collections::HashMap;

use super::BuilderResources;
use super::Chunk;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::IVec3;
use glam::UVec3;
use rayon::prelude::*;

pub struct Builder {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: BuilderResources,

    chunk_init_sm: ShaderModule,
    chunk_init_ppl: ComputePipeline,
    chunk_init_ds: DescriptorSet,
    descriptor_pool: DescriptorPool,

    chunk_res: UVec3,
    chunks: HashMap<IVec3, Chunk>,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_res: UVec3,
    ) -> Self {
        if chunk_res.x != chunk_res.y || chunk_res.y != chunk_res.z {
            log::error!("Resolution must be equal in all dimensions");
        }
        if chunk_res.x & (chunk_res.x - 1) != 0 {
            log::error!("Resolution must be a power of 2");
        }

        let chunk_init_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_init_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &chunk_init_sm);

        // reuse the descriptor pool later.
        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            chunk_init_ppl
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
        )
        .unwrap();

        let resources = BuilderResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            &chunk_init_sm,
            chunk_res,
        );

        let chunk_init_ds = Self::create_chunk_init_descriptor_set(
            descriptor_pool.clone(),
            &vulkan_context,
            &chunk_init_ppl,
            &resources,
        );

        Self {
            vulkan_context,
            allocator,
            resources,
            chunk_init_sm,
            chunk_init_ppl,
            chunk_init_ds,
            descriptor_pool,
            chunk_res,
            chunks: HashMap::new(),
        }
    }

    pub fn init_chunk(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        let chunk_data = self.generate_chunk_data(command_pool, self.chunk_res, chunk_pos);
        let chunk = Chunk {
            res: self.chunk_res,
            pos: chunk_pos,
            data: chunk_data,
        };
        self.chunks.insert(chunk_pos, chunk);
    }

    fn index_3d(x: u32, y: u32, z: u32, chunk_res: &[u32; 3]) -> usize {
        (z * chunk_res[0] * chunk_res[1] + y * chunk_res[0] + x) as usize
    }

    fn get_block_type(chunk_data: &Vec<u8>, chunk_res: &[u32; 3], uvi: [u32; 3]) -> u8 {
        let idx = Self::index_3d(uvi[0], uvi[1], uvi[2], chunk_res);
        chunk_data[idx]
    }

    fn is_empty_block(block_type: u8) -> bool {
        block_type == 0
    }

    /// Check if a block is occluded by its neighbours.
    fn is_occluded(uvi: &[u32; 3], chunk_res: &[u32; 3], chunk_data: &Vec<u8>) -> bool {
        let neighbours_to_check = [
            [-1, 0, 0],
            [1, 0, 0],
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
        ];

        for neighbour in neighbours_to_check.iter() {
            let cur = [
                uvi[0] as i32 + neighbour[0],
                uvi[1] as i32 + neighbour[1],
                uvi[2] as i32 + neighbour[2],
            ];

            let x = cur[0];
            let y = cur[1];
            let z = cur[2];

            // TODO: check neighbouring chunks later.
            // for now, just say that it is not occluded.
            if x < 0 || y < 0 || z < 0 {
                return false;
            }

            // and this one too.
            if x >= chunk_res[0] as i32 || y >= chunk_res[1] as i32 || z >= chunk_res[2] as i32 {
                return false;
            }

            if Self::is_empty_block(Self::get_block_type(
                chunk_data,
                chunk_res,
                [x as u32, y as u32, z as u32],
            )) {
                return false;
            }
        }
        return true;
    }

    fn calculate_normal(uvi: &[u32; 3], chunk_res: &[u32; 3], chunk_data: &Vec<u8>) -> [f32; 3] {
        let min_bound = [uvi[0] - 2, uvi[1] - 2, uvi[2] - 2];
        let max_bound = [uvi[0] + 2, uvi[1] + 2, uvi[2] + 2];

        // guard against out of bounds
        let min_bound = [
            min_bound[0].max(0),
            min_bound[1].max(0),
            min_bound[2].max(0),
        ];
        let max_bound = [
            max_bound[0].min(chunk_res[0] - 1),
            max_bound[1].min(chunk_res[1] - 1),
            max_bound[2].min(chunk_res[2] - 1),
        ];

        let mut normal = [0.0, 0.0, 0.0];
        for x in min_bound[0]..=max_bound[0] {
            for y in min_bound[1]..=max_bound[1] {
                for z in min_bound[2]..=max_bound[2] {
                    let delta = [
                        (x as i32 - uvi[0] as i32) as f32,
                        (y as i32 - uvi[1] as i32) as f32,
                        (z as i32 - uvi[2] as i32) as f32,
                    ];

                    normal = [
                        normal[0] + delta[0],
                        normal[1] + delta[1],
                        normal[2] + delta[2],
                    ];
                }
            }
        }

        let normal = glam::Vec3A::new(normal[0], normal[1], normal[2]).normalize();
        normal.to_array()
    }

    pub fn cull_chunk(&mut self, chunk_pos: IVec3) -> Vec<u32> {
        // Return an empty result if the chunk does not exist
        let Some(chunk) = self.chunks.get(&chunk_pos) else {
            log::error!("Chunk not found at position {:?}", chunk_pos);
            return Vec::new();
        };

        let chunk_data = &chunk.data;
        let chunk_res = self.chunk_res.to_array(); // [x, y, z]
        let total = (chunk_res[0] * chunk_res[1] * chunk_res[2]) as usize;

        let all: Vec<usize> = (0..total).collect();

        let culled_data: Vec<u32> = all
            .par_iter()
            .filter_map(|&i| {
                let (x, y, z) = Self::decompose_3d_index(i, chunk_res);
                let uvi = [x, y, z];

                let block_type = Self::get_block_type(chunk_data, &chunk_res, uvi);
                if !Self::is_empty_block(block_type)
                    && !Self::is_occluded(&uvi, &chunk_res, chunk_data)
                {
                    let normal = Self::calculate_normal(&uvi, &chunk_res, chunk_data);
                    Some(i as u32)
                } else {
                    None
                }
            })
            .collect();

        log::debug!(
            "original voxel count: {}, culled voxel count: {}, percentage: {}",
            total,
            culled_data.len(),
            culled_data.len() as f32 / total as f32
        );
        culled_data
    }

    /// Helper that converts a flattened 3D index back to (x, y, z).
    fn decompose_3d_index(i: usize, chunk_res: [u32; 3]) -> (u32, u32, u32) {
        let (x_res, y_res, _) = (chunk_res[0], chunk_res[1], chunk_res[2]);
        let z = i as u32 / (x_res * y_res);
        let rem = i as u32 % (x_res * y_res);
        let y = rem / x_res;
        let x = rem % x_res;
        (x, y, z)
    }

    fn create_chunk_init_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        resources: &BuilderResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                &resources.chunk_build_info_buf,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.weight_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        compute_descriptor_set
    }

    fn generate_chunk_data(
        &mut self,
        command_pool: &CommandPool,
        resolution: UVec3,
        chunk_pos: IVec3,
    ) -> Vec<u8> {
        // modify the uniform buffer to guide the chunk generation
        let chunk_build_info_layout = BufferBuilder::from_layout(
            self.chunk_init_sm
                .get_buffer_layout("ChunkBuildInfo")
                .unwrap(),
        );
        let chunk_build_info_data = chunk_build_info_layout
            .set_uvec3("chunk_res", resolution.to_array())
            .set_ivec3("chunk_pos", chunk_pos.to_array())
            .build();
        self.resources
            .chunk_build_info_buf
            .fill_raw(&chunk_build_info_data)
            .expect("Failed to fill buffer data");

        let start = std::time::Instant::now();
        execute_one_time_command(
            self.vulkan_context.device(),
            command_pool,
            &self.vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.resources
                    .weight_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    0,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, resolution.to_array());
            },
        );
        let end = std::time::Instant::now();
        log::debug!("Chunk generation time: {:?}", end - start);

        let start = std::time::Instant::now();
        let chunk_data = self
            .resources
            .weight_tex
            .fetch_data(&self.vulkan_context.get_general_queue(), command_pool)
            .expect("Failed to fetch buffer data");
        let end = std::time::Instant::now();
        log::debug!("Chunk data fetch time: {:?}", end - start);

        chunk_data
    }
}
