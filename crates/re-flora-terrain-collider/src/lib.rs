use glam::{IVec3, UVec3, Vec3};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
pub struct ContreeCpuNode {
    pub packed_0: u32,
    pub child_mask_lo: u32,
    pub child_mask_hi: u32,
}

#[derive(Clone, Debug)]
pub struct ContreeCpuChunkCache {
    pub chunk_idx: UVec3,
    pub nodes: Vec<ContreeCpuNode>,
    pub leaves: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContreeSparseVoxelError {
    InvalidVoxelDimension(UVec3),
    MissingNode { address: usize },
    MissingLeaf { address: usize },
    UnexpectedLeafDepth { actual: u32, expected: u32 },
    UnexpectedNodeDepth { actual: u32, expected: u32 },
    CoordinateOverflow,
}

impl ContreeCpuChunkCache {
    pub fn voxel_type_at(&self, point: Vec3, voxel_type_mask: u32) -> u8 {
        if self.nodes.is_empty() {
            return 0;
        }

        let local_pos = point - self.chunk_idx.as_vec3() + Vec3::ONE;
        if local_pos.cmplt(Vec3::ONE).any() || local_pos.cmpge(Vec3::splat(2.0)).any() {
            return 0;
        }

        let mut scale_exp = 21i32;
        let mut node = self.nodes[0];
        for _ in 0..16 {
            let Some(child_idx) = contree_node_cell_index(local_pos, scale_exp).map(|idx| idx as u32)
            else {
                return 0;
            };
            if !contree_child_mask_test(node, child_idx) {
                return 0;
            }

            let bits = contree_child_mask_bitcount_below(node, child_idx);
            let child_addr = ((node.packed_0 >> 1) + bits) as usize;
            if node.packed_0 & 1 != 0 {
                return self
                    .leaves
                    .get(child_addr)
                    .map_or(0, |voxel| (*voxel & voxel_type_mask) as u8);
            }

            let Some(next_node) = self.nodes.get(child_addr).copied() else {
                return 0;
            };
            node = next_node;
            scale_exp -= 2;
        }

        0
    }

    /// Traverses only allocated Contree nodes/leaves and returns matching surface voxels in
    /// deterministic world-voxel order. This avoids probing every coordinate in a dense chunk.
    pub fn voxels_matching_type(
        &self,
        voxel_dim_per_chunk: UVec3,
        voxel_type_mask: u32,
        wanted_voxel_type: u32,
    ) -> Result<Vec<UVec3>, ContreeSparseVoxelError> {
        let levels = contree_levels_for_voxel_dim(voxel_dim_per_chunk)?;
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }
        let chunk_origin = UVec3::new(
            self.chunk_idx
                .x
                .checked_mul(voxel_dim_per_chunk.x)
                .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
            self.chunk_idx
                .y
                .checked_mul(voxel_dim_per_chunk.y)
                .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
            self.chunk_idx
                .z
                .checked_mul(voxel_dim_per_chunk.z)
                .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
        );
        let mut matches = Vec::new();
        collect_matching_contree_voxels(
            self,
            0,
            0,
            UVec3::ZERO,
            levels,
            chunk_origin,
            voxel_type_mask,
            wanted_voxel_type & voxel_type_mask,
            &mut matches,
        )?;
        matches.sort_unstable_by_key(|voxel| voxel.to_array());
        Ok(matches)
    }
}

fn contree_levels_for_voxel_dim(
    voxel_dim_per_chunk: UVec3,
) -> Result<u32, ContreeSparseVoxelError> {
    if voxel_dim_per_chunk.cmpeq(UVec3::ZERO).any()
        || voxel_dim_per_chunk.x != voxel_dim_per_chunk.y
        || voxel_dim_per_chunk.x != voxel_dim_per_chunk.z
    {
        return Err(ContreeSparseVoxelError::InvalidVoxelDimension(
            voxel_dim_per_chunk,
        ));
    }
    let mut dim = voxel_dim_per_chunk.x;
    let mut levels = 0;
    while dim > 1 {
        if !dim.is_multiple_of(4) {
            return Err(ContreeSparseVoxelError::InvalidVoxelDimension(
                voxel_dim_per_chunk,
            ));
        }
        dim /= 4;
        levels += 1;
    }
    if levels == 0 {
        return Err(ContreeSparseVoxelError::InvalidVoxelDimension(
            voxel_dim_per_chunk,
        ));
    }
    Ok(levels)
}

