//! Generation-bound rest-skin attachment for a rigid bee house. Reuses the
//! canonical tree pose; no new per-frame GPU readback or duplicate wind solver.
use super::{GardenTrees, TreeRecord};
use crate::tree_gen::skin::SkinBinding;
use glam::{Mat3, Quat, UVec3, Vec3};
fn owns_support(record: &TreeRecord, support: UVec3) -> bool {
    if !support.cmpge(record.bound.min()).all() || !support.cmplt(record.bound.max()).all() {
        return false;
    }
    let local = support.as_vec3() + Vec3::splat(0.5) - record.position * 256.;
    record
        .rest_tree
        .trunks()
        .iter()
        .any(|c| c.signed_distance(local) <= 0.75)
}
impl GardenTrees {
    /// Voxel-fallback picks carry no raster primitive. Overlapping wood uses the
    /// first stable tree ID; metadata never silently switches hosts afterward.
    pub(in crate::app::core) fn hive_tree_at(&self, support: UVec3) -> Option<u32> {
        self.records
            .iter()
            .find_map(|(&id, record)| owns_support(record, support).then_some(id))
    }
    pub(in crate::app::core) fn resolve_hive_pose(
        &self,
        id: u32,
        support: UVec3,
        rest: Vec3,
        facing: Vec3,
        posed: bool,
        cached: &mut Option<(u64, SkinBinding)>,
    ) -> Option<(Vec3, Quat)> {
        let record = self.records.get(&id)?;
        let generation = record.canopy_acoustic_descriptor.generation();
        if cached.is_none_or(|(old, _)| old != generation) {
            let local = support.as_vec3() + Vec3::splat(0.5) - record.position * 256.;
            if !owns_support(record, support) {
                return None;
            }
            *cached = Some((
                generation,
                SkinBinding::at_rest_position(&record.rest_tree, local).ok()?,
            ));
        }
        if !posed {
            return Some((rest, Quat::from_rotation_y(facing.x.atan2(facing.z))));
        }
        let transform = cached.as_ref()?.1.transform(record.pose.branches()).ok()?;
        let forward = transform.normal(facing);
        let right = transform.normal(Vec3::Y).cross(forward).normalize_or_zero();
        let up = forward.cross(right).normalize_or_zero();
        Some((
            transform.point(rest),
            Quat::from_mat3(&Mat3::from_cols(right, up, forward)).normalize(),
        ))
    }
}
