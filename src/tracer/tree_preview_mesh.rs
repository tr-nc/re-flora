use glam::{Vec3, Vec4};

use crate::{
    geom::RoundCone,
    tree_gen::{Tree, TreeDesc},
    util::stable_perpendicular_basis,
};

use super::{GeometryPreviewMesh, GeometryPreviewVertex};

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const TRUNK_SIDE_COUNT: usize = 4;
const TRUNK_VERTICES_PER_CONE: usize = TRUNK_SIDE_COUNT * 2;
const TRUNK_INDICES_PER_CONE: usize = TRUNK_SIDE_COUNT * 6;
const MAX_LEAF_CLUSTERS: usize = 32;
const LEAF_VERTICES_PER_CLUSTER: usize = 6;
const LEAF_INDICES_PER_CLUSTER: usize = 24;
const TRUNK_BLUEPRINT_COLOR: Vec4 = Vec4::new(0.12, 0.55, 0.95, 0.54);
const LEAF_BLUEPRINT_COLOR: Vec4 = Vec4::new(0.24, 0.82, 1.00, 0.32);

/// Builds a deterministic, low-poly tree preview in world-local coordinates.
///
/// The source tree remains the same voxel-space procedural tree used by normal placement.
/// Each valid trunk cone becomes a four-sided frustum, while a bounded sample of leaf anchors
/// becomes simple octahedral canopy markers.
pub fn build_tree_preview_mesh(desc: &TreeDesc) -> GeometryPreviewMesh {
    let tree = Tree::new(desc.clone());
    let leaf_cluster_count = tree.relative_leaf_positions().len().min(MAX_LEAF_CLUSTERS);
    let mut mesh = GeometryPreviewMesh {
        vertices: Vec::with_capacity(
            tree.trunks()
                .len()
                .saturating_mul(TRUNK_VERTICES_PER_CONE)
                .saturating_add(leaf_cluster_count.saturating_mul(LEAF_VERTICES_PER_CLUSTER)),
        ),
        indices: Vec::with_capacity(
            tree.trunks()
                .len()
                .saturating_mul(TRUNK_INDICES_PER_CONE)
                .saturating_add(leaf_cluster_count.saturating_mul(LEAF_INDICES_PER_CLUSTER)),
        ),
    };

    for cone in tree.trunks() {
        append_trunk_cone(&mut mesh, cone);
    }

    let leaf_radius_voxels = 2.0_f32.powi(desc.leaves_size_level.min(8) as i32) * 0.5;
    let leaf_positions = tree.relative_leaf_positions();
    for sample_index in 0..leaf_cluster_count {
        let source_index = sample_index * leaf_positions.len() / leaf_cluster_count;
        append_leaf_cluster(
            &mut mesh,
            leaf_positions[source_index] / VOXELS_PER_WORLD_UNIT,
            leaf_radius_voxels / VOXELS_PER_WORLD_UNIT,
        );
    }

    mesh
}

fn append_trunk_cone(mesh: &mut GeometryPreviewMesh, cone: &RoundCone) {
    let center_a = cone.center_a() / VOXELS_PER_WORLD_UNIT;
    let center_b = cone.center_b() / VOXELS_PER_WORLD_UNIT;
    let radius_a = cone.radius_a() / VOXELS_PER_WORLD_UNIT;
    let radius_b = cone.radius_b() / VOXELS_PER_WORLD_UNIT;
    let axis = center_b - center_a;

    if !center_a.is_finite()
        || !center_b.is_finite()
        || !radius_a.is_finite()
        || !radius_b.is_finite()
        || radius_a <= 0.0
        || radius_b <= 0.0
        || !axis.is_finite()
    {
        return;
    }

    let Some(direction) = axis.try_normalize() else {
        return;
    };
    let (tangent, _) = stable_perpendicular_basis(direction);
    let Some(bitangent) = direction.cross(tangent).try_normalize() else {
        return;
    };
    let radial_directions = [tangent, bitangent, -tangent, -bitangent];
    let start_positions = radial_directions.map(|radial| center_a + radial * radius_a);
    let end_positions = radial_directions.map(|radial| center_b + radial * radius_b);
    if !start_positions
        .iter()
        .chain(end_positions.iter())
        .all(|position| position.is_finite())
    {
        return;
    }
    let Some(base) = next_vertex_base(mesh, TRUNK_VERTICES_PER_CONE) else {
        return;
    };

    mesh.vertices.extend(
        start_positions.map(|position| GeometryPreviewVertex::new(position, TRUNK_BLUEPRINT_COLOR)),
    );
    mesh.vertices.extend(
        end_positions.map(|position| GeometryPreviewVertex::new(position, TRUNK_BLUEPRINT_COLOR)),
    );

    for side in 0..TRUNK_SIDE_COUNT as u32 {
        let next = (side + 1) % TRUNK_SIDE_COUNT as u32;
        let start = base + side;
        let start_next = base + next;
        let end = base + TRUNK_SIDE_COUNT as u32 + side;
        let end_next = base + TRUNK_SIDE_COUNT as u32 + next;
        mesh.indices
            .extend([start, start_next, end, start_next, end_next, end]);
    }
}

