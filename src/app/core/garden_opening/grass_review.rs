//! Opt-in native regression: an opening meadow must survive mow/replant and
//! overlapping player strokes without changing its world-space species layout.
use super::*;
use std::collections::BTreeSet;

type GrassRoots = BTreeSet<([u32; 3], usize, u32)>;

fn grass_roots(app: &App) -> GrassRoots {
    app.surface_builder
        .resources
        .instances
        .chunk_flora_instances
        .iter()
        .flat_map(|(_, chunk)| {
            chunk.hydration[..2]
                .iter()
                .enumerate()
                .flat_map(move |(species, roots)| {
                    roots
                        .iter()
                        .map(move |(root, _)| (chunk.chunk_id.to_array(), species, *root))
                })
        })
        .collect()
}

impl App {
    // Run immediately after the real opening meadow is planted, before authored
    // flowers, trees and intentionally mown paths change its available surface.
    pub(super) fn review_opening_grass_distribution(&mut self) -> anyhow::Result<()> {
        if std::env::var_os("RE_FLORA_GARDEN_GRASS_AUDIT").is_none() {
            return Ok(());
        }
        anyhow::ensure!(self.world_chunk_dim.x > 1, "review needs a chunk boundary");
        let original = grass_roots(self);
        anyhow::ensure!(!original.is_empty(), "opening meadow is empty");
        let saved = self.surface_builder.capture_flora_snapshot(0)?;
        let selection = self.player_tools.planting.flora_index;
        self.player_tools.planting.flora_index = 0; // Player's ordinary Grass Mix.
        let result = (|| -> anyhow::Result<()> {
            let world = self.world_chunk_dim * VOXEL_DIM_PER_CHUNK;
            let z = world.z as f32 / 256.0 * 0.65;
            let mut checked = 0;
            for (i, xz) in [Vec2::new(1.0, z), Vec2::new(1.025, z + 0.035)]
                .into_iter()
                .enumerate()
            {
                let center = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
                let edit = TerrainBrushEdit {
                    start: center,
                    end: center + Vec3::Z * 0.04,
                    radius: 0.08,
                };
                self.apply_surface_flora_removal(edit)?;
                let mown = grass_roots(self);
                let removed: Vec<_> = original.difference(&mown).copied().collect();
                anyhow::ensure!(removed.len() > 100, "review did not mow a grass patch");
                let chunks: BTreeSet<_> = removed.iter().map(|root| root.0).collect();
                anyhow::ensure!(chunks.len() >= 2, "review did not cross chunk boundaries");
                self.apply_surface_flora_regeneration_at(edit, 731 + i as u32, true, 1000)?;
                let replanted = grass_roots(self);
                let missing = original.difference(&replanted).count();
                let unexpected = replanted.difference(&original).count();
                anyhow::ensure!(
                    missing == 0 && unexpected == 0,
                    "opening/player grass distribution differs: missing={missing} unexpected={unexpected} removed={}",
                    removed.len()
                );
                anyhow::ensure!(
                    removed.iter().any(|root| root.1 == 0)
                        && removed.iter().any(|root| root.1 == 1),
                    "meadow patch must contain both tall and short grass"
                );
                // Repeat with a different stroke serial/time. Existing grass and
                // the surrounding untouched meadow must remain exactly unchanged.
                self.apply_surface_flora_regeneration_at(edit, 918 + i as u32, true, 2000)?;
                anyhow::ensure!(
                    grass_roots(self) == original,
                    "overpaint changed grass layout"
                );
                checked += removed.len();
            }
            log::info!("[GRASS_DISTRIBUTION_REVIEW] checked_roots={checked} mixed=true mow_replant=identical overlapping_strokes=identical cross_chunk=true untouched_roots=preserved");
            Ok(())
        })();
        self.player_tools.planting.flora_index = selection;
        self.surface_builder.restore_flora_snapshot(&saved, 0)?;
        result
    }
}
