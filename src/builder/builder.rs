use super::chunk_data_builder::ChunkDataBuilder;
use super::frag_list_builder::FragListBuilder;
use super::octree_builder::OctreeBuilder;
use super::ExternalSharedResources;
use super::Resources;
use crate::geom::build_bvh;
use crate::geom::Aabb;
use crate::geom::BvhNode;
use crate::geom::RoundCone;
use crate::tree_gen::Tree;
use crate::util::ShaderCompiler;
use crate::util::Timer;
use crate::vkn::Allocator;
use crate::vkn::DescriptorPool;
use crate::vkn::VulkanContext;
use glam::UVec3;
use glam::Vec3;

pub struct Builder {
    vulkan_context: VulkanContext,
    resources: Resources,

    voxel_dim: UVec3,
    chunk_dim: UVec3,
    visible_chunk_dim: UVec3,

    chunk_data_builder: ChunkDataBuilder,
    frag_list_builder: FragListBuilder,
    octree_builder: OctreeBuilder,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        voxel_dim: UVec3,
        chunk_dim: UVec3,
        visible_chunk_dim: UVec3,
        octree_buffer_size: u64,
    ) -> Self {
        if voxel_dim.x != voxel_dim.y || voxel_dim.y != voxel_dim.z {
            log::error!("Dimension must be equal in all dimensions");
        }
        if voxel_dim.x & (voxel_dim.x - 1) != 0 {
            log::error!("Dimension must be a power of 2");
        }

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_context.device()).unwrap();

        let resources = Resources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            shader_compiler,
            voxel_dim,
            chunk_dim,
            visible_chunk_dim,
            octree_buffer_size,
        );

        let chunk_data_builder = ChunkDataBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let frag_list_builder = FragListBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
        );

        let octree_builder = OctreeBuilder::new(
            &vulkan_context,
            shader_compiler,
            descriptor_pool.clone(),
            &resources,
            octree_buffer_size,
        );

        Self {
            vulkan_context,
            resources,
            voxel_dim,
            chunk_dim,
            visible_chunk_dim,
            chunk_data_builder,
            frag_list_builder,
            octree_builder,
        }
    }

    // current benchmark results:
    // [re_flora::builder::builder] Average chunk init time: 4.3815ms
    // [re_flora::builder::builder] Average fragment + octree time: 1.351508ms

    pub fn init_chunks(&mut self) {
        let chunk_positions = {
            let mut positions = Vec::new();
            for i in 0..self.chunk_dim.x {
                for j in 0..self.chunk_dim.y {
                    for k in 0..self.chunk_dim.z {
                        positions.push(UVec3::new(i, j, k));
                    }
                }
            }
            positions
        };

        let timer = Timer::new();
        for chunk_pos in chunk_positions.iter() {
            self.build_chunk_data(*chunk_pos);
        }
        log::debug!(
            "Average chunk init time: {:?}",
            timer.elapsed() / chunk_positions.len() as u32
        );

        let timer = Timer::new();
        for chunk_pos in chunk_positions.iter() {
            self.build_octree(*chunk_pos);
        }
        log::debug!(
            "Average octree time: {:?}",
            timer.elapsed() / chunk_positions.len() as u32
        );

        self.update_octree_offset_atlas_tex();
    }

    fn build_chunk_data(&mut self, chunk_pos: UVec3) {
        self.chunk_data_builder.chunk_init(
            &self.vulkan_context,
            &self.resources,
            self.voxel_dim,
            chunk_pos,
        );
    }

    fn modify_chunk(&mut self, chunk_pos: UVec3, bvh_nodes: &[BvhNode], round_cones: &[RoundCone]) {
        self.chunk_data_builder.chunk_modify(
            &self.vulkan_context,
            &self.resources,
            self.voxel_dim,
            chunk_pos,
            bvh_nodes,
            round_cones,
        );
    }

    fn update_octree_offset_atlas_tex(&mut self) {
        self.octree_builder.update_octree_offset_atlas_tex(
            &self.vulkan_context,
            &self.resources,
            self.visible_chunk_dim,
        );
    }

    pub fn add_tree(&mut self, tree: &Tree, tree_pos: Vec3) {
        let mut round_cones = tree.get_trunks().to_vec();
        for round_cone in &mut round_cones {
            round_cone.transform(tree_pos);
        }

        let mut trunk_aabbs = Vec::new();
        for round_cone in &round_cones {
            trunk_aabbs.push(round_cone.get_aabb());
        }

        let bvh_nodes = build_bvh(&trunk_aabbs);

        let bounding_box = &bvh_nodes[0].aabb; // the root node of the BVH

        let affacted_chunk_positions =
            determine_relative_chunk_positions(self.voxel_dim, bounding_box).unwrap();

        log::debug!(
            "Affacted chunk positions: {:?}",
            affacted_chunk_positions.iter().collect::<Vec<_>>()
        );

        for chunk_pos in affacted_chunk_positions.iter() {
            self.modify_chunk(*chunk_pos, &bvh_nodes, &round_cones);
        }

        for chunk_pos in affacted_chunk_positions.iter() {
            self.build_octree(*chunk_pos);
        }

        self.update_octree_offset_atlas_tex();

        fn determine_relative_chunk_positions(
            voxel_dim: UVec3,
            aabb: &Aabb,
        ) -> Result<Vec<UVec3>, String> {
            let rect_min = aabb.min().floor();
            let rect_max = aabb.max().ceil();

            if rect_min.x < 0.0 || rect_min.y < 0.0 || rect_min.z < 0.0 {
                return Err("AABB min is out of bounds".to_string());
            }
            if rect_max.x < 0.0 || rect_max.y < 0.0 || rect_max.z < 0.0 {
                return Err("AABB max is out of bounds".to_string());
            }

            let rect_min_u = rect_min.as_uvec3();
            let rect_max_u = rect_max.as_uvec3();
            let min_chunk_pos = world_voxel_pos_to_chunk_pos(voxel_dim, rect_min_u);
            let max_chunk_pos = world_voxel_pos_to_chunk_pos(voxel_dim, rect_max_u);

            let mut positions = Vec::new();
            for i in min_chunk_pos.x..=max_chunk_pos.x {
                for j in min_chunk_pos.y..=max_chunk_pos.y {
                    for k in min_chunk_pos.z..=max_chunk_pos.z {
                        positions.push(UVec3::new(i, j, k));
                    }
                }
            }
            return Ok(positions);

            fn world_voxel_pos_to_chunk_pos(voxel_dim: UVec3, world_pos: UVec3) -> UVec3 {
                let chunk_pos = world_pos / voxel_dim;
                chunk_pos
            }
        }
    }

    fn build_octree(&mut self, chunk_pos: UVec3) {
        self.frag_list_builder
            .build(&self.vulkan_context, &self.resources, chunk_pos);

        let fragment_list_len = self.frag_list_builder.get_fraglist_length(&self.resources);
        if fragment_list_len == 0 {
            log::debug!("Fragment list for chunk {:?} is empty", chunk_pos);
            return;
        } else {
            log::debug!(
                "Fragment list for chunk {:?} has {} fragments",
                chunk_pos,
                fragment_list_len
            );
        }

        self.octree_builder.build(
            &self.vulkan_context,
            &self.resources,
            fragment_list_len,
            chunk_pos,
            self.voxel_dim,
        );
    }

    pub fn get_external_shared_resources(&self) -> &ExternalSharedResources {
        &self.resources.external_shared_resources
    }
}
