use glam::Vec3;
use std::collections::HashMap;

/// Result of clustering multiple positions into one representative point.
#[derive(Debug, Clone)]
pub struct ClusterResult {
    pub pos: Vec3,
    pub items_count: u32,
}

/// Greedy spatial clustering based on a distance threshold.
///
/// Each input position is either assigned to the nearest existing cluster whose
/// center is within `distance_threshold`, or starts a new cluster. This keeps
/// every member of a cluster within `distance_threshold` of its cluster center.
pub fn cluster_positions(positions: &[Vec3], distance_threshold: f32) -> Vec<ClusterResult> {
    if positions.is_empty() {
        return Vec::new();
    }

    // Treat non-positive threshold as "no clustering".
    if distance_threshold <= 0.0 {
        return positions
            .iter()
            .map(|&pos| ClusterResult {
                pos,
                items_count: 1,
            })
            .collect();
    }

    let cell_size = distance_threshold;
    let radius_sq = distance_threshold * distance_threshold;

    let mut clusters: Vec<ClusterResult> = Vec::new();
    let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();

    fn cell_coords(pos: Vec3, cell_size: f32) -> (i32, i32, i32) {
        let inv = 1.0 / cell_size;
        let x = (pos.x * inv).floor() as i32;
        let y = (pos.y * inv).floor() as i32;
        let z = (pos.z * inv).floor() as i32;
        (x, y, z)
    }

    for &pos in positions {
        let cell = cell_coords(pos, cell_size);

        // Find the nearest cluster center within distance_threshold.
        let mut best_idx: Option<usize> = None;
        let mut best_dist_sq = radius_sq;

        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor_cell = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    if let Some(indices) = grid.get(&neighbor_cell) {
                        for &cluster_idx in indices {
                            let cluster_pos = clusters[cluster_idx].pos;
                            let diff = pos - cluster_pos;
                            let dist_sq = diff.length_squared();
                            if dist_sq <= best_dist_sq {
                                best_dist_sq = dist_sq;
                                best_idx = Some(cluster_idx);
                            }
                        }
                    }
                }
            }
        }

        if let Some(idx) = best_idx {
            // Assign to existing cluster.
            clusters[idx].items_count += 1;
        } else {
            // Create a new cluster with this position as the center.
            let idx = clusters.len();
            clusters.push(ClusterResult {
                pos,
                items_count: 1,
            });
            grid.entry(cell).or_default().push(idx);
        }
    }

    clusters
}