#[allow(clippy::too_many_arguments)]
fn collect_matching_contree_voxels(
    cache: &ContreeCpuChunkCache,
    node_address: usize,
    depth: u32,
    prefix: UVec3,
    expected_levels: u32,
    chunk_origin: UVec3,
    voxel_type_mask: u32,
    wanted_voxel_type: u32,
    matches: &mut Vec<UVec3>,
) -> Result<(), ContreeSparseVoxelError> {
    let node = cache
        .nodes
        .get(node_address)
        .copied()
        .ok_or(ContreeSparseVoxelError::MissingNode {
            address: node_address,
        })?;
    let children_are_leaves = node.packed_0 & 1 != 0;
    let child_base = (node.packed_0 >> 1) as usize;
    for child_index in 0..64 {
        if !contree_child_mask_test(node, child_index) {
            continue;
        }
        let child_rank = contree_child_mask_bitcount_below(node, child_index) as usize;
        let child_address = child_base + child_rank;
        let digit = UVec3::new(
            child_index % 4,
            child_index / 16,
            (child_index / 4) % 4,
        );
        let child_prefix = prefix * 4 + digit;
        let child_depth = depth + 1;
        if children_are_leaves {
            if child_depth != expected_levels {
                return Err(ContreeSparseVoxelError::UnexpectedLeafDepth {
                    actual: child_depth,
                    expected: expected_levels,
                });
            }
            let voxel = *cache
                .leaves
                .get(child_address)
                .ok_or(ContreeSparseVoxelError::MissingLeaf {
                    address: child_address,
                })?;
            if voxel & voxel_type_mask == wanted_voxel_type {
                matches.push(checked_world_voxel(chunk_origin, child_prefix)?);
            }
        } else {
            if child_depth >= expected_levels {
                return Err(ContreeSparseVoxelError::UnexpectedNodeDepth {
                    actual: child_depth,
                    expected: expected_levels,
                });
            }
            collect_matching_contree_voxels(
                cache,
                child_address,
                child_depth,
                child_prefix,
                expected_levels,
                chunk_origin,
                voxel_type_mask,
                wanted_voxel_type,
                matches,
            )?;
        }
    }
    Ok(())
}

fn checked_world_voxel(
    chunk_origin: UVec3,
    local_voxel: UVec3,
) -> Result<UVec3, ContreeSparseVoxelError> {
    Ok(UVec3::new(
        chunk_origin
            .x
            .checked_add(local_voxel.x)
            .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
        chunk_origin
            .y
            .checked_add(local_voxel.y)
            .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
        chunk_origin
            .z
            .checked_add(local_voxel.z)
            .ok_or(ContreeSparseVoxelError::CoordinateOverflow)?,
    ))
}

