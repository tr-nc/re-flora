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
    frame: u32,
    phase: Option<u32>,
}
impl State {
    pub fn new() -> Result<Self> {
        let review = std::env::var("RE_FLORA_STONE_REVIEW").ok();
        if let Some(mode) = review.as_deref() {
            ensure!(["voxel-rock","direct-rock","voxel-slab","direct-slab","cycle"].contains(&mode),"RE_FLORA_STONE_REVIEW must be voxel-rock, direct-rock, voxel-slab, direct-slab, or cycle");
        }
        Ok(Self {
            base: None,
            was_enabled: false,
            review,
            frame: 0,
            phase: None,
        })
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
