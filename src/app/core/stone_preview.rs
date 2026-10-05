//! Saved declarative controls drive an isolated native renderer request. The
//! opt-in hidden fixture edits those same live fields, never config/world files.
use super::App;
use crate::{
    app::gui_config::GuiAdjustables,
    stone_models::{RockParams, SlabParams, StoneKind, StoneSpec},
    tracer::StonePreviewRequest,
};
use anyhow::{ensure, Result};
use glam::Vec3;

pub(super) struct State {
    base: Option<Vec3>,
    was_enabled: bool,
    review: Option<String>,
    style_review: Option<String>,
    frame: u32,
    phase: Option<u32>,
    gui_save: Option<GuiSaveReview>,
}
struct GuiSaveReview {
    phase: u8,
    button: Option<egui::Pos2>,
}
impl State {
    pub fn new() -> Result<Self> {
        let review = std::env::var("RE_FLORA_STONE_REVIEW").ok();
        if let Some(mode) = review.as_deref() {
            ensure!(["voxel-rock","direct-rock","voxel-slab","direct-slab","cycle"].contains(&mode),"RE_FLORA_STONE_REVIEW must be voxel-rock, direct-rock, voxel-slab, direct-slab, or cycle");
        }
        let style_review = std::env::var("RE_FLORA_STONE_STYLE_REVIEW").ok();
        if let Some(style) = style_review.as_deref() {
            ensure!(
                ["a", "128", "256", "global", "combined", "cycle"].contains(&style),
                "invalid RE_FLORA_STONE_STYLE_REVIEW"
            );
            ensure!(
                review.is_some(),
                "stone style fixture requires a stone review"
            );
        }
        let gui_save = std::env::var_os("RE_FLORA_STONE_GUI_SAVE_REVIEW").is_some();
        ensure!(!gui_save || review.is_some(), "stone GUI Save replay requires RE_FLORA_STONE_REVIEW and a backed-up worktree GUI config");
        Ok(Self {
            base: None,
            was_enabled: false,
            review,
            style_review,
            frame: 0,
            phase: None,
            gui_save: gui_save.then_some(GuiSaveReview {
                phase: 0,
                button: None,
            }),
        })
    }
    pub fn remember_save_button(&mut self, center: egui::Pos2) {
        if let Some(review) = &mut self.gui_save {
            review.button = Some(center);
        }
    }
    /// Replay normalized native GUI input only: no per-field save hook and no
    /// visible window/OS pointer needed. The real Save response owns persistence.
    pub fn gui_save_events(
        &mut self,
        pixels_per_point: f32,
    ) -> Option<[winit::event::WindowEvent; 3]> {
        use winit::{
            dpi::PhysicalPosition,
            event::{DeviceId, ElementState, MouseButton, WindowEvent},
        };
        let review = self.gui_save.as_mut()?;
        let center = review.button?;
        if self.frame < 3 || review.phase >= 2 {
            return None;
        }
        let pressed = review.phase == 0;
        review.phase += 1;
        log::info!("[STONE_GUI_REVIEW] actual_button=Save input={} persistence=normal_gui_handler search=Stone_Rendering", if pressed { "press" } else { "release" });
        Some([
            WindowEvent::Focused(true),
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(
                    f64::from(center.x * pixels_per_point),
                    f64::from(center.y * pixels_per_point),
                ),
            },
            WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state: if pressed {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                },
                button: MouseButton::Left,
            },
        ])
    }
}
fn spec(settings: &GuiAdjustables) -> StoneSpec {
    let kind = if settings.stone_kind.value == 0 {
        StoneKind::Slab
    } else {
        StoneKind::Rock
    };
    StoneSpec {
        kind,
        seed: settings.stone_seed.value,
        size: Vec3::new(
            settings.stone_width.value,
            if kind == StoneKind::Slab {
                settings.stone_slab_thickness.value
            } else {
                settings.stone_rock_height.value
            },
            settings.stone_depth.value,
        ),
        variation: settings.stone_variation.value,
        slab: SlabParams {
            edge_cut: settings.stone_slab_edge_cut.value,
        },
        rock: RockParams {
            facets: settings.stone_rock_facets.value,
        },
    }
    .sanitized()
}
impl App {
    pub(super) fn prepare_stone_preview(&mut self) -> Result<()> {
        if let Some(mode) = self.stone_preview.review.clone() {
            let phase = if mode == "cycle" {
                (self.stone_preview.frame / 21).min(9)
            } else {
                0
            };
            self.stone_preview.frame += 1;
            if self.stone_preview.frame == 1 && self.stone_preview.style_review.is_some() {
                if let Some(size) = self
                    .window_state
                    .window()
                    .request_inner_size(winit::dpi::PhysicalSize::new(1600, 900))
                {
                    self.queue_frame_extent(re_flora_vkn::Extent2D::new(size.width, size.height));
                }
            }
            let settings = &mut self.debug_settings.adjustables;
            settings.stone_preview_enabled.value = phase != 6;
            settings.stone_direct_triangles.value = if mode == "cycle" {
                [1, 2, 5, 8, 9].contains(&phase)
            } else {
                mode.starts_with("direct")
            };
            settings.stone_kind.value = if mode == "cycle" {
                u32::from(phase < 4)
            } else {
                u32::from(mode.ends_with("rock"))
            };
            settings.stone_seed.value = if mode == "cycle" && phase >= 2 { 7 } else { 42 };
            settings.stone_width.value = if phase == 2 || phase == 3 { 0.42 } else { 0.24 };
            settings.stone_rock_height.value = if phase == 2 || phase == 3 { 0.38 } else { 0.20 };
            settings.stone_yaw.value = if phase >= 2 { 55. } else { 0. };
            settings.stone_preview_lift.value = 0.15;
            settings.auto_daynight_cycle.value = false;
            if mode == "cycle" && phase >= 7 {
                // Exercise real extent/descriptor publication with submitted
                // stone frames. The final grid ratio stays original 64:1.
                settings.scene_supersampling_enabled.value = phase == 7 || phase == 8;
                settings.scene_supersampling_quality.value = u32::from(phase == 8);
                settings.scene_pixel_ratio.value = if phase == 7 { 2 } else { 3 };
            }
            if let Some(style) = self.stone_preview.style_review.as_deref() {
                settings.model_view_quantization_enabled.value = match style {
                    "128" | "256" | "combined" => true,
                    "cycle" => phase % 3 != 0,
                    _ => false,
                };
                settings.model_pixel_view_count.value =
                    if style == "256" || (style == "cycle" && phase % 3 == 2) {
                        256
                    } else {
                        128
                    };
                settings.ordered_dither_global.value =
                    matches!(style, "global" | "combined") || (style == "cycle" && phase >= 7);
                settings.ordered_dither_pattern.value = u32::from(phase == 9);
                settings.ordered_dither_levels.value = 8;
                settings.ordered_dither_strength.value = 1.;
                if self.stone_preview.phase != Some(phase) {
                    log::info!(
                        "[STONE_STYLE_REVIEW] style={style} quantized={} count={} global={} bank_binding=19",
                        settings.model_view_quantization_enabled.value,
                        settings.model_pixel_view_count.value,
                        settings.ordered_dither_global.value
                    );
                }
            }
            if self.stone_preview.phase != Some(phase) {
                self.stone_preview.phase = Some(phase);
                log::info!("[STONE_REVIEW] mode={mode} phase={phase} saved=false terrain_edits=0");
            }
        }
        let enabled = self.debug_settings.adjustables.stone_preview_enabled.value;
        if !enabled {
            self.stone_preview.was_enabled = false;
            return self.tracer.set_stone_preview(None);
        }
        if self.stone_preview.base.is_none() {
            // Inspection site only: one read-only downward terrain query. Do
            // not regenerate flora, author terrain, or claim a gameplay collider.
            let origin = Vec3::new(1.10, self.world_chunk_dim.y as f32 + 1., 1.40);
            let Some(hit) = self.query_terrain_ray_cpu(origin, Vec3::NEG_Y) else {
                return Ok(());
            };
            self.stone_preview.base =
                Some(Vec3::new(origin.x, hit.position.y + 1. / 256., origin.z));
        }
        let settings = &self.debug_settings.adjustables;
        let model = spec(settings);
        let lift = if settings.stone_preview_lift.value.is_finite() {
            settings.stone_preview_lift.value.clamp(0., 0.6)
        } else {
            0.15
        };
        let base = self.stone_preview.base.unwrap() + Vec3::Y * lift;
        let yaw = if settings.stone_yaw.value.is_finite() {
            settings.stone_yaw.value.rem_euclid(360.)
        } else {
            0.
        };
        let focus = settings.stone_preview_focus.value;
        let direct = settings.stone_direct_triangles.value;
        if !self.stone_preview.was_enabled && focus {
            let target = base + Vec3::Y * model.size.y * 0.45;
            self.camera_control.apply_snapshot_mode(true);
            self.camera_control.set_orbit_focus(target);
            ensure!(
                self.tracer
                    .set_camera_pose_looking_at(target + Vec3::new(0.34, 0.28, 0.53), target),
                "stone preview camera rejected finite pose"
            );
            if self.stone_preview.review.is_some() {
                self.set_manual_time_of_day(0.46);
            }
        }
        self.stone_preview.was_enabled = true;
        self.tracer.set_stone_preview(Some(StonePreviewRequest {
            spec: model,
            direct,
            base,
            yaw,
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config_loader::GuiConfigLoader;
    #[test]
    fn native_gui_save_replay_waits_for_real_layout_then_presses_and_releases_once() {
        use winit::event::{ElementState, WindowEvent};
        let mut state = State {
            base: None,
            was_enabled: false,
            review: Some("direct-rock".to_owned()),
            style_review: None,
            frame: 2,
            phase: None,
            gui_save: Some(GuiSaveReview {
                phase: 0,
                button: None,
            }),
        };
        assert!(state.gui_save_events(1.6).is_none());
        state.remember_save_button(egui::pos2(100., 50.));
        assert!(state.gui_save_events(1.6).is_none());
        state.frame = 3;
        let [_, position, press] = state.gui_save_events(1.6).unwrap();
        let WindowEvent::CursorMoved { position, .. } = position else {
            panic!("native pointer event");
        };
        assert_eq!(position.x, 160.);
        assert_eq!(position.y, 80.);
        assert!(matches!(
            press,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            }
        ));
        assert!(matches!(
            state.gui_save_events(1.6).unwrap()[2],
            WindowEvent::MouseInput {
                state: ElementState::Released,
                ..
            }
        ));
        assert!(state.gui_save_events(1.6).is_none());
    }

    #[test]
    fn saved_control_adapter_uses_type_specific_dimensions_and_one_source_spec() {
        let mut s = GuiAdjustables::from_config(&GuiConfigLoader::load());
        for kind in [0, 1] {
            s.stone_kind.value = kind;
            s.stone_seed.value = 19;
            let a = spec(&s);
            s.stone_direct_triangles.value = !s.stone_direct_triangles.value;
            assert_eq!(a, spec(&s));
            assert_eq!(
                a.size.y,
                if kind == 0 {
                    s.stone_slab_thickness.value
                } else {
                    s.stone_rock_height.value
                }
            );
        }
    }
}
