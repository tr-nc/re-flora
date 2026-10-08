//! Physical tree geometry is authored rest root-axis wood, never the visual skin stream.
//! Resident occupancy preserves cuts. All visual wood is separately excluded from terrain physics.
use super::{RestCollisionGeometry, CUBE_INDICES, NEIGHBORS, VOXEL_VERTICES};
use crate::{geom::RoundConeClearanceIndex, tree_gen::Tree};
use anyhow::{ensure, Context, Result};
use glam::{I64Vec3, UVec3, Vec3};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct RestTrunkCollision {
    cells: BTreeSet<[u32; 3]>,
}
impl RestTrunkCollision {
    pub(super) fn include_tree(
        &mut self,
        resident: &BTreeSet<[u32; 3]>,
        tree: &Tree,
        origin: Vec3,
    ) {
        let cones = tree.world_physical_trunks(origin);
        let trunk = RoundConeClearanceIndex::new(&cones);
        self.cells.extend(resident.iter().copied().filter(|cell| {
            let center = UVec3::from_array(*cell).as_vec3() + Vec3::splat(0.5);
            !trunk.has_minimum_clearance(center, 0.)
        }));
    }

    pub(super) fn append(&mut self, other: &Self) {
        self.cells.extend(other.cells.iter().copied());
    }

    pub(super) fn cells(&self) -> &BTreeSet<[u32; 3]> {
        &self.cells
    }

    pub(super) fn geometry(&self) -> Result<RestCollisionGeometry> {
        let mut positions = Vec::new();
        let mut corners = BTreeMap::new();
        let mut triangles = Vec::new();
        for &cell in &self.cells {
            for (face, direction) in NEIGHBORS.into_iter().enumerate() {
                // Signed wide arithmetic handles world-boundary faces without wrapping.
                let neighbor = UVec3::from_array(cell).as_i64vec3() + direction.as_i64vec3();
                if neighbor.cmpge(I64Vec3::ZERO).all()
                    && neighbor.cmple(I64Vec3::splat(u32::MAX as i64)).all()
                    && self.cells.contains(&neighbor.as_uvec3().to_array())
                {
                    continue;
                }
                // Test the physical occupancy, not visual faces. A lateral limb
                // touching a trunk must not leave an open hole in the trunk collider.
                for triangle in CUBE_INDICES[face * 6..face * 6 + 6].chunks_exact(3) {
                    let mut indices = [0; 3];
                    for (index, &corner) in indices.iter_mut().zip(triangle) {
                        let offset = VOXEL_VERTICES[corner as usize].to_array();
                        let key = [
                            cell[0]
                                .checked_add(offset[0])
                                .context("tree collision corner overflow")?,
                            cell[1]
                                .checked_add(offset[1])
                                .context("tree collision corner overflow")?,
                            cell[2]
                                .checked_add(offset[2])
                                .context("tree collision corner overflow")?,
                        ];
                        if let Some(&previous) = corners.get(&key) {
                            *index = previous;
                        } else {
                            *index = u32::try_from(positions.len())
                                .context("tree collision vertex overflow")?;
                            let position = UVec3::from_array(key).as_vec3() / 256.;
                            ensure!(position.is_finite(), "nonfinite tree collision vertex");
                            positions.push(position);
                            corners.insert(key, *index);
                        }
                    }
                    triangles.push(indices);
                }
            }
        }
        Ok((positions, triangles.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_occupancy_is_closed_compact_and_has_no_internal_faces() {
        let surface = RestTrunkCollision {
            cells: BTreeSet::from([[0, 0, 0], [1, 0, 0]]),
        };
        let (positions, triangles) = surface.geometry().unwrap();
        assert_eq!(positions.len(), 12);
        assert_eq!(triangles.len(), 20);
        let mut edges = BTreeMap::new();
        for triangle in triangles.iter() {
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let mut edge = [triangle[a], triangle[b]];
                edge.sort_unstable();
                *edges.entry(edge).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|&count| count == 2),
            "closed physical surface"
        );
        // Removed root wood stays removed rather than rebuilding an uncut authored cone.
        let cut = RestTrunkCollision {
            cells: BTreeSet::from([[1, 0, 0]]),
        };
        let (positions, triangles) = cut.geometry().unwrap();
        assert_eq!(triangles.len(), 12);
        assert!(positions.iter().all(|p| p.x >= 1. / 256.));
    }

    #[test]
    fn collision_corner_overflow_rejects_instead_of_wrapping() {
        let surface = RestTrunkCollision {
            cells: BTreeSet::from([[u32::MAX, 0, 0]]),
        };
        assert!(surface
            .geometry()
            .unwrap_err()
            .to_string()
            .contains("corner overflow"));
    }
}
