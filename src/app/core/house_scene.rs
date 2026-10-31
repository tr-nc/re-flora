//! Isolated, unsaved visit to the reference-inspired two-storey flower cottage.
mod architecture;

use super::{planting::AuthoredFloraPlacementBatch, App};
use crate::{
    builder::{
        voxel_type_from_atlas_byte, PlainBuilder, VOXEL_TYPE_DIRT, VOXEL_TYPE_GLASS,
        VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND,
    },
    gameplay::{camera::PlayerWalkMovementRequest, CameraPose},
};
use anyhow::{ensure, Context, Result};
use architecture::{DOOR_X, FRONT_Z, MAX_X, MIN_X, MIN_Z, ORIGIN, PLANTERS, WINDOWS};
use glam::{UVec3, Vec3};

#[derive(Clone, Copy, Debug)]
struct Ground {
    min: u32,
    max: u32,
    approach: u32,
}

fn sample_ground(builder: &mut PlainBuilder) -> Result<Ground> {
    let offset = UVec3::new(MIN_X as u32, 32, MIN_Z as u32) + ORIGIN.as_uvec3();
    let dim = UVec3::new(
        (MAX_X - MIN_X) as u32 + 1,
        224,
        (FRONT_Z - MIN_Z) as u32 + 1,
    );
    let bytes = builder.read_chunk_atlas_region(offset, dim)?;
    let mut min = u32::MAX;
    let mut max = 0;
    let mut approach = None;
    for z in 0..dim.z {
        for x in 0..dim.x {
            let y = (0..dim.y)
                .rev()
                .find(|&y| {
                    let index = ((z * dim.y + y) * dim.x + x) as usize;
                    matches!(
                        u32::from(voxel_type_from_atlas_byte(bytes[index])),
                        VOXEL_TYPE_DIRT | VOXEL_TYPE_ROCK | VOXEL_TYPE_SAND
                    )
                })
                .with_context(|| format!("no natural terrain below cottage column x={x} z={z}"))?
                + offset.y;
            min = min.min(y);
            max = max.max(y);
            if x == (DOOR_X - MIN_X) as u32 && z == dim.z - 1 {
                approach = Some(y);
            }
        }
    }
    Ok(Ground {
        min,
        max,
        approach: approach.context("missing cottage approach ground")?,
    })
}

fn world(local_voxels: Vec3) -> Vec3 {
    (local_voxels + ORIGIN) / 256.
}

fn view(position: Vec3, target: Vec3, fov_deg: f32) -> CameraPose {
    let d = target - position;
    CameraPose {
        position: world(position),
        yaw_deg: d.x.atan2(-d.z).to_degrees(),
        pitch_deg: d.y.atan2(glam::Vec2::new(d.x, d.z).length()).to_degrees(),
        fov_deg,
    }
}

impl App {
    pub(super) fn apply_house_scene(&mut self) -> Result<()> {
        self.set_manual_time_of_day(0.36);
        let ground = sample_ground(&mut self.plain_builder)?;
        let base = ground.max as f32 + 1.;
        self.execute_world_edit(architecture::plan(
            base,
            ground.min as f32,
            ground.approach as f32,
        )?)?;
        self.cottage_base_y = Some(base);
        self.plain_builder.mark_all_solid_workgroups_dirty();
        log::info!("[HOUSE_SCENE] built half_timber_flower_cottage base_y={base} natural_surface={}..{} storeys=2 roof=curved_terracotta dormer=true balcony=true windows=6 dedicated_glass=true pane_thickness=2 lanterns=3 persistence=disabled", ground.min, ground.max);
        Ok(())
    }

    pub(super) fn finish_house_garden(&mut self) -> Result<()> {
        let base = self.cottage_base_y.context("missing cottage foundation")?;
        let species = crate::garden_layout::FLOWER_KEYS.map(|key| {
            crate::flora::species::species()
                .iter()
                .position(|s| s.key == key)
                .expect("cottage flowers must come from the existing garden catalog")
                as u32
        });
        // Ordinary meadow patches frame the building without painting its roof or
        // upper flower boxes. The entrance keeps an actual mown approach.
        for (x, z, radius) in [
            (180., 467., 100.),
            (360., 300., 105.),
            (10., 300., 100.),
            (180., 120., 100.),
        ] {
            let center = world(Vec3::new(x, base - 10., z));
            self.seed_opening_grass(
                crate::app::world_edits::TerrainBrushEdit {
                    start: center,
                    end: center,
                    radius: radius / 256.,
                },
                7319,
            )?;
        }
        self.apply_surface_flora_removal(crate::app::world_edits::TerrainBrushEdit {
            start: world(Vec3::new(DOOR_X, base, FRONT_Z)),
            end: world(Vec3::new(DOOR_X, base - 10., 510.)),
            radius: 24. / 256.,
        })?;
        let mut batch = AuthoredFloraPlacementBatch::new();
        let now = self.time_info.time_since_start_duration().as_millis() as u32;
        let mut count = 0;
        for (index, p) in PLANTERS.into_iter().enumerate() {
            for z in (p.min_z + 2..p.max_z - 1).step_by(4) {
                for x in (p.min_x + 2..p.max_x - 1).step_by(6) {
                    let contact = world(Vec3::new(x as f32 + 0.5, base + p.top, z as f32 + 0.5));
                    let anchor = self.resolve_plantable_contact(contact)?.with_context(|| {
                        format!("cottage planter {index} has no exposed soil at {contact:?}")
                    })?;
                    let seed = crate::garden_layout::hash(x ^ (z << 12) ^ (index as u32 * 7919));
                    ensure!(
                        self.try_place_authored_flora(
                            &mut batch,
                            species[(count + index) % species.len()],
                            anchor,
                            255,
                            now,
                            seed
                        ),
                        "cottage flower root was rejected"
                    );
                    count += 1;
                }
            }
        }
        self.finish_authored_flora_placement(batch)?;
        log::info!("[HOUSE_SCENE][GARDEN] authored_flowers={count} planters={} existing_species={:?} substrate_validated=true", PLANTERS.len(), crate::garden_layout::FLOWER_KEYS);
        self.verify_house_passage()?;
        self.apply_house_camera(base);
        Ok(())
    }

