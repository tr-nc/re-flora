//! Cursor-guided mower: the pointer supplies a destination, never a teleport or a speed.
use glam::{Mat3, Quat, Vec2, Vec3};
use winit::event::{ElementState, MouseButton};

use super::{player_tools::PlayerTool, App};
use crate::app::world_edits::TerrainBrushEdit;

pub(super) const MOWER_MAX_SPEED: f32 = 30.0 / 256.0;
pub(super) const MOWER_CUT_RADIUS: f32 = 11.0 / 256.0;
const MIN_CAMERA_DISTANCE: f32 = 0.7;
const MAX_FRAME_STEP: f32 = 0.1;
const MOWER_MAX_TURN_SPEED: f32 = 120. * std::f32::consts::PI / 180.;
const MOWER_APPROACH_RATE: f32 = 3.;
const MOWER_ARRIVAL_DISTANCE: f32 = 0.25 / 256.;
const MOWER_RESUME_DISTANCE: f32 = 0.5 / 256.;

#[derive(Debug, Default)]
pub(super) struct MowerRuntime {
    pub(super) position: Option<Vec3>,
    pub(super) yaw: f32,
    rotation: Quat,
    visual_position: Option<Vec3>,
    dragging: bool,
    settled_target: Option<Vec2>,
    settled_rotation: Option<Quat>,
    validation_complete: bool,
}

/// Bounded planar pursuit reaches slow targets exactly, without overshoot or stop/start bursts.
fn follow_step(position: Vec3, target: Vec3, dt: f32) -> Vec3 {
    if !position.is_finite() || !target.is_finite() || !dt.is_finite() || dt <= 0.0 {
        return position;
    }
    let delta = Vec2::new(target.x - position.x, target.z - position.z);
    let step = delta.clamp_length_max(MOWER_MAX_SPEED * dt.min(MAX_FRAME_STEP));
    position + Vec3::new(step.x, 0.0, step.y)
}

fn slope_orientation(normal: Vec3, yaw: f32) -> Quat {
    let up = normal.normalize_or_zero();
    let up = if up.y >= 0.5 { up } else { Vec3::Y };
    let heading = Vec3::new(yaw.sin(), 0., yaw.cos());
    let forward = (heading - up * heading.dot(up)).normalize();
    Quat::from_mat3(&Mat3::from_cols(up.cross(forward), up, forward)).normalize()
}

fn turn_step(current: Quat, target: Quat, dt: f32) -> Quat {
    if !dt.is_finite() || dt <= 0. {
        return current;
    }
    let angle = 2. * current.dot(target).abs().clamp(0., 1.).acos();
    if angle < 1e-6 {
        return target;
    }
    current
        .slerp(
            target,
            (MOWER_MAX_TURN_SPEED * dt.min(MAX_FRAME_STEP) / angle).min(1.),
        )
        .normalize()
}

fn steered_step(position: Vec3, target: Vec3, rotation: Quat, dt: f32) -> Vec3 {
    let desired = follow_step(position, target, dt) - position;
    let forward = rotation * Vec3::Z;
    let forward = Vec3::new(forward.x, 0., forward.z).normalize_or_zero();
    let alignment = desired.normalize_or_zero().dot(forward).max(0.);
    // A full one-frame projection onto the heading turns the remaining error
    // perpendicular to the chassis. Near targets then orbit as fast as steering
    // can turn. Approach over time instead, keeping the target bearing catchable.
    let distance = Vec2::new(target.x - position.x, target.z - position.z).length();
    let approach = 1. - (-MOWER_APPROACH_RATE * dt.min(MAX_FRAME_STEP).max(0.)).exp();
    position + forward * desired.length().min(distance * approach) * alignment
}

fn mode_allows_mower(orbit: bool, cursor_visible: bool, distance: f32) -> bool {
    orbit && cursor_visible && distance.is_finite() && distance >= MIN_CAMERA_DISTANCE
}

impl MowerRuntime {
    /// Once reached, hold both motion and heading until the pointer meaningfully
    /// moves. Separate enter/exit distances reject picking/physics roundoff noise.
    fn guidance_target(&mut self, target: Vec3) -> Option<Vec3> {
        let position = self.position?;
        let target_xz = Vec2::new(target.x, target.z);
        if let Some(settled) = self.settled_target {
            if target_xz.distance(settled) <= MOWER_RESUME_DISTANCE {
                return None;
            }
            self.settled_target = None;
            self.settled_rotation = None;
        }
        if target_xz.distance(Vec2::new(position.x, position.z)) <= MOWER_ARRIVAL_DISTANCE {
            self.settled_target = Some(target_xz);
            // Keep the arrived heading; slope alignment may finish, but it must
            // not keep chasing the bearing of a tiny remaining position error.
            let forward = self.rotation * Vec3::Z;
            self.yaw = forward.x.atan2(forward.z);
            return None;
        }
        Some(target)
    }

