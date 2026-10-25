//! Native saved-atlas checks for the retired-tree migration, opt-in via garden snapshot smoke.
use super::*;
use crate::app::core::VOXEL_DIM_PER_CHUNK;
use crate::terrain_persistence::TerrainSnapshotReader;

impl App {
    pub(in crate::app::core) fn verify_retired_tree_terrain(
        &mut self,
        snapshot: &TreeSnapshot,
        reader: &mut TerrainSnapshotReader,
    ) -> Result<()> {
        let original: Vec<_> = snapshot
            .trees
            .iter()
            .filter(|saved| saved.mature_desc.is_retired())
            .flat_map(|saved| {
                Tree::new(saved.mature_desc.saved_geometry_desc().at_age(snapshot.age))
                    .world_trunks(Vec3::from_array(saved.position))
            })
            .collect();
        let current: Vec<_> = self
            .trees
            .records
            .values()
            .flat_map(|record| record.trunk_geometry.round_cones.iter().cloned())
            .collect();
        let near = |cones: &[crate::geom::RoundCone], point, margin| {
            cones
                .iter()
                .any(|cone| cone.signed_distance(point) <= margin)
        };
        let mut changed = 0usize;
        let mut old_interior_probes = 0usize;
        while let Some(chunk) = reader.read_next_chunk()? {
            let origin = UVec3::from_array(chunk.coordinate) * VOXEL_DIM_PER_CHUNK;
            let bytes = self
                .plain_builder
                .read_chunk_atlas_region(origin, VOXEL_DIM_PER_CHUNK)?;
            anyhow::ensure!(
                bytes.len() == chunk.bytes.len(),
                "migration chunk length changed"
            );
            for (index, (&before, &after)) in chunk.bytes.iter().zip(&bytes).enumerate() {
                let before = u32::from(before);
                let after = u32::from(after);
                if before == after && before != VOXEL_TYPE_CHERRY_WOOD {
                    continue;
                }
                let index = index as u32;
                let point = (origin
                    + UVec3::new(
                        index % VOXEL_DIM_PER_CHUNK.x,
                        (index / VOXEL_DIM_PER_CHUNK.x) % VOXEL_DIM_PER_CHUNK.y,
                        index / (VOXEL_DIM_PER_CHUNK.x * VOXEL_DIM_PER_CHUNK.y),
                    ))
                .as_vec3()
                    + Vec3::splat(0.5);
                // Two-voxel tolerance accounts for sample-center/voxel boundary differences.
                // This is a bounded-change check, not a CPU reimplementation of voxelization.
                let belongs_to_current = near(&current, point, 2.);
                if before != after {
                    anyhow::ensure!(
                        after == VOXEL_TYPE_CHERRY_WOOD
                            || (before == VOXEL_TYPE_CHERRY_WOOD && after == VOXEL_TYPE_EMPTY),
                        "migration changed a non-wood material at {point:?}: {before} -> {after}"
                    );
                    anyhow::ensure!(
                        belongs_to_current || near(&original, point, 2.),
                        "migration changed terrain away from tree wood at {point:?}"
                    );
                    changed += 1;
                }
                if before == VOXEL_TYPE_CHERRY_WOOD
                    && !belongs_to_current
                    && near(&original, point, -1.)
                {
                    old_interior_probes += 1;
                    anyhow::ensure!(
                        after != VOXEL_TYPE_CHERRY_WOOD,
                        "retired tree left an interior wood ghost at {point:?}"
                    );
                }
            }
        }
        log::info!("[TREE_MIGRATION_REVIEW] changed_voxels={changed} old_interior_probes={old_interior_probes} old_wood_ghosts=0 changes=tree_wood_only");
        Ok(())
    }
}
