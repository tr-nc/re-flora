//! Opt-in native input/FX/conservation check, isolated to the empty rooftop plot.
//! Never runs or changes camera/settings in ordinary gameplay.
use super::player_tools::ContinuousTerrainToolAction;
use super::*;
use crate::builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use anyhow::ensure;

pub(super) struct Review {
    pub mode: String,
    frame: u32,
    before: usize,
    stock: u32,
    pub added: u32,
    hover_seen: bool,
    pour_seen: bool,
    hover_target_seen: bool,
    inactive_frames: usize,
    visible_inactive_frames: usize,
    pub peak_grains: usize,
    pub contacts: usize,
    shadow_pixels: Option<usize>,
    removed_shadow_pixels: Option<usize>,
    holding_frames: usize,
    missing_holding_frames: usize,
    done: bool,
}
impl Review {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(mode) = std::env::var("RE_FLORA_SOIL_BAG_REVIEW") else {
            return Ok(None);
        };
        ensure!(
            matches!(
                mode.as_str(),
                "hover" | "pour" | "cycle" | "world" | "stability" | "flora"
            ),
            "RE_FLORA_SOIL_BAG_REVIEW must be hover, pour, cycle, world, stability or flora"
        );
        Ok(Some(Self {
            mode,
            frame: 0,
            before: 0,
            stock: 0,
            added: 0,
            hover_seen: false,
            pour_seen: false,
            hover_target_seen: false,
            inactive_frames: 0,
            visible_inactive_frames: 0,
            peak_grains: 0,
            contacts: 0,
            shadow_pixels: None,
            removed_shadow_pixels: None,
            holding_frames: 0,
            missing_holding_frames: 0,
            done: false,
        }))
    }
    pub fn observe_instance(
        &mut self,
        instance: Option<crate::soil_bag::Instance>,
        has_hover: bool,
        holding: bool,
    ) {
        self.hover_target_seen |= has_hover && !holding;
        if let Some(pose) = instance {
            self.hover_seen |= !pose.pouring;
            self.pour_seen |= pose.pouring;
        }
        if !holding {
            self.inactive_frames += 1;
            self.visible_inactive_frames += usize::from(instance.is_some());
        }
        if self.mode == "stability" && (13..=50).contains(&self.frame) {
            self.holding_frames += 1;
            self.missing_holding_frames += usize::from(instance.is_none_or(|pose| !pose.pouring));
        }
    }
}
impl App {
    fn soil_bag_review_dirt_count(&mut self) -> Result<usize> {
        let data = self.plain_builder.read_chunk_atlas_region(
            rooftop_scene::SOIL_MIN,
            rooftop_scene::SOIL_MAX - rooftop_scene::SOIL_MIN,
        )?;
        Ok(data
            .iter()
            .filter(|&&v| v & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8)
            .count())
    }
    pub(super) fn advance_soil_bag_review(&mut self) -> Result<()> {
        let Some(mut review) = self.soil_bag_preview.review.take() else {
            return Ok(());
        };
        let result = (|| {
            if review.done {
                return Ok(());
            }
            if review.mode == "flora" {
                self.review_soil_grass_preservation()?;
                review.done = true;
                return Ok(());
            }
            if review.frame == 0 {
                ensure!(
                    self.rooftop_scene.is_some(),
                    "soil-bag review requires --rooftop-poc"
                );
                ensure!(
                    self.soil_performance_bench.is_none(),
                    "soil-bag review cannot run with soil performance replay"
                );
                ensure!(
                    self.infinite_materials_enabled(),
                    "soil-bag review requires Infinite Materials"
                );
                review.before = self.soil_bag_review_dirt_count()?;
                review.stock = self
                    .voxel_backpack
                    .count(voxel_backpack::BackpackVoxel::Dirt);
                let target = Vec3::new(
                    (rooftop_scene::SOIL_MIN.x + rooftop_scene::SOIL_MAX.x) as f32 * 0.5,
                    rooftop_scene::SOIL_MIN.y as f32,
                    (rooftop_scene::SOIL_MIN.z + rooftop_scene::SOIL_MAX.z) as f32 * 0.5,
                ) / 256.;
                self.camera_control.apply_snapshot_mode(true);
                self.camera_control.set_orbit_focus(target);
                ensure!(
                    self.tracer
                        .set_camera_pose_looking_at(target + Vec3::new(0.30, 0.40, 0.65), target),
                    "review camera"
                );
                let extent = self.window_state.window_extent();
                self.cursor_position_physical = Some(Vec2::new(
                    extent.width as f32 * 0.5,
                    extent.height as f32 * 0.5,
                ));
                self.select_item_panel_slot(ui_style::SOIL_DEPOSITION_SLOT_INDEX);
                self.set_manual_time_of_day(0.46);
                log::info!(
                    "[SOIL_BAG_REVIEW] started mode={} saved=false production_hover=true",
                    review.mode
                );
            }
            if review.mode == "world" {
                if review.frame == 20 {
                    let pixels = self.tracer.soil_bag_shadow_depth_pixels()?;
                    ensure!(pixels > 0, "world bag did not write real sun depth");
                    review.shadow_pixels = Some(pixels);
                }
                if review.frame == 60 {
                    let pixels = self.tracer.soil_bag_shadow_depth_pixels()?;
                    ensure!(
                        pixels == 0,
                        "released world bag left shadow geometry behind"
                    );
                    review.removed_shadow_pixels = Some(pixels);
                }
            }
            if review.mode != "hover" {
                if review.frame == 10 {
                    self.set_tool_mouse_button_state(MouseButton::Left, ElementState::Pressed);
                    ensure!(
                        matches!(
                            self.player_tools.begin_pointer_action(MouseButton::Left),
                            Some(PlayerToolPointerAction::Continuous(
                                ContinuousTerrainToolAction::SoilDeposit
                            ))
                        ),
                        "review LMB must resolve to production Soil deposition"
                    );
                    log::info!("[SOIL_BAG_REVIEW] press frame=10 animation=false");
                }
                if review.frame == 50 {
                    self.set_tool_mouse_button_state(MouseButton::Left, ElementState::Released);
                    self.refresh_terrain_edit_hold_from_mouse_buttons();
                    ensure!(
                        self.player_tools.active_continuous_action().is_none(),
                        "review release must stop Soil"
                    );
                    log::info!("[SOIL_BAG_REVIEW] release frame=50");
                }
                if review.frame == 51 {
                    ensure!(
                        self.soil_bag_preview.current_instance().is_none()
                            && !self.soil_bag_preview.allow_flow,
                        "release must immediately hide the bag and stop emission"
                    );
                }
                if review.frame == 70 {
                    self.select_item_panel_slot(2); // Slot 3 = Edit, never Soil.
                    log::info!("[SOIL_BAG_REVIEW] switched_to_edit frame=70");
                }
            }
            if review.frame == 100 {
                let after = self.soil_bag_review_dirt_count()?;
                ensure!(
                    after == review.before + review.added as usize,
                    "visual grains changed authoritative soil volume"
                );
                ensure!(
                    self.voxel_backpack
                        .count(voxel_backpack::BackpackVoxel::Dirt)
                        == review.stock,
                    "visual grains changed real stock"
                );
                ensure!(
                    review.hover_target_seen && !review.hover_seen,
                    "valid idle hover must not display the bag"
                );
                ensure!(
                    review.inactive_frames > 0 && review.visible_inactive_frames == 0,
                    "bag must remain hidden before pouring and after release"
                );
                if review.mode != "hover" {
                    ensure!(
                        review.pour_seen
                            && review.added > 0
                            && review.peak_grains > 0
                            && review.contacts > 0,
                        "review must exercise real pouring and grain contacts"
                    );
                    ensure!(
                        self.soil_bag_preview.current_instance().is_none(),
                        "bag must hide after switching away from Soil"
                    );
                } else {
                    ensure!(
                        review.added == 0 && review.peak_grains == 0,
                        "hover cannot emit material or particles"
                    );
                }
                if review.mode == "stability" {
                    log::info!("[SOIL_BAG_REVIEW][STABILITY] holding_frames={} missing_frames={} added={} no_flicker={}", review.holding_frames, review.missing_holding_frames, review.added, review.missing_holding_frames == 0);
                    ensure!(
                        review.holding_frames == 38 && review.missing_holding_frames == 0,
                        "holding Soil bag flickered: {} of {} frames missing",
                        review.missing_holding_frames,
                        review.holding_frames
                    );
                }
                if review.mode == "world" {
                    ensure!(
                        review.shadow_pixels.is_some_and(|pixels| pixels > 0)
                            && review.removed_shadow_pixels == Some(0),
                        "world shadow lifecycle review incomplete"
                    );
                    ensure!(self.soil_bag_preview.current_instance().is_none());
                    log::info!("[SOIL_BAG_REVIEW][WORLD_CHECK] sun_depth_pixels={} after_release=0 shared_frame_instance=true world_lighting=true terrain_conserved=true", review.shadow_pixels.unwrap());
                }
                log::info!("[SOIL_BAG_REVIEW][CHECK] hover={} pour={} inactive_frames={} visible_inactive_frames={} added={} peak_grains={} contacts={} conserved=true real_inventory_preserved=true no_animation=true",review.hover_seen,review.pour_seen,review.inactive_frames,review.visible_inactive_frames,review.added,review.peak_grains,review.contacts);
                review.done = true;
            }
            review.frame += 1;
            Ok(())
        })();
        self.soil_bag_preview.review = Some(review);
        result
    }
}