/// Samples a dense x-fastest voxel block from immutable Contree CPU caches.
///
/// This lives in the release-optimized terrain-collider crate because full-world collider import
/// executes the sampling loop for every world voxel, even when the application uses its dev
/// profile.
pub fn export_contree_voxel_types(
    chunk_dim: UVec3,
    voxel_dim_per_chunk: UVec3,
    scene_chunks: &[Option<UVec3>],
    chunk_caches: &HashMap<UVec3, Arc<ContreeCpuChunkCache>>,
    voxel_min: UVec3,
    dim: UVec3,
    voxel_type_mask: u32,
) -> Vec<u8> {
    let voxel_max = voxel_min + dim;
    let element_count = dim.x as usize * dim.y as usize * dim.z as usize;
    let chunk_min = voxel_min / voxel_dim_per_chunk;
    let chunk_max = (voxel_max - UVec3::ONE) / voxel_dim_per_chunk;

    if chunk_min == chunk_max {
        if !contree_scene_chunk_present(chunk_dim, scene_chunks, chunk_min) {
            return vec![0; element_count];
        }
        let cache = chunk_caches
            .get(&chunk_min)
            .expect("ready scene chunks must have a CPU cache");
        let mut voxel_types = Vec::with_capacity(element_count);
        for z in voxel_min.z..voxel_max.z {
            for y in voxel_min.y..voxel_max.y {
                for x in voxel_min.x..voxel_max.x {
                    let local_voxel = UVec3::new(x, y, z) % voxel_dim_per_chunk;
                    let point = chunk_min.as_vec3()
                        + (local_voxel.as_vec3() + Vec3::splat(0.5))
                            / voxel_dim_per_chunk.as_vec3();
                    voxel_types.push(cache.voxel_type_at(point, voxel_type_mask));
                }
            }
        }
        return voxel_types;
    }

    let mut voxel_types = Vec::with_capacity(element_count);
    for z in voxel_min.z..voxel_max.z {
        for y in voxel_min.y..voxel_max.y {
            for x in voxel_min.x..voxel_max.x {
                let voxel = UVec3::new(x, y, z);
                let chunk_idx = voxel / voxel_dim_per_chunk;
                let voxel_type = if !contree_scene_chunk_present(
                    chunk_dim,
                    scene_chunks,
                    chunk_idx,
                ) {
                    0
                } else {
                    let local_voxel = voxel % voxel_dim_per_chunk;
                    let point = chunk_idx.as_vec3()
                        + (local_voxel.as_vec3() + Vec3::splat(0.5))
                            / voxel_dim_per_chunk.as_vec3();
                    chunk_caches
                        .get(&chunk_idx)
                        .expect("ready scene chunks must have a CPU cache")
                        .voxel_type_at(point, voxel_type_mask)
                };
                voxel_types.push(voxel_type);
            }
        }
    }
    voxel_types
}

fn contree_scene_chunk_present(
    chunk_dim: UVec3,
    scene_chunks: &[Option<UVec3>],
    chunk_idx: UVec3,
) -> bool {
    let index =
        (chunk_idx.x + chunk_idx.z * chunk_dim.x + chunk_idx.y * chunk_dim.x * chunk_dim.z) as usize;
    scene_chunks[index].is_some()
}

fn contree_node_cell_index(pos: Vec3, scale_exp: i32) -> Option<i32> {
    let shift = u32::try_from(scale_exp).ok()?;
    let px = (pos.x.to_bits() >> shift) & 3;
    let py = (pos.y.to_bits() >> shift) & 3;
    let pz = (pos.z.to_bits() >> shift) & 3;
    Some((px + pz * 4 + py * 16) as i32)
}

fn contree_child_mask_test(node: ContreeCpuNode, idx: u32) -> bool {
    if idx < 32 {
        (node.child_mask_lo & (1 << idx)) != 0
    } else {
        (node.child_mask_hi & (1 << (idx - 32))) != 0
    }
}

fn contree_child_mask_bitcount_below(node: ContreeCpuNode, idx: u32) -> u32 {
    if idx < 32 {
        (node.child_mask_lo & ((1 << idx) - 1)).count_ones()
    } else {
        node.child_mask_lo.count_ones()
            + (node.child_mask_hi & ((1 << (idx - 32)) - 1)).count_ones()
    }
}