fn append_leaf_cluster(mesh: &mut GeometryPreviewMesh, center: Vec3, radius: f32) {
    if !center.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let positions = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
        .map(|axis| center + axis * radius);
    if !positions.iter().all(|position| position.is_finite()) {
        return;
    }
    let Some(base) = next_vertex_base(mesh, LEAF_VERTICES_PER_CLUSTER) else {
        return;
    };

    mesh.vertices.extend(
        positions.map(|position| GeometryPreviewVertex::new(position, LEAF_BLUEPRINT_COLOR)),
    );
    mesh.indices.extend(
        [
            2, 0, 4, 2, 4, 1, 2, 1, 5, 2, 5, 0, 3, 4, 0, 3, 1, 4, 3, 5, 1, 3, 0, 5,
        ]
        .map(|index| base + index),
    );
}

fn next_vertex_base(mesh: &GeometryPreviewMesh, vertex_count: usize) -> Option<u32> {
    let last_index = mesh
        .vertices
        .len()
        .checked_add(vertex_count)?
        .checked_sub(1)?;
    u32::try_from(last_index).ok()?;
    u32::try_from(mesh.vertices.len()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tree_preview_is_deterministic_and_nonempty() {
        let desc = TreeDesc::default();
        let first = build_tree_preview_mesh(&desc);
        let second = build_tree_preview_mesh(&desc);

        assert!(!first.indices.is_empty());
        assert_eq!(first, second);
    }

    #[test]
    fn preview_vertices_and_indices_are_valid() {
        let mesh = build_tree_preview_mesh(&TreeDesc::default());

        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.iter().all(|value| value.is_finite())
                && vertex.color_alpha.iter().all(|value| value.is_finite())
        }));
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&index| (index as usize) < mesh.vertices.len()));
    }

    #[test]
    fn degenerate_and_non_finite_cones_are_skipped() {
        let mut mesh = GeometryPreviewMesh::default();
        append_trunk_cone(&mut mesh, &RoundCone::new(1.0, Vec3::ZERO, 1.0, Vec3::ZERO));
        append_trunk_cone(
            &mut mesh,
            &RoundCone::new(1.0, Vec3::ZERO, 1.0, Vec3::splat(f32::NAN)),
        );
        append_trunk_cone(
            &mut mesh,
            &RoundCone::new(1.0, Vec3::ZERO, 1.0, Vec3::splat(f32::MAX)),
        );

        assert!(mesh.indices.is_empty());
        assert!(mesh.vertices.is_empty());
    }

    #[test]
    fn trunk_coordinates_convert_from_voxels_to_world_local() {
        let mut mesh = GeometryPreviewMesh::default();
        append_trunk_cone(
            &mut mesh,
            &RoundCone::new(16.0, Vec3::ZERO, 16.0, Vec3::Y * 256.0),
        );

        let min_y = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(min_y, 0.0);
        assert_eq!(max_y, 1.0);
    }

    #[test]
    fn preview_canopy_is_cheaper_than_full_leaf_geometry() {
        let desc = TreeDesc::default();
        let tree = Tree::new(desc.clone());
        let mesh = build_tree_preview_mesh(&desc);
        let preview_leaf_cluster_count =
            tree.relative_leaf_positions().len().min(MAX_LEAF_CLUSTERS);
        let preview_leaf_triangle_count = preview_leaf_cluster_count * 8;

        assert!(preview_leaf_cluster_count <= MAX_LEAF_CLUSTERS);
        assert!(preview_leaf_triangle_count < tree.relative_leaf_placements().len());
        assert_eq!(
            mesh.indices.len() / 3,
            tree.trunks().len() * TRUNK_SIDE_COUNT * 2 + preview_leaf_triangle_count
        );
    }
}
