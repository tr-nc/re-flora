use super::super::App;
use super::coordinator::WaterFrameRequest;
use crate::app::world_edits::TerrainRemovalEdit;
use crate::builder::VOXEL_TYPE_ROCK;
use glam::{Vec2, Vec3};

#[derive(Clone, Debug, Default)]
pub(in crate::app::core) struct WaterEditSoak {
    next_step: usize,
}

#[derive(Clone, Copy, Debug)]
enum WaterEditSoakOp {
    Remove,
    PlaceRock,
}

#[derive(Clone, Copy, Debug)]
struct WaterEditSoakStep {
    delay_sec: f32,
    label: &'static str,
    xz: Vec2,
    radius: f32,
    op: WaterEditSoakOp,
}

impl App {
    pub(in crate::app::core) fn update_water_sim(
        &mut self,
        frame_delta_time: f32,
        world_tick_seconds: f32,
    ) {
        self.water.advance_frame(
            &self.debug_settings.adjustables,
            WaterFrameRequest {
                frame_delta_time,
                world_tick_seconds,
                world_tick_multiplier: self
                    .debug_settings
                    .adjustables
                    .water_world_tick_multiplier
                    .value,
                perf_logging: self.perf_logging,
            },
        );
    }

    pub(in crate::app::core) fn process_water_edit_soak(&mut self) {
        let Some(render_start) = self.render_start_time else {
            return;
        };
        let Some(current_step) = self.water_edit_soak.as_ref().map(|soak| soak.next_step) else {
            return;
        };
        let Some(step) = water_edit_soak_step(current_step) else {
            return;
        };

        if render_start.elapsed().as_secs_f32() < step.delay_sec {
            return;
        }
        if !self.water.terrain_status().is_ready() {
            return;
        }

        let next_step = current_step + 1;
        if let Err(err) = self.apply_water_edit_soak_step(step) {
            log::error!(
                "[WATER][EDIT_SOAK] step {} ({}) failed: {}",
                current_step,
                step.label,
                err,
            );
        }
        if let Some(soak) = &mut self.water_edit_soak {
            soak.next_step = next_step;
            if water_edit_soak_step(soak.next_step).is_none() {
                log::info!("[WATER][EDIT_SOAK] completed deterministic terrain-edit sequence");
            }
        }
    }

    fn apply_water_edit_soak_step(&mut self, step: WaterEditSoakStep) -> anyhow::Result<()> {
        let terrain_height = self.query_terrain_height_cpu(step.xz);
        let center = Vec3::new(step.xz.x, terrain_height, step.xz.y);
        match step.op {
            WaterEditSoakOp::Remove => {
                let readback = self.apply_surface_terrain_removal(
                    TerrainRemovalEdit {
                        center,
                        radius: step.radius,
                    },
                    None,
                    Some(65_536),
                    None,
                )?;
                let removed: u32 = readback.stats.removed_counts.iter().sum();
                log::info!(
                    "[WATER][EDIT_SOAK] applied {} center=({:.3},{:.3},{:.3}) radius={:.3} removed_voxels={} sampled_positions={}",
                    step.label,
                    center.x,
                    center.y,
                    center.z,
                    step.radius,
                    removed,
                    readback.sampled_positions_world.len(),
                );
            }
            WaterEditSoakOp::PlaceRock => {
                let readback = self.apply_surface_terrain_placement(
                    TerrainRemovalEdit {
                        center,
                        radius: step.radius,
                    },
                    VOXEL_TYPE_ROCK,
                    65_536,
                )?;
                let added: u32 = readback.stats.added_counts.iter().sum();
                log::info!(
                    "[WATER][EDIT_SOAK] applied {} center=({:.3},{:.3},{:.3}) radius={:.3} added_voxels={} sampled_positions={}",
                    step.label,
                    center.x,
                    center.y,
                    center.z,
                    step.radius,
                    added,
                    readback.sampled_positions_world.len(),
                );
            }
        }
        Ok(())
    }
}

fn water_edit_soak_step(step: usize) -> Option<WaterEditSoakStep> {
    match step {
        0 => Some(WaterEditSoakStep {
            delay_sec: 2.0,
            label: "shore-dig-a",
            xz: Vec2::new(1.34, 1.42),
            radius: 0.055,
            op: WaterEditSoakOp::Remove,
        }),
        1 => Some(WaterEditSoakStep {
            delay_sec: 4.0,
            label: "shore-rock-place",
            xz: Vec2::new(1.66, 1.38),
            radius: 0.050,
            op: WaterEditSoakOp::PlaceRock,
        }),
        2 => Some(WaterEditSoakStep {
            delay_sec: 6.0,
            label: "shore-dig-b",
            xz: Vec2::new(1.52, 1.70),
            radius: 0.055,
            op: WaterEditSoakOp::Remove,
        }),
        _ => None,
    }
}
