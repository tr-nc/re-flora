//! Cursor-guided mower: the pointer supplies a destination, never a teleport or a speed.
use glam::{Vec2, Vec3};
use winit::event::{ElementState, MouseButton};

use super::{player_tools::PlayerTool, App};
use crate::app::world_edits::TerrainBrushEdit;

pub(super) const MOWER_MAX_SPEED: f32 = 30.0 / 256.0;
pub(super) const MOWER_CUT_RADIUS: f32 = 11.0 / 256.0;
const MIN_CAMERA_DISTANCE: f32 = 0.7;
const MAX_FRAME_STEP: f32 = 0.1;

#[derive(Debug, Default)]
pub(super) struct MowerRuntime {
    pub(super) position: Option<Vec3>,
    pub(super) yaw: f32,
    dragging: bool,
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

fn mode_allows_mower(orbit: bool, cursor_visible: bool, distance: f32) -> bool {
    orbit && cursor_visible && distance.is_finite() && distance >= MIN_CAMERA_DISTANCE
}

impl MowerRuntime {
    pub(super) fn cancel_drag(&mut self) {
        self.dragging = false;
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
            self.mower.dragging = false;
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
        if !self.mower_controls_available() {
            return true;
        }
        let Some(hit) = self.mower_cursor_surface() else {
            return true;
        };
        match self.mower.position {
            None => {
                let hit = match self.terrain_physics.place_mower_on_surface(hit) {
                    Ok(Some(position)) => position,
                    Ok(None) => return true,
                    Err(error) => {
                        log::error!("[MOWER] placement failed: {error:#}");
                        return true;
                    }
                };
                self.mower.position = Some(hit);
                self.mower.dragging = true;
                log::info!("[MOWER] placed position={hit:?} max_speed={MOWER_MAX_SPEED:.4} cut_radius={MOWER_CUT_RADIUS:.4} persistence=session");
            }
            Some(position) => {
                // Grab the machine, not an arbitrary point elsewhere in the world.
                self.mower.dragging =
                    Vec2::new(hit.x - position.x, hit.z - position.z).length() <= 20.0 / 256.0;
            }
        }
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
            self.mower.dragging = false;
        }
        if self.mower.dragging {
            if let (Some(previous), Some(target)) =
                (self.mower.position, self.mower_cursor_surface())
            {
                let candidate = follow_step(previous, target, dt);
                if candidate.distance_squared(previous) > 1e-12 {
                    if let Some(position) = self.terrain_physics.move_mower_on_surface(
                        previous,
                        candidate - previous,
                        dt.min(MAX_FRAME_STEP),
                    )? {
                        let direction = position - previous;
                        if Vec2::new(direction.x, direction.z).length_squared() > 1e-12 {
                            self.mower.yaw = direction.x.atan2(direction.z);
                            // Model floors are traversable, but they are not flora substrates.
                            let near_terrain = self
                                .contree_builder
                                .query_terrain_ray_cpu(
                                    position + Vec3::Y * MOWER_CUT_RADIUS,
                                    Vec3::NEG_Y,
                                )
                                .is_some_and(|hit| {
                                    hit.position.distance(position) <= MOWER_CUT_RADIUS
                                });
                            if near_terrain {
                                self.apply_flora_trim_path(TerrainBrushEdit {
                                    start: previous,
                                    end: position,
                                    radius: MOWER_CUT_RADIUS,
                                })?;
                            }
                        }
                        self.mower.position = Some(position);
                    }
                }
            }
        }
        self.tracer.show_mower(self.mower.position, self.mower.yaw)
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
            for _ in 0..12 {
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
        }
        log::info!("[MOWER][SURFACE_CHECK] bare_roof_placement=true bare_roof_driving=true road_placement=true road_driving=true outside_voxel_bounds=true grounded=true shared_player_controller=true");
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
            self.update_mower(0.1)?;
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
            self.mower.position == Some(finish),
            "mower moved after release"
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
        self.tracer
            .show_mower(self.mower.position, self.mower.yaw)?;
        log::info!("[MOWER][CHECK] pointer_placement=true speed_limited=true grounded=true trim_growth=true plants_preserved=true terrain_unchanged=true release_stops=true close_camera_blocked=true raster_postprocess=true position={finish:?}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