    fn apply_house_camera(&mut self, base: f32) {
        let name = self
            .launch_owners
            .snapshot_name()
            .unwrap_or("cozy-home-arrival");
        // Explicit user snapshots retain authority; only built-ins follow the foundation.
        if !crate::app::camera_snapshots::is_house_camera_preset(name) {
            return;
        }
        let (pose, fly) = match name {
            "cozy-home-interior" => (
                view(
                    Vec3::new(178., base + 28., 333.),
                    Vec3::new(205., base + 25., 276.),
                    68.,
                ),
                true,
            ),
            "cozy-home-balcony" => (
                view(
                    Vec3::new(302., base + 104., 438.),
                    Vec3::new(206., base + 90., 354.),
                    56.,
                ),
                true,
            ),
            "cozy-home-exterior" | "house-overlook" => (
                view(
                    Vec3::new(380., base + 156., 680.),
                    Vec3::new(180., base + 99., 304.),
                    45.,
                ),
                true,
            ),
            _ => {
                let xz = world(Vec3::new(335., 0., 660.));
                let xz = glam::Vec2::new(xz.x, xz.z);
                let y = self.query_terrain_height_cpu(xz) * 256. + 20.5;
                (
                    view(
                        Vec3::new(335., y, 660.),
                        Vec3::new(180., base + 94., 310.),
                        52.,
                    ),
                    false,
                )
            }
        };
        self.tracer.apply_camera_pose(pose);
        self.camera_control.apply_snapshot_mode(fly);
        log::info!(
            "[HOUSE_SCENE][CAMERA] preset={name} foundation_relative=true fly={fly} position={:?}",
            pose.position
        );
    }

    fn verify_house_passage(&mut self) -> Result<()> {
        let base = self.cottage_base_y.context("missing cottage floor")?;
        let floor = self
            .query_terrain_ray_cpu(world(Vec3::new(DOOR_X, base + 24., 320.)), Vec3::NEG_Y)
            .context("cottage has no interior floor")?;
        let eye = floor.position.y + 0.08;
        let hit = self
            .query_terrain_ray_cpu(world(Vec3::new(DOOR_X, eye * 256., 410.)), Vec3::NEG_Z)
            .context("cottage has no back wall")?;
        ensure!(
            hit.position.z < (340. + ORIGIN.z) / 256.,
            "cottage entrance is blocked: {hit:?}"
        );
        for w in WINDOWS {
            let origin = world(Vec3::new(
                w.x - w.width * 0.23,
                base + w.bottom + w.height * 0.7,
                w.z + 18.,
            ));
            let glass = self
                .query_terrain_ray_cpu(origin, Vec3::NEG_Z)
                .context("missing cottage window")?;
            ensure!(
                glass.voxel_type == VOXEL_TYPE_GLASS,
                "cottage pane is not dedicated Glass: {glass:?}"
            );
        }
        let walk = self.terrain_physics.move_player_capsule(
            PlayerWalkMovementRequest {
                camera_position: world(Vec3::new(DOOR_X, eye * 256. + 0.256, 360.)),
                camera_height: 0.08,
                desired_translation: Vec3::new(0., 0., -56. / 256.),
            },
            0.25,
        )?;
        ensure!(
            walk.translation.z < -50. / 256.,
            "player capsule cannot enter cottage: {walk:?}"
        );
        let approach_ground = self
            .query_terrain_ray_cpu(world(Vec3::new(DOOR_X, 360., 490.)), Vec3::NEG_Y)
            .context("missing cottage garden approach")?;
        let mut position = approach_ground.position + Vec3::Y * 0.081;
        for _ in 0..190 {
            let movement = self.terrain_physics.move_player_capsule(
                PlayerWalkMovementRequest {
                    camera_position: position,
                    camera_height: 0.08,
                    desired_translation: Vec3::new(0., -0.05 / 60., -1. / 256.),
                },
                1. / 60.,
            )?;
            position += movement.translation;
        }
        ensure!(
            position.z < (320. + ORIGIN.z) / 256.,
            "player cannot walk up cottage steps: {position:?}"
        );
        log::info!("[HOUSE_SCENE][PASSAGE] published_cpu_collision=true dedicated_glass_front_panes=4 capsule_entered=true garden_approach_final_voxels={:?}", position * 256.);
        // Walk the ordinary collision capsule up every tread, not a teleported
        // upper-floor probe. This catches blocked stairwells and wrong risers.
        let mut upstairs = world(Vec3::new(104., base + 20.8, 246.));
        for _ in 0..151 {
            let movement = self.terrain_physics.move_player_capsule(
                PlayerWalkMovementRequest {
                    camera_position: upstairs,
                    camera_height: 0.08,
                    desired_translation: Vec3::new(1. / 256., -0.05 / 60., 0.),
                },
                1. / 60.,
            )?;
            upstairs += movement.translation;
        }
        ensure!(
            upstairs.y * 256. >= base + architecture::FLOOR_HEIGHT + 19.
                && upstairs.x * 256. > ORIGIN.x + 249.,
            "player cannot reach cottage upper floor: {upstairs:?}"
        );
        log::info!("[HOUSE_SCENE][STAIRS] production_capsule=true upper_floor_reached=true final_voxels={:?}", upstairs * 256.);
        Ok(())
    }
}