/// Build a signed-distance field from a regular solid/empty voxel sample grid.
///
/// The input samples use x-fastest order:
/// `((z * dim.y + y) * dim.x) + x`.
///
/// The output has the same layout and sign convention:
/// negative values are solid, positive values are empty. Distances are measured
/// in world units derived from `bounds_min_ws..bounds_max_ws`.
pub fn signed_distance_from_solid_samples(
    dim: UVec3,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
    solid: &[bool],
) -> Vec<f32> {
    assert!(dim.x >= 2 && dim.y >= 2 && dim.z >= 2);
    assert_eq!(solid.len(), grid_len(dim));

    let cell_size = (bounds_max_ws - bounds_min_ws) / (dim - UVec3::ONE).as_vec3();
    let fallback_distance = (bounds_max_ws - bounds_min_ws)
        .length()
        .max(cell_size.length());
    let mut distance = vec![f32::INFINITY; solid.len()];
    let mut heap = BinaryHeap::new();

    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                let idx = grid_index(dim, x, y, z);
                for_each_neighbor(dim, x, y, z, |nx, ny, nz, offset| {
                    let neighbor_idx = grid_index(dim, nx, ny, nz);
                    if solid[idx] != solid[neighbor_idx] {
                        let seed_distance = neighbor_step_distance(offset, cell_size) * 0.5;
                        distance[idx] = distance[idx].min(seed_distance);
                    }
                });
                if distance[idx].is_finite() {
                    heap.push(DistanceQueueEntry {
                        distance: distance[idx],
                        index: idx,
                    });
                }
            }
        }
    }

    if heap.is_empty() {
        return solid
            .iter()
            .map(|&is_solid| {
                if is_solid {
                    -fallback_distance
                } else {
                    fallback_distance
                }
            })
            .collect();
    }

    while let Some(entry) = heap.pop() {
        if entry.distance > distance[entry.index] + 1.0e-6 {
            continue;
        }

        let (x, y, z) = grid_coords(dim, entry.index);
        for_each_neighbor(dim, x, y, z, |nx, ny, nz, offset| {
            let neighbor_idx = grid_index(dim, nx, ny, nz);
            let next_distance = entry.distance + neighbor_step_distance(offset, cell_size);
            if next_distance < distance[neighbor_idx] {
                distance[neighbor_idx] = next_distance;
                heap.push(DistanceQueueEntry {
                    distance: next_distance,
                    index: neighbor_idx,
                });
            }
        });
    }

    distance
        .into_iter()
        .zip(solid.iter().copied())
        .map(|(distance, is_solid)| if is_solid { -distance } else { distance })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DistanceQueueEntry {
    distance: f32,
    index: usize,
}

impl Eq for DistanceQueueEntry {}

impl Ord for DistanceQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for DistanceQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn grid_len(dim: UVec3) -> usize {
    (dim.x as usize) * (dim.y as usize) * (dim.z as usize)
}

fn grid_index(dim: UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * dim.y as usize + y as usize) * dim.x as usize) + x as usize
}

fn grid_coords(dim: UVec3, index: usize) -> (u32, u32, u32) {
    let x = index % dim.x as usize;
    let yz = index / dim.x as usize;
    let y = yz % dim.y as usize;
    let z = yz / dim.y as usize;
    (x as u32, y as u32, z as u32)
}

fn for_each_neighbor(dim: UVec3, x: u32, y: u32, z: u32, mut f: impl FnMut(u32, u32, u32, IVec3)) {
    for oz in -1..=1 {
        for oy in -1..=1 {
            for ox in -1..=1 {
                if ox == 0 && oy == 0 && oz == 0 {
                    continue;
                }
                let nx = x as i32 + ox;
                let ny = y as i32 + oy;
                let nz = z as i32 + oz;
                if nx >= 0
                    && ny >= 0
                    && nz >= 0
                    && nx < dim.x as i32
                    && ny < dim.y as i32
                    && nz < dim.z as i32
                {
                    f(nx as u32, ny as u32, nz as u32, IVec3::new(ox, oy, oz));
                }
            }
        }
    }
}