    pub(super) fn cancel_drag(&mut self) {
        self.dragging = false;
        self.position = None;
        self.visual_position = None;
        self.rotation = Quat::IDENTITY;
        self.yaw = 0.;
        self.settled_target = None;
        self.settled_rotation = None;
    }
}

impl App {
    pub(super) fn mower_mode_available(&self) -> bool {
        mode_allows_mower(
            self.is_orbit_edit_camera_mode(),
            self.window_state.is_cursor_visible(),
            self.camera_control
                .orbit_spherical(self.tracer.camera_position())
                .2,
        )
    }

    pub(super) fn mower_controls_available(&self) -> bool {
        self.mower_mode_available() && self.terrain_edit_pointer_available()
    }

    pub(super) fn handle_mower_pointer(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> bool {
        if button == MouseButton::Left && state == ElementState::Released {
            let was_dragging = self.mower.dragging;
            self.mower.cancel_drag();
            self.tracer
                .show_mower(None, Quat::IDENTITY)
                .expect("hiding mower requires no GPU upload");
            if was_dragging {
                return true;
            }
        }
        if self.player_tools.selected_tool() != PlayerTool::Mower {
            return false;
        }
        if button != MouseButton::Left {
            return false;
        }
        if state != ElementState::Pressed {
            return true;
        }
        self.mower.cancel_drag();
        if !self.mower_controls_available() {
            return true;
        }
        let Some(hit) = self.mower_cursor_surface() else {
            return true;
        };
        let hit = match self.terrain_physics.place_mower_on_surface(hit) {
            Ok(Some(position)) => position,
            Ok(None) => return true,
            Err(error) => {
                log::error!("[MOWER] placement failed: {error:#}");
                return true;
            }
        };
        self.mower.position = Some(hit);
        self.mower.visual_position = Some(hit);
        self.mower.dragging = true;
        log::info!("[MOWER] spawned position={hit:?} max_speed={MOWER_MAX_SPEED:.4} cut_radius={MOWER_CUT_RADIUS:.4} lifetime=pointer_hold");
        true
    }

    fn mower_cursor_surface(&mut self) -> Option<Vec3> {
        let (origin, direction) = self.terrain_edit_ray()?;
        self.terrain_physics
            .pick_walkable_surface(origin, direction, 10.)
    }

    pub(super) fn update_mower(&mut self, dt: f32) -> anyhow::Result<()> {
        if std::env::var_os("RE_FLORA_MOWER_SURFACE_VALIDATE").is_some()
            && !self.mower.validation_complete
        {
            self.validate_mower_model_surfaces()?;
        }
        if std::env::var_os("RE_FLORA_MOWER_VALIDATE").is_some() && !self.mower.validation_complete
        {
            self.validate_mower()?;
        }
        if self.player_tools.selected_tool() != PlayerTool::Mower
            || !self.mower_controls_available()
            || !self.terrain_persistence.allows_world_updates()
        {
            self.mower.cancel_drag();
        }
        let target = if self.mower.dragging {
            self.mower_cursor_surface()
                .and_then(|target| self.mower.guidance_target(target))
        } else {
            None
        };
        if let (Some(previous), Some(target)) = (self.mower.position, target) {
            let desired = target - previous;
            if Vec2::new(desired.x, desired.z).length_squared() > 1e-12 {
                self.mower.yaw = desired.x.atan2(desired.z);
            }
        }
        self.update_mower_orientation(dt);
        if let (Some(previous), Some(target)) = (self.mower.position, target) {
            // Travel follows the turning chassis. Reversing the pointer first turns the
            // machine, rather than sliding it backwards with an unrelated visual heading.
            let candidate = steered_step(previous, target, self.mower.rotation, dt);
            if candidate.distance_squared(previous) > 1e-12 {
                if let Some(position) = self.terrain_physics.move_mower_on_surface(
                    previous,
                    candidate - previous,
                    dt.min(MAX_FRAME_STEP),
                )? {
                    let near_terrain = self
                        .contree_builder
                        .query_terrain_ray_cpu(position + Vec3::Y * MOWER_CUT_RADIUS, Vec3::NEG_Y)
                        .is_some_and(|hit| hit.position.distance(position) <= MOWER_CUT_RADIUS);
                    if near_terrain {
                        self.apply_flora_trim_path(TerrainBrushEdit {
                            start: previous,
                            end: position,
                            radius: MOWER_CUT_RADIUS,
                        })?;
                    }
                    self.mower.position = Some(position);
                    self.refresh_mower_contact();
                }
            }
        }
        self.tracer
            .show_mower(self.mower.visual_position, self.mower.rotation)
    }

    fn update_mower_orientation(&mut self, dt: f32) {
        let Some(feet) = self.mower.position else {
            return;
        };
        let target = if let Some(settled) = self.mower.settled_rotation {
            settled
        } else {
            let normal = self
                .terrain_physics
                .mower_support_frame(feet, self.mower.rotation)
                .map_or(self.mower.rotation * Vec3::Y, |(_, normal)| normal);
            let target = slope_orientation(normal, self.mower.yaw);
            if self.mower.settled_target.is_some() {
                // Cache the arrived support frame so wheel-sampling roundoff
                // cannot feed an endless idle orientation loop.
                self.mower.settled_rotation = Some(target);
            }
            target
        };
        self.mower.rotation = turn_step(self.mower.rotation, target, dt);
        self.refresh_mower_contact();
    }

    fn refresh_mower_contact(&mut self) {
        self.mower.visual_position = self.mower.position.map(|feet| {
            self.terrain_physics
                .mower_support_frame(feet, self.mower.rotation)
                .map_or(feet, |(position, _)| position)
        });
    }

    fn validate_mower_model_surfaces(&mut self) -> anyhow::Result<()> {
        use anyhow::{ensure, Context};
        self.mower.validation_complete = true;
        ensure!(
            self.rooftop_scene.is_some(),
            "model surface fixture requires --rooftop-poc"
        );
        self.select_item_panel_slot(super::ui_style::MOWER_SLOT_INDEX);
        let extent = self.window_state.window_extent();
        for focus in [
            Vec3::new(254., 192., 246.) / 256.,
            Vec3::new(-60., 64., 570.) / 256.,
        ] {
            self.mower.position = None;
            self.mower.cancel_drag();
            self.tracer
                .set_camera_pose_looking_at(focus + Vec3::new(0.7, 0.55, 0.7), focus);
            self.camera_control.set_orbit_focus(focus);
            self.cursor_position_physical =
                Some(Vec2::new(extent.width as f32, extent.height as f32) * 0.5);
            self.handle_mower_pointer(MouseButton::Left, ElementState::Pressed);
            let start = self
                .mower
                .position
                .context("bare fixed-model roof/road rejected mower placement")?;
            ensure!(
                (start.y - focus.y).abs() < 1. / 256.,
                "wrong fixed surface height: {start:?}"
            );
            let target = start + Vec3::new(0.07, 0., 0.02);
            let projected = self.tracer.camera_view_projection() * target.extend(1.);
            let ndc = projected.truncate() / projected.w;
            self.cursor_position_physical = Some(Vec2::new(
                (ndc.x + 1.) * extent.width as f32 * 0.5,
                (ndc.y + 1.) * extent.height as f32 * 0.5,
            ));
            for _ in 0..60 {
                self.update_mower(0.1)?;
            }
            let finish = self.mower.position.unwrap();
            ensure!(
                finish.distance(start) > 0.05,
                "mower failed to drive on model surface: {start:?} -> {finish:?}"
            );
            ensure!(
                (finish.y - focus.y).abs() < 1. / 256.,
                "mower lost model grounding"
            );
            self.handle_mower_pointer(MouseButton::Left, ElementState::Released);
            ensure!(
                self.mower.position.is_none() && self.mower.visual_position.is_none(),
                "release did not destroy mower"
            );
        }
        self.validate_mower_stationary_hold()?;
        log::info!("[MOWER][SURFACE_CHECK] bare_roof_placement=true bare_roof_driving=true road_placement=true road_driving=true outside_voxel_bounds=true grounded=true shared_player_controller=true");
        Ok(())
    }

    /// Real pointer + collision + render-pose regression for a tiny drag followed by a hold.
    fn validate_mower_stationary_hold(&mut self) -> anyhow::Result<()> {
        use anyhow::{ensure, Context};
        let extent = self.window_state.window_extent();
        self.cursor_position_physical =
            Some(Vec2::new(extent.width as f32, extent.height as f32) * 0.5);
        self.handle_mower_pointer(MouseButton::Left, ElementState::Pressed);
        let start = self
            .mower
            .position
            .context("hold fixture failed to spawn mower")?;
        let project = |matrix: glam::Mat4, target: Vec3| {
            let projected = matrix * target.extend(1.);
            let ndc = projected.truncate() / projected.w;
            Vec2::new(
                (ndc.x + 1.) * extent.width as f32 * 0.5,
                (ndc.y + 1.) * extent.height as f32 * 0.5,
            )
        };
        let target = start + Vec3::X * 0.002;
        self.cursor_position_physical = Some(project(self.tracer.camera_view_projection(), target));
        for _ in 0..180 {
            self.update_mower(1. / 60.)?;
        }
        ensure!(
            self.mower.settled_target.is_some(),
            "tiny stationary held target never settled"
        );
        let stopped_position = self.mower.position;
        let stopped_rotation = self.mower.rotation;
        let stopped_visual_position = self.mower.visual_position;
        for _ in 0..120 {
            self.update_mower(1. / 60.)?;
        }
        ensure!(
            self.mower.position == stopped_position
                && self.mower.rotation == stopped_rotation
                && self.mower.visual_position == stopped_visual_position,
            "stationary hold kept moving/turning after arrival"
        );
        self.cursor_position_physical = Some(project(
            self.tracer.camera_view_projection(),
            target + Vec3::X * 0.02,
        ));
        self.update_mower(1. / 60.)?;
        ensure!(
            self.mower.settled_target.is_none(),
            "moving the held pointer failed to resume guidance"
        );
        for _ in 0..30 {
            self.update_mower(1. / 60.)?;
        }
        ensure!(
            self.mower
                .position
                .unwrap()
                .distance(stopped_position.unwrap())
                > 0.001,
            "resumed guidance failed to move mower"
        );
        self.handle_mower_pointer(MouseButton::Left, ElementState::Released);
        ensure!(
            self.mower.position.is_none(),
            "hold fixture failed to destroy mower on release"
        );
        log::info!("[MOWER][HOLD_CHECK] short_drag=true stationary_hold_stops=true stationary_heading_stops=true pointer_move_resumes=true release_destroyed=true");
        Ok(())
    }

    /// Opt-in real-app fixture: exercises pointer placement/pursuit, GPU flora trim and raster draw.
    /// Never part of unit tests or normal gameplay. Run on the default generated terrain.
    fn validate_mower(&mut self) -> anyhow::Result<()> {
        use anyhow::{ensure, Context};
        use glam::UVec3;
        self.mower.validation_complete = true;
        ensure!(
            self.rooftop_scene.is_none(),
            "mower fixture requires the default terrain scene"
        );
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let mut patch = None;
        'search: for x in 3..18 {
            for z in 3..18 {
                let origin = Vec3::new(x as f32 * 0.1, 3., z as f32 * 0.1);
                let Some(hit) = self
                    .contree_builder
                    .query_terrain_ray_cpu(origin, Vec3::NEG_Y)
                else {
                    continue;
                };
                if hit.voxel_type != crate::builder::VOXEL_TYPE_DIRT {
                    continue;
                }
                let Some(start) = self.terrain_physics.place_mower_on_surface(hit.position)? else {
                    continue;
                };
                if self
                    .terrain_physics
                    .place_mower_on_surface(start + Vec3::X * 0.04)?
                    .is_some()
                {
                    patch = Some(start);
                    break 'search;
                }
            }
        }
        let start = patch.context("no supported mower fixture patch")?;
        self.select_item_panel_slot(super::ui_style::MOWER_SLOT_INDEX);
        self.tracer
            .set_camera_pose_looking_at(start + Vec3::new(0.5, 0.45, 0.5), start);
        self.camera_control.set_orbit_focus(start);
        let extent = self.window_state.window_extent();
        self.cursor_position_physical =
            Some(Vec2::new(extent.width as f32, extent.height as f32) * 0.5);
        ensure!(
            self.handle_mower_pointer(MouseButton::Left, ElementState::Pressed),
            "mower did not own placement"
        );
        let placed = self
            .mower
            .position
            .context("pointer placement produced no mower")?;
        ensure!(self.mower.dragging, "placement did not start dragging");
        let region_min = (start * 256.)
            .floor()
            .as_uvec3()
            .saturating_sub(UVec3::splat(24));
        let region_size = UVec3::splat(48);
        let terrain_before = self
            .plain_builder
            .read_chunk_atlas_region(region_min, region_size)?;
        let now_ms = self.time_info.time_since_start_duration().as_millis() as u32;
        let grass_selection = crate::flora::species::PLAYER_FLORA_PAINT_SELECTIONS
            .iter()
            .position(|s| *s == crate::flora::species::FloraPaintSelection::GrassMix)
            .context("missing grass selection")?;
        self.select_flora_paint_selection_index(grass_selection);
        self.apply_surface_flora_regeneration(
            TerrainBrushEdit {
                start: placed,
                end: placed + Vec3::X * 0.04,
                radius: 0.08,
            },
            1,
            true,
        )?;
        let plant_species = crate::flora::species::authored_plant_species_indices()
            .next()
            .context("missing authored plant species")?;
        let column = glam::UVec2::new(
            (placed.x * 256.).floor() as u32,
            (placed.z * 256.).floor() as u32,
        );
        let anchor = self
            .resolve_plantable_surface_column_below(column, placed.y + 0.1)
            .map_err(|e| anyhow::anyhow!("fixture plant anchor: {e:?}"))?;
        let mut batch = super::planting::AuthoredFloraPlacementBatch::new();
        ensure!(
            self.try_place_authored_flora(&mut batch, plant_species, anchor, 255, u32::MAX, 42),
            "fixture plant insertion failed"
        );
        self.finish_authored_flora_placement(batch)?;
        let before = self.surface_builder.capture_flora_snapshot(now_ms)?;
        log::info!(
            "[MOWER][FIXTURE] mature grass patch counts={:?}",
            before.counts()
        );
        let target = placed + Vec3::X * 0.04;
        let projected = self.tracer.camera_view_projection() * target.extend(1.);
        let ndc = projected.truncate() / projected.w;
        self.cursor_position_physical = Some(Vec2::new(
            (ndc.x + 1.) * extent.width as f32 * 0.5,
            (ndc.y + 1.) * extent.height as f32 * 0.5,
        ));
        for _ in 0..4 {
            let previous = self.mower.position.unwrap();
            let previous_rotation = self.mower.rotation;
            self.update_mower(0.1)?;
            ensure!(
                previous_rotation.angle_between(self.mower.rotation)
                    <= MOWER_MAX_TURN_SPEED * 0.1 + 1e-4,
                "mower exceeded turn speed"
            );
            let position = self.mower.position.unwrap();
            let distance = Vec2::new(position.x - previous.x, position.z - previous.z).length();
            ensure!(
                distance <= MOWER_MAX_SPEED * 0.1 + 1e-6,
                "mower exceeded top speed"
            );
        }
        let finish = self.mower.position.unwrap();
        ensure!(
            finish.distance(placed) > 0.01,
            "mower failed to follow the pointer"
        );
        let after = self.surface_builder.capture_flora_snapshot(now_ms)?;
        ensure!(before.counts() == after.counts(), "mower removed plants");
        ensure!(
            !before.same_layout_and_growth(&after),
            "mower did not reduce plant growth"
        );
        ensure!(
            terrain_before
                == self
                    .plain_builder
                    .read_chunk_atlas_region(region_min, region_size)?,
            "mower changed terrain"
        );
        self.handle_mower_pointer(MouseButton::Left, ElementState::Released);
        self.update_mower(0.1)?;
        ensure!(
            self.mower.position.is_none() && self.mower.visual_position.is_none(),
            "mower survived release"
        );
        self.tracer
            .set_camera_pose_looking_at(finish + Vec3::Y * 0.2, finish);
        self.camera_control.set_orbit_focus(finish);
        ensure!(
            !self.mower_mode_available(),
            "close camera must block mower controls"
        );
        self.handle_mower_pointer(MouseButton::Left, ElementState::Pressed);
        ensure!(!self.mower.dragging, "close camera started mower dragging");
        // A diagnostic close crop at an allowed distance makes the small raster model inspectable.
        let focus = finish + Vec3::Y * 0.035;
        self.tracer
            .set_camera_pose_looking_at(focus + Vec3::new(0.5, 0.45, 0.5), focus);
        self.camera_control.set_orbit_focus(focus);
        let mut pose = self.tracer.camera_pose();
        pose.fov_deg = 20.;
        self.tracer.apply_camera_pose(pose);
        // Hold a fresh mower for the diagnostic screenshot, using the real pointer lifecycle.
        let projected = self.tracer.camera_view_projection() * finish.extend(1.);
        let ndc = projected.truncate() / projected.w;
        self.cursor_position_physical = Some(Vec2::new(
            (ndc.x + 1.) * extent.width as f32 * 0.5,
            (ndc.y + 1.) * extent.height as f32 * 0.5,
        ));
        self.handle_mower_pointer(MouseButton::Left, ElementState::Pressed);
        ensure!(
            self.mower.position.is_some(),
            "fresh diagnostic pointer press failed to spawn mower"
        );
        log::info!("[MOWER][CHECK] pointer_placement=true speed_limited=true turn_speed_limited=true grounded=true trim_growth=true plants_preserved=true terrain_unchanged=true release_destroyed=true fresh_press_respawn=true close_camera_blocked=true raster_postprocess=true position={finish:?}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_drag_then_stationary_hold_settles_without_endless_turning() {
        let mut position = Vec3::ZERO;
        let mut rotation = Quat::IDENTITY;
        let mut yaw = 0.;
        let target = Vec3::new(0.002, 0., 0.);
        let mut tail_turn = 0.;
        for frame in 0..1200 {
            let desired = target - position;
            if Vec2::new(desired.x, desired.z).length_squared() > 1e-12 {
                yaw = desired.x.atan2(desired.z);
            }
            let next_rotation = turn_step(rotation, slope_orientation(Vec3::Y, yaw), 1. / 60.);
            if frame >= 1080 {
                tail_turn += rotation.angle_between(next_rotation);
            }
            rotation = next_rotation;
            position = steered_step(position, target, rotation, 1. / 60.);
        }
        assert!(
            position.distance(target) < 0.0001,
            "failed to reach short-drag destination: {position:?}"
        );
        assert!(
            tail_turn < 0.001,
            "still turning with a stationary held pointer: {tail_turn}"
        );
    }

    #[test]
    fn stationary_hold_stops_motion_and_heading_and_resumes_after_pointer_moves() {
        for distance in [0.002, 0.01, 0.04] {
            let target = Vec3::X * distance;
            let mut mower = MowerRuntime {
                position: Some(Vec3::ZERO),
                dragging: true,
                ..Default::default()
            };
            let mut tail_motion = 0.;
            let mut tail_turn = 0.;
            let mut settled_frame = None;
            for frame in 0..1200 {
                let previous_position = mower.position.unwrap();
                let previous_rotation = mower.rotation;
                if let Some(active_target) = mower.guidance_target(target) {
                    let delta = active_target - previous_position;
                    mower.yaw = delta.x.atan2(delta.z);
                    mower.rotation = turn_step(
                        mower.rotation,
                        slope_orientation(Vec3::Y, mower.yaw),
                        1. / 60.,
                    );
                    mower.position = Some(steered_step(
                        previous_position,
                        active_target,
                        mower.rotation,
                        1. / 60.,
                    ));
                }
                if mower.settled_target.is_some() && settled_frame.is_none() {
                    settled_frame = Some(frame);
                }
                if frame >= 1080 {
                    tail_motion += mower.position.unwrap().distance(previous_position);
                    tail_turn += mower.rotation.angle_between(previous_rotation);
                }
            }
            assert!(mower.position.unwrap().distance(target) <= MOWER_ARRIVAL_DISTANCE);
            assert!(mower.settled_target.is_some());
            assert!(
                settled_frame.unwrap() <= 180,
                "short target should settle within three seconds, not merely stop eventually"
            );
            assert_eq!(tail_motion, 0.);
            assert_eq!(tail_turn, 0.);
            assert!(mower
                .guidance_target(target + Vec3::X * (MOWER_RESUME_DISTANCE * 0.9))
                .is_none());
            assert!(mower
                .guidance_target(target + Vec3::X * (MOWER_RESUME_DISTANCE * 1.1))
                .is_some());
            assert!(mower.settled_target.is_none());
            mower.cancel_drag();
            assert!(mower.settled_target.is_none());
        }
    }

    #[test]
    fn model_up_and_forward_follow_pitch_and_roll() {
        let normal = Vec3::new(-0.3, 1., 0.2).normalize();
        let rotation = slope_orientation(normal, 1.2);
        assert!((rotation * Vec3::Y).distance(normal) < 1e-6);
        assert!((rotation * Vec3::Z).dot(normal).abs() < 1e-6);
        assert!(rotation.is_normalized());
    }

    #[test]
    fn sudden_reversal_has_bounded_rotation_and_no_backward_slide() {
        let target = slope_orientation(Vec3::Y, std::f32::consts::PI);
        let next = turn_step(Quat::IDENTITY, target, 1. / 60.);
        assert!(Quat::IDENTITY.angle_between(next) <= MOWER_MAX_TURN_SPEED / 60. + 1e-4);
        assert!(next.angle_between(target) > 2.);
        assert_eq!(
            steered_step(Vec3::ZERO, Vec3::NEG_Z, next, 1. / 60.),
            Vec3::ZERO
        );
        let forward = steered_step(Vec3::ZERO, Vec3::Z * 0.001, Quat::IDENTITY, 1. / 60.);
        assert!(forward.z > 0. && forward.z < 0.001);
        assert!(forward.x == 0. && forward.y == 0.);
    }

    #[test]
    fn turning_uses_shortest_arc_and_is_frame_rate_independent() {
        let start = Quat::from_rotation_y(170_f32.to_radians());
        let target = Quat::from_rotation_y(-170_f32.to_radians());
        assert!(turn_step(start, target, 0.1).angle_between(target) < start.angle_between(target));
        let simulate = |dt, count| {
            let mut q = Quat::IDENTITY;
            for _ in 0..count {
                q = turn_step(q, target, dt);
            }
            q
        };
        assert!(simulate(1. / 30., 15).angle_between(simulate(1. / 120., 60)) < 0.001);
        assert_eq!(turn_step(start, target, f32::NAN), start);
    }

    #[test]
    fn cancellation_destroys_the_temporary_machine() {
        let mut mower = MowerRuntime {
            position: Some(Vec3::ONE),
            visual_position: Some(Vec3::ONE),
            dragging: true,
            validation_complete: true,
            ..Default::default()
        };
        mower.cancel_drag();
        assert!(mower.position.is_none() && mower.visual_position.is_none() && !mower.dragging);
        assert_eq!(mower.rotation, Quat::IDENTITY);
        assert!(mower.validation_complete);
    }

    #[test]
    fn only_zoomed_out_visible_cursor_orbit_can_operate_mower() {
        assert!(mode_allows_mower(true, true, MIN_CAMERA_DISTANCE));
        assert!(!mode_allows_mower(true, true, MIN_CAMERA_DISTANCE - 0.001));
        assert!(!mode_allows_mower(false, true, 2.));
        assert!(!mode_allows_mower(true, false, 2.));
        assert!(!mode_allows_mower(true, true, f32::NAN));
    }

    #[test]
    fn fast_pointer_is_only_a_direction_and_never_teleports() {
        let start = Vec3::new(1., 0.5, 1.);
        let step = follow_step(start, Vec3::new(9., 3., 9.), 0.02);
        assert!((step.distance(start) - MOWER_MAX_SPEED * 0.02).abs() < 1e-6);
        assert_eq!(step.y, start.y);
    }
    #[test]
    fn slow_pointer_is_followed_exactly_without_overshoot() {
        let start = Vec3::ZERO;
        let target = Vec3::new(0.0005, 0., 0.0005);
        assert_eq!(follow_step(start, target, 0.02), target);
        assert_eq!(follow_step(target, target, 0.02), target);
    }
    #[test]
    fn stalled_frames_and_invalid_input_cannot_jump() {
        assert!(
            follow_step(Vec3::ZERO, Vec3::X, 100.).length() <= MOWER_MAX_SPEED * MAX_FRAME_STEP
        );
        for dt in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(follow_step(Vec3::ZERO, Vec3::X, dt), Vec3::ZERO);
        }
        assert_eq!(
            follow_step(Vec3::ZERO, Vec3::splat(f32::NAN), 0.1),
            Vec3::ZERO
        );
    }
    #[test]
    fn speed_is_independent_of_frame_rate() {
        let simulate = |dt: f32, count: usize| {
            let mut p = Vec3::ZERO;
            for _ in 0..count {
                p = follow_step(p, Vec3::X, dt);
            }
            p
        };
        assert!(simulate(1. / 30., 30).distance(simulate(1. / 120., 120)) < 1e-6);
    }
}
