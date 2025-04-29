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
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::VulkanContext;
use glam::IVec3;
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

    pub fn init_chunks(&mut self) -> Result<(), String> {
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
            self.build_chunk_data(*chunk_pos)?;
        }
        log::debug!(
            "Average chunk init time: {:?}",
            timer.elapsed() / chunk_positions.len() as u32
        );

        let timer = Timer::new();
        for chunk_pos in chunk_positions.iter() {
            self.build_octree(*chunk_pos)?;
        }
        log::debug!(
            "Average octree time: {:?}",
            timer.elapsed() / chunk_positions.len() as u32
        );

        self.update_octree_offset_atlas_tex();
        return Ok(());
    }

    /// Debug only function
    pub fn create_scene_bvh(&mut self, tree: &Tree, tree_pos: Vec3) {
        let mut leaves = tree.leaves().to_vec();

        for leaf in &mut leaves {
            leaf.transform(tree_pos);
        }
        for leaf in &mut leaves {
            leaf.scale(Vec3::new(1.0 / 256.0, 1.0 / 256.0, 1.0 / 256.0));
        }
        let mut trunk_aabbs = Vec::new();
        for leaf in &leaves {
            trunk_aabbs.push(leaf.aabb());
        }

        let bvh_nodes = build_bvh(&trunk_aabbs);

        update_scene_bvh_nodes(&self.resources, &bvh_nodes);

        fn update_scene_bvh_nodes(resources: &Resources, bvh_nodes: &[BvhNode]) {
            for i in 0..bvh_nodes.len() {
                let bvh_node = &bvh_nodes[i];

                let combined_offset: u32 = if bvh_node.is_leaf {
                    let primitive_idx = bvh_node.data_offset;
                    0x8000_0000 | primitive_idx
                } else {
                    bvh_node.left
                };
                let data = StructMemberDataBuilder::from_buffer(resources.scene_bvh_nodes())
                    .set_field(
                        "data.aabb_min",
                        PlainMemberTypeWithData::Vec3(bvh_node.aabb.min().to_array()),
                    )
                    .unwrap()
                    .set_field(
                        "data.aabb_max",
                        PlainMemberTypeWithData::Vec3(bvh_node.aabb.max().to_array()),
                    )
                    .unwrap()
                    .set_field(
                        "data.offset",
                        PlainMemberTypeWithData::UInt(combined_offset),
                    )
                    .unwrap()
                    .get_data_u8();
                resources
                    .scene_bvh_nodes()
                    .fill_element_with_raw_u8(&data, i as u64)
                    .unwrap();
            }
        }
    }

    fn build_chunk_data(&mut self, chunk_pos: UVec3) -> Result<(), String> {
        self.check_chunk_pos(chunk_pos)?;
        self.chunk_data_builder.chunk_init(
            &self.vulkan_context,
            &self.resources,
            self.voxel_dim,
            chunk_pos,
        );
        return Ok(());
    }

    fn check_chunk_pos(&self, chunk_pos: UVec3) -> Result<(), String> {
        if chunk_pos.x >= self.chunk_dim.x
            || chunk_pos.y >= self.chunk_dim.y
            || chunk_pos.z >= self.chunk_dim.z
        {
            return Err(format!("Chunk position out of bounds: {:?}", chunk_pos));
        }
        Ok(())
    }

    fn modify_chunk(
        &mut self,
        chunk_pos: UVec3,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
    ) -> Result<(), String> {
        self.check_chunk_pos(chunk_pos)?;
        self.chunk_data_builder.chunk_modify(
            &self.vulkan_context,
            &self.resources,
            self.voxel_dim,
            chunk_pos,
            bvh_nodes,
            round_cones,
        );

        return Ok(());
    }

    fn update_octree_offset_atlas_tex(&mut self) {
        self.octree_builder.update_octree_offset_atlas_tex(
            &self.vulkan_context,
            &self.resources,
            self.visible_chunk_dim,
        );
    }

    pub fn add_tree(&mut self, tree: &Tree, tree_pos: Vec3) -> Result<(), String> {
        let mut trunks = tree.trunks().to_vec();
        for trunk in &mut trunks {
            trunk.transform(tree_pos);
        }

        let mut trunk_aabbs = Vec::new();
        for trunk in &trunks {
            trunk_aabbs.push(trunk.aabb());
        }

        let bvh_nodes = build_bvh(&trunk_aabbs);

        let bounding_box = &bvh_nodes[0].aabb; // the root node of the BVH

        let affacted_chunk_positions =
            determine_relative_chunk_positions(self.voxel_dim, bounding_box).unwrap();
        let in_bound_chunk_positions =
            select_only_in_bound_chunk_positions(affacted_chunk_positions, self.chunk_dim);

        for chunk_pos in in_bound_chunk_positions.iter() {
            self.build_chunk_data(*chunk_pos)?; // this allows the tree to be built in place, with removal of the old tree on the same chunk
            self.modify_chunk(*chunk_pos, &bvh_nodes, &trunks)?;
        }

        for chunk_pos in in_bound_chunk_positions.iter() {
            self.build_octree(*chunk_pos)?;
        }

        self.update_octree_offset_atlas_tex();

        return Ok(());

        fn determine_relative_chunk_positions(
            voxel_dim: UVec3,
            aabb: &Aabb,
        ) -> Result<Vec<IVec3>, String> {
            let rect_min = aabb.min().floor();
            let rect_max = aabb.max().ceil();

            let rect_min_i = rect_min.as_ivec3();
            let rect_max_i = rect_max.as_ivec3();
            let min_chunk_pos = world_voxel_pos_to_chunk_pos(voxel_dim, rect_min_i);
            let max_chunk_pos = world_voxel_pos_to_chunk_pos(voxel_dim, rect_max_i);

            let mut positions = Vec::new();
            for i in min_chunk_pos.x..=max_chunk_pos.x {
                for j in min_chunk_pos.y..=max_chunk_pos.y {
                    for k in min_chunk_pos.z..=max_chunk_pos.z {
                        positions.push(IVec3::new(i, j, k));
                    }
                }
            }
            return Ok(positions);

            fn world_voxel_pos_to_chunk_pos(voxel_dim: UVec3, world_pos: IVec3) -> IVec3 {
                let chunk_pos = world_pos / voxel_dim.as_ivec3();
                chunk_pos
            }
        }

        fn select_only_in_bound_chunk_positions(
            all_chunk_positions: Vec<IVec3>,
            chunk_dim: UVec3,
        ) -> Vec<UVec3> {
            let mut selected_chunk_positions = Vec::new();
            for chunk_pos in all_chunk_positions.iter() {
                if chunk_pos.x >= 0
                    && chunk_pos.y >= 0
                    && chunk_pos.z >= 0
                    && chunk_pos.x < chunk_dim.x as i32
                    && chunk_pos.y < chunk_dim.y as i32
                    && chunk_pos.z < chunk_dim.z as i32
                {
                    selected_chunk_positions.push(chunk_pos.as_uvec3());
                }
            }
            return selected_chunk_positions;
        }
    }

    fn build_octree(&mut self, chunk_pos: UVec3) -> Result<(), String> {
        self.check_chunk_pos(chunk_pos)?;
        self.frag_list_builder.build(
            &self.vulkan_context,
            &self.resources,
            chunk_pos * self.voxel_dim,
            self.voxel_dim,
            true,
        );

        let fragment_list_len = self.frag_list_builder.get_fraglist_length(&self.resources);
        if fragment_list_len == 0 {
            log::debug!("Fragment list for chunk {:?} is empty", chunk_pos);
            return Ok(());
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
        return Ok(());
    }

    pub fn get_external_shared_resources(&self) -> &ExternalSharedResources {
        &self.resources.external_shared_resources
    }
}