fn neighbor_step_distance(offset: IVec3, cell_size: Vec3) -> f32 {
    (offset.as_vec3() * cell_size).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_voxel_contree(local_voxel: UVec3, voxel_type: u32) -> ContreeCpuChunkCache {
        let digits = [
            UVec3::new(local_voxel.x / 64, local_voxel.y / 64, local_voxel.z / 64),
            UVec3::new(
                (local_voxel.x / 16) % 4,
                (local_voxel.y / 16) % 4,
                (local_voxel.z / 16) % 4,
            ),
            UVec3::new(
                (local_voxel.x / 4) % 4,
                (local_voxel.y / 4) % 4,
                (local_voxel.z / 4) % 4,
            ),
            local_voxel % 4,
        ];
        let child = |digit: UVec3| digit.x + digit.z * 4 + digit.y * 16;
        let mask = |index: u32| {
            if index < 32 {
                (1 << index, 0)
            } else {
                (0, 1 << (index - 32))
            }
        };
        let mut nodes = Vec::new();
        for (depth, digit) in digits.iter().take(3).enumerate() {
            let (child_mask_lo, child_mask_hi) = mask(child(*digit));
            nodes.push(ContreeCpuNode {
                packed_0: ((depth as u32 + 1) << 1),
                child_mask_lo,
                child_mask_hi,
            });
        }
        let (child_mask_lo, child_mask_hi) = mask(child(digits[3]));
        nodes.push(ContreeCpuNode {
            packed_0: 1,
            child_mask_lo,
            child_mask_hi,
        });
        ContreeCpuChunkCache {
            chunk_idx: UVec3::new(2, 1, 3),
            nodes,
            leaves: vec![voxel_type],
        }
    }

    fn multi_leaf_root_contree() -> ContreeCpuChunkCache {
        let child_mask_lo = (1 << 0) | (1 << 1);
        let child_mask_hi = (1 << (32 - 32)) | (1 << (63 - 32));
        ContreeCpuChunkCache {
            chunk_idx: UVec3::new(2, 1, 3),
            nodes: vec![ContreeCpuNode {
                packed_0: 1,
                child_mask_lo,
                child_mask_hi,
            }],
            // Packed child order is 0, 1, 32, 63.
            leaves: vec![8, 7, 8, 8],
        }
    }

    #[test]
    fn sparse_type_iteration_recovers_exact_world_voxel_without_dense_sampling() {
        let cache = one_voxel_contree(UVec3::new(129, 34, 255), 8);

        let voxels = cache
            .voxels_matching_type(UVec3::splat(256), 0x0f, 8)
            .unwrap();

        assert_eq!(voxels, vec![UVec3::new(641, 290, 1023)]);
        assert!(cache
            .voxels_matching_type(UVec3::splat(256), 0x0f, 7)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn sparse_multi_leaf_lo_hi_masks_equal_the_existing_point_query_oracle() {
        let cache = multi_leaf_root_contree();
        let voxel_dim = UVec3::splat(4);

        let sparse = cache
            .voxels_matching_type(voxel_dim, 0x0f, 8)
            .unwrap();
        let mut oracle = Vec::new();
        for z in 0..voxel_dim.z {
            for y in 0..voxel_dim.y {
                for x in 0..voxel_dim.x {
                    let local_voxel = UVec3::new(x, y, z);
                    let point = cache.chunk_idx.as_vec3()
                        + (local_voxel.as_vec3() + Vec3::splat(0.5)) / voxel_dim.as_vec3();
                    if cache.voxel_type_at(point, 0x0f) == 8 {
                        oracle.push(cache.chunk_idx * voxel_dim + local_voxel);
                    }
                }
            }
        }
        oracle.sort_unstable_by_key(|voxel| voxel.to_array());

        assert_eq!(sparse, oracle);
        assert_eq!(
            sparse,
            vec![
                UVec3::new(8, 4, 12),
                UVec3::new(8, 6, 12),
                UVec3::new(11, 7, 15),
            ]
        );
        assert_eq!(
            cache
                .voxels_matching_type(voxel_dim, 0x0f, 8)
                .unwrap(),
            sparse,
            "world-coordinate order must be stable across calls"
        );
    }

    #[test]
    fn sparse_type_iteration_rejects_non_base_four_or_malformed_trees() {
        let cache = one_voxel_contree(UVec3::ZERO, 8);
        assert_eq!(
            cache.voxels_matching_type(UVec3::splat(128), 0x0f, 8),
            Err(ContreeSparseVoxelError::InvalidVoxelDimension(
                UVec3::splat(128)
            ))
        );

        let mut malformed = cache;
        malformed.nodes[2].packed_0 = 200 << 1;
        assert!(matches!(
            malformed.voxels_matching_type(UVec3::splat(256), 0x0f, 8),
            Err(ContreeSparseVoxelError::MissingNode { .. })
        ));

        let mut missing_leaf = multi_leaf_root_contree();
        missing_leaf.leaves.truncate(3);
        assert_eq!(
            missing_leaf.voxels_matching_type(UVec3::splat(4), 0x0f, 8),
            Err(ContreeSparseVoxelError::MissingLeaf { address: 3 })
        );

        let mut early_leaf = one_voxel_contree(UVec3::ZERO, 8);
        early_leaf.nodes[0].packed_0 = 1;
        early_leaf.leaves = vec![8];
        assert_eq!(
            early_leaf.voxels_matching_type(UVec3::splat(256), 0x0f, 8),
            Err(ContreeSparseVoxelError::UnexpectedLeafDepth {
                actual: 1,
                expected: 4,
            })
        );

        let mut node_beyond_voxel_depth = one_voxel_contree(UVec3::ZERO, 8);
        node_beyond_voxel_depth.nodes[3].packed_0 = 0;
        assert_eq!(
            node_beyond_voxel_depth.voxels_matching_type(UVec3::splat(256), 0x0f, 8),
            Err(ContreeSparseVoxelError::UnexpectedNodeDepth {
                actual: 4,
                expected: 4,
            })
        );

        assert_eq!(
            checked_world_voxel(UVec3::new(u32::MAX, 0, 0), UVec3::X),
            Err(ContreeSparseVoxelError::CoordinateOverflow)
        );
    }

    #[test]
    fn signed_distance_marks_solid_negative_and_empty_positive() {
        let dim = UVec3::new(3, 3, 3);
        let mut solid = vec![false; grid_len(dim)];
        for z in 0..dim.z {
            for x in 0..dim.x {
                solid[grid_index(dim, x, 0, z)] = true;
            }
        }

        let sdf = signed_distance_from_solid_samples(dim, Vec3::ZERO, Vec3::ONE, &solid);

        assert!(sdf[grid_index(dim, 1, 0, 1)] < 0.0);
        assert!(sdf[grid_index(dim, 1, 2, 1)] > 0.0);
        assert!(sdf[grid_index(dim, 1, 1, 1)].abs() < sdf[grid_index(dim, 1, 2, 1)].abs());
    }

    #[test]
    fn signed_distance_flat_floor_uses_half_cell_boundary() {
        let dim = UVec3::new(5, 5, 5);
        let mut solid = vec![false; grid_len(dim)];
        for z in 0..dim.z {
            for y in 0..=1 {
                for x in 0..dim.x {
                    solid[grid_index(dim, x, y, z)] = true;
                }
            }
        }

        let sdf = signed_distance_from_solid_samples(dim, Vec3::ZERO, Vec3::ONE, &solid);

        assert_close(sdf[grid_index(dim, 2, 1, 2)], -0.125);
        assert_close(sdf[grid_index(dim, 2, 2, 2)], 0.125);
        assert_close(sdf[grid_index(dim, 2, 4, 2)], 0.625);
    }

    #[test]
    fn signed_distance_all_solid_uses_negative_fallback() {
        let dim = UVec3::new(3, 3, 3);
        let solid = vec![true; grid_len(dim)];

        let sdf = signed_distance_from_solid_samples(dim, Vec3::ZERO, Vec3::ONE, &solid);

        assert!(sdf
            .iter()
            .all(|distance| distance.is_finite() && *distance < 0.0));
        assert_close(sdf[grid_index(dim, 1, 1, 1)], -Vec3::ONE.length());
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }
}
