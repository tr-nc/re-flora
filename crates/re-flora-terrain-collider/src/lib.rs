use glam::{IVec3, UVec3, Vec3};
use std::{cmp::Ordering, collections::BinaryHeap};

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
