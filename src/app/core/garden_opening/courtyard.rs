//! Fresh-world courtyard, compiled from the same wall recipe as the drawing tool.
//! Runs before startup terrain publication; loaded worlds never reach this adapter.
use super::*;
use crate::{
    app::{
        low_walls::{LowWallKind, LowWallRecipe},
        path_band::PathSample,
        world_edits::{VoxelEdit, WorldEditTransaction},
    },
    builder::{voxel_type_from_atlas_byte, ModelTriangleGpu, VOXEL_TYPE_DIRT, VOXEL_TYPE_SAND},
};
use anyhow::{ensure, Context, Result};
use glam::UVec3;

const WALL_WIDTH: f32 = 0.08;
const WALL_HEIGHT: f32 = 0.24;

fn segments(size: Vec2) -> [(Vec2, Vec2); 5] {
    let min = size * 0.10;
    let max = size * 0.90;
    let gate = size.x * 0.5;
    let half_gate = 0.13;
    [
        (min, Vec2::new(max.x, min.y)),
        (Vec2::new(max.x, min.y), max),
        (max, Vec2::new(gate + half_gate, max.y)),
        (Vec2::new(gate - half_gate, max.y), Vec2::new(min.x, max.y)),
        (Vec2::new(min.x, max.y), min),
    ]
}

impl App {
    pub(in crate::app::core) fn initialize_garden_courtyard(&mut self, seed: u32) -> Result<()> {
        ensure!(
            !self.terrain_persistence.startup_load_requested(),
            "cannot rebuild a loaded courtyard"
        );
        let dim = self.world_chunk_dim * VOXEL_DIM_PER_CHUNK;
        let size = Vec2::new(dim.x as f32, dim.z as f32) / 256.;
        let recipe = LowWallRecipe::new(LowWallKind::Chinese, WALL_WIDTH, WALL_HEIGHT, 0.05, seed);
        let mut plaster = Vec::new();
        let mut tiles = Vec::new();
        let mut columns = 0;
        // Read the natural substrate before any wall writes. Each narrow side
        // includes its full vertical range; no procedural-height duplicate.
        for (start, end) in segments(size) {
            let min = (start.min(end) - Vec2::splat(WALL_WIDTH)).max(Vec2::ZERO);
            let max = (start.max(end) + Vec2::splat(WALL_WIDTH)).min(size);
            let offset = UVec3::new(
                (min.x * 256.).floor() as u32,
                0,
                (min.y * 256.).floor() as u32,
            );
            let end_voxel = UVec3::new(
                (max.x * 256.).ceil() as u32,
                dim.y,
                (max.y * 256.).ceil() as u32,
            );
            let extent = end_voxel - offset;
            let bytes = self.plain_builder.read_chunk_atlas_region(offset, extent)?;
            let ground = |point: Vec2| -> Result<f32> {
                let p = (point * 256.).floor().as_uvec2();
                let x =
                    p.x.checked_sub(offset.x)
                        .context("courtyard sample outside region")?;
                let z =
                    p.y.checked_sub(offset.z)
                        .context("courtyard sample outside region")?;
                ensure!(
                    x < extent.x && z < extent.z,
                    "courtyard sample outside region"
                );
                let y = (0..extent.y)
                    .rev()
                    .find(|&y| {
                        let index = ((z * extent.y + y) * extent.x + x) as usize;
                        matches!(
                            u32::from(voxel_type_from_atlas_byte(bytes[index])),
                            VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND
                        )
                    })
                    .context("courtyard has no natural soil support")?;
                Ok((y as f32 + 1.) / 256.)
            };
            let count = (start.distance(end) / recipe.pitch).ceil() as u32;
            let pitch = start.distance(end) / count as f32;
            let forward = (end - start).normalize();
            for i in 0..count {
                let center = start + forward * pitch * (i as f32 + 0.5);
                let row = PathSample::straight(
                    Vec3::new(center.x, 0., center.y),
                    Vec3::new(forward.x, 0., forward.y),
                    i as f32 * pitch,
                    i,
                    pitch,
                );
                let pieces = recipe.pieces(row);
                ensure!(pieces.len() == recipe.layers(), "invalid courtyard column");
                for piece in pieces {
                    let mut heights = [0.; 4];
                    for (i, point) in piece.cell.footprint.iter().enumerate() {
                        heights[i] = ground(*point)?;
                    }
                    let solid = piece.solid(heights);
                    ensure!(
                        solid
                            .positions
                            .iter()
                            .all(|p| p.cmpgt(Vec3::ZERO).all()
                                && p.cmplt(dim.as_vec3() / 256.).all()),
                        "courtyard outside world"
                    );
                    let target = if piece.layer == 0 {
                        &mut plaster
                    } else {
                        &mut tiles
                    };
                    target.extend(solid.triangles().map(|[a, b, c]| ModelTriangleGpu {
                        a: a.extend(0.).to_array(),
                        b: b.extend(0.).to_array(),
                        c: c.extend(0.).to_array(),
                    }));
                }
                columns += 1;
            }
        }
        // Material batches remain collections of closed solids. The normal
        // loading publication builds CPU picking, rendering and colliders once.
        let mut edits = Vec::new();
        for (triangles, material) in [
            (plaster, crate::builder::VOXEL_MATERIAL_IRON_RED),
            (tiles, crate::builder::VOXEL_TYPE_ASPHALT),
        ] {
            // Bound each upload independently of world size / staging capacity.
            for batch in triangles.chunks(1200) {
                edits.push(VoxelEdit::StampModel {
                    triangles: batch.to_vec(),
                    position: Vec3::ZERO,
                    material,
                });
            }
        }
        // One atlas read for the whole foundation, not one GPU readback per column.
        let (offset, extent) = super::temple::ground_region(dim);
        let bytes = self.plain_builder.read_chunk_atlas_region(offset, extent)?;
        edits.extend(super::temple::plan(dim, |column| {
            let x = column
                .x
                .checked_sub(offset.x)
                .context("temple sample outside region")?;
            let z = column
                .y
                .checked_sub(offset.z)
                .context("temple sample outside region")?;
            ensure!(x < extent.x && z < extent.z, "temple sample outside region");
            (0..extent.y)
                .rev()
                .find(|&y| {
                    let index = ((z * extent.y + y) * extent.x + x) as usize;
                    matches!(
                        u32::from(voxel_type_from_atlas_byte(bytes[index])),
                        VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND
                    )
                })
                .map(|y| y as f32 + 1.)
                .context("temple has no natural soil support")
        })?);
        self.execute_world_edit(WorldEditTransaction::during_loading(edits))?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!("[COURTYARD] initialized columns={columns} wall=chinese_red tile_cap=pitched gate_width=0.26 editable=true fresh_only=true");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn courtyard_is_square_with_a_walkable_centered_front_gate() {
        let sides = segments(Vec2::splat(2.));
        assert_eq!(sides[0], (Vec2::splat(0.2), Vec2::new(1.8, 0.2)));
        assert_eq!(sides[1].1, sides[2].0);
        assert_eq!(sides[3].1, sides[4].0);
        assert_eq!(sides[4].1, sides[0].0);
        let gap = sides[2].1.x - sides[3].0.x;
        assert!((gap - 0.26).abs() < 1e-6);
        assert!((sides[2].1.x + sides[3].0.x - 2.).abs() < 1e-6);
        assert!(gap - WALL_WIDTH > 0.1);
    }
}
