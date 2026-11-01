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
const GINKGO_SIZE: f32 = 48.;

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

pub(super) fn ginkgo_description(seed: u32) -> TreeDesc {
    let mut desc = TreeSpecies::Ginkgo.default_desc();
    desc.branching.seed = layout::tree_individual(seed, 100);
    // An authored mature specimen, not a hidden change to the Grow recipe.
    desc.size = GINKGO_SIZE;
    desc.form = crate::tree_gen::TreeForm::Veteran;
    desc.trunk_thickness = 0.48;
    desc.leaves_size_level = 5;
    desc.leaf_density = 0.08;
    desc.leaf_spray_width_ratio = 1.2;
    desc.leaf_spray_thickness_ratio = 0.58;
    desc.leaf_spray_tip_offset_ratio = 0.05;
    desc
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
        edits.push(self.courtyard_root_soil(seed, size, dim)?);
        self.execute_world_edit(WorldEditTransaction::during_loading(edits))?;
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!("[COURTYARD] initialized columns={columns} wall=chinese_red tile_cap=pitched gate_width=0.26 editable=true fresh_only=true");
        Ok(())
    }

    fn courtyard_root_soil(&mut self, seed: u32, size: Vec2, world: UVec3) -> Result<VoxelEdit> {
        // This is an authored planting bed in a fresh world, not a relaxation of
        // Grow's material protection. A veteran root flare needs deeper soil
        // than the meadow generator's shallow topsoil over rock.
        let tree = crate::tree_gen::Tree::new(ginkgo_description(seed));
        let base = tree.trunks()[0].aabb();
        let xz = size * Vec2::new(0.5, 0.43) * 256.;
        let radius = tree.trunks()[0].radius_a().ceil() as u32 + 3;
        let min = base.min() + Vec3::new(xz.x, 0., xz.y);
        let max = base.max() + Vec3::new(xz.x, 0., xz.y);
        let offset = UVec3::new(min.x.floor() as u32, 0, min.z.floor() as u32);
        let dim = UVec3::new(max.x.ceil() as u32, world.y, max.z.ceil() as u32) - offset;
        let mut bytes = self.plain_builder.read_chunk_atlas_region(offset, dim)?;
        let mut converted = 0;
        for z in 0..dim.z {
            for x in 0..dim.x {
                let index = |y| ((z * dim.y + y) * dim.x + x) as usize;
                let surface = (0..dim.y)
                    .rev()
                    .find(|&y| {
                        matches!(
                            u32::from(voxel_type_from_atlas_byte(bytes[index(y)])),
                            VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND
                        )
                    })
                    .context("veteran planting bed has no natural soil")?;
                for y in surface.saturating_sub(radius)..=surface {
                    let byte = &mut bytes[index(y)];
                    if u32::from(voxel_type_from_atlas_byte(*byte))
                        == crate::builder::VOXEL_TYPE_ROCK
                    {
                        *byte = VOXEL_TYPE_DIRT as u8;
                        converted += 1;
                    }
                }
            }
        }
        log::info!("[COURTYARD] prepared_root_soil=true depth_voxels={radius} natural_rock_to_soil={converted} surface_preserved=true");
        Ok(VoxelEdit::RestoreAtlasRegion {
            origin: offset,
            dim,
            data: bytes.into(),
        })
    }

    pub(super) fn prepare_courtyard_ginkgo(
        &mut self,
        seed: u32,
        size: Vec2,
        batch: &mut super::super::vegetation::TreePlantingBatch,
    ) -> Result<()> {
        let xz = size * Vec2::new(0.50, 0.43);
        let mut position = Vec3::new(xz.x, self.query_terrain_height_cpu(xz), xz.y);
        let desc = ginkgo_description(seed);
        let tree = crate::tree_gen::Tree::new(desc.clone());
        // A broad basal flare spans several natural soil heights. Seat the
        // authored root on its whole footprint, not a single low center column;
        // retain ordinary planting's obstruction checks and never erase terrain.
        let base = tree.trunks()[0].aabb();
        let min = base.min() + position * 256.;
        let max = base.max() + position * 256.;
        for z in min.z.floor() as u32..=max.z.ceil() as u32 {
            for x in min.x.floor() as u32..=max.x.ceil() as u32 {
                let p = Vec2::new(x as f32 + 0.5, z as f32 + 0.5) / 256.;
                position.y = position.y.max(self.query_terrain_height_cpu(p));
            }
        }
        let height = tree.height_voxels();
        let id = self.prepare_tree_planting(batch, desc, position, true)?;
        log::info!("[COURTYARD] ginkgo_id={id} mature=true size={GINKGO_SIZE} height_voxels={height:.3} position={position:?}");
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
    #[test]
    fn courtyard_ginkgo_is_large_reproducible_and_persistable() {
        let desc = ginkgo_description(layout::DEFAULT_SEED);
        assert_eq!(desc.species, TreeSpecies::Ginkgo);
        assert!(desc.size > TreeSpecies::Ginkgo.default_desc().size);
        assert_eq!(desc.fruit_spawn_probability, 0.);
        assert_eq!(desc, ginkgo_description(layout::DEFAULT_SEED));
        let tree = crate::tree_gen::Tree::new(desc.clone());
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for cone in tree.world_trunks(Vec3::new(2., 0.5234375, 1.72)) {
            low = low.min(cone.aabb().min());
            high = high.max(cone.aabb().max());
        }
        assert!(low.cmpgt(Vec3::ZERO).all());
        assert!(high.cmplt(Vec3::new(1024., 512., 1024.)).all());
        assert!(tree.height_voxels() / 256. > WALL_HEIGHT * 2.);
        assert!(tree.height_voxels() / 256. < 1.4);
        assert!(layout::tree_radius_voxels(&desc) / 256. < 1.2);
        let restored: TreeDesc =
            serde_json::from_str(&serde_json::to_string(&desc).unwrap()).unwrap();
        assert_eq!(desc, restored);
    }
}
