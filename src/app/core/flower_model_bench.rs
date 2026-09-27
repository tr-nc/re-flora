//! Explicit bounded Release diagnosis. Uses real planted instances, not renderer
//! proxies; the normal game and saved settings are unchanged without the env var.
use super::{planting::AuthoredFloraPlacementBatch, App};
use anyhow::{ensure, Context, Result};
use glam::{UVec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Front,
    Away,
    Outside,
}
#[derive(Clone, Copy, Debug)]
struct Parameters {
    count: u32,
    view: View,
    resolution: u32,
    heads_only: bool,
}
impl Parameters {
    fn parse(input: &str) -> Result<Self> {
        let fields = input.split(',').collect::<Vec<_>>();
        ensure!(
            fields.len() == 4,
            "expected count,front|away|outside,pixels,heads|whole"
        );
        let count = fields[0].parse::<u32>().context("flower count")?;
        ensure!(
            count <= 1024,
            "flower diagnosis is bounded to 0..1024 plants"
        );
        let view = match fields[1] {
            "front" => View::Front,
            "away" => View::Away,
            "outside" => View::Outside,
            _ => anyhow::bail!("view must be front, away or outside"),
        };
        let resolution = fields[2].parse::<u32>().context("flower resolution")?;
        ensure!((8..=64).contains(&resolution), "pixels must be 8..64");
        let heads_only = match fields[3] {
            "heads" => true,
            "whole" => false,
            _ => anyhow::bail!("mode must be heads or whole"),
        };
        Ok(Self {
            count,
            view,
            resolution,
            heads_only,
        })
    }
}
pub(super) struct FlowerModelBench {
    parameters: Parameters,
    target: Option<Vec3>,
}
impl FlowerModelBench {
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(input) = std::env::var("RE_FLORA_FLOWER_BENCH") else {
            return Ok(None);
        };
        ensure!(
            std::env::var_os("RE_FLORA_FLOWER_MODEL_REVIEW").is_none(),
            "flower bench and flower review are mutually exclusive"
        );
        Ok(Some(Self {
            parameters: Parameters::parse(&input)?,
            target: None,
        }))
    }
}
impl App {
    pub(super) fn prepare_flower_model_bench(&mut self) -> Result<()> {
        let Some(bench) = &self.flower_model_bench else {
            return Ok(());
        };
        let parameters = bench.parameters;
        let settings = &mut self.debug_settings.adjustables;
        settings.model_flower_heads_only.value = parameters.heads_only;
        settings.model_flower_pixel_resolution.value = parameters.resolution;
        settings.model_flower_size_scale.value = 1.;
        settings.model_pixel_view_count.value = 16;
        settings.model_pixel_screen_grid.value = false;
        settings.flora_growth_override_enabled.value = true;
        settings.flora_growth_override.value = 1.;
        settings.auto_daynight_cycle.value = false;
        if bench.target.is_none() {
            let center = self
                .resolve_plantable_surface_column(UVec2::new(186, 348))
                .map_err(|e| anyhow::anyhow!("flower bench center: {e:?}"))?;
            let target = center.base_center_vox() / 256.0 + Vec3::Y * 0.06;
            let mut batch = AuthoredFloraPlacementBatch::new();
            for i in 0..parameters.count {
                let column = UVec2::new(154 + (i % 32) * 2, 332 + (i / 32) * 2);
                let anchor = self
                    .resolve_plantable_surface_column(column)
                    .map_err(|e| anyhow::anyhow!("flower bench root {column:?}: {e:?}"))?;
                ensure!(
                    self.try_place_authored_flora(
                        &mut batch,
                        crate::flora::MODEL_FLOWER_FIRST_SPECIES,
                        anchor,
                        255,
                        u32::MAX,
                        i * 137
                    ),
                    "flower bench insert {i}"
                );
            }
            self.finish_authored_flora_placement(batch)?;
            self.flower_model_bench.as_mut().unwrap().target = Some(target);
            self.set_manual_time_of_day(0.45);
            log::info!("[FLOWER_BENCH] species=wild-geranium count={} view={:?} resolution={} heads_only={} target={target:?} placement=production saved=false",parameters.count,parameters.view,parameters.resolution,parameters.heads_only);
        }
        let target = self.flower_model_bench.as_ref().unwrap().target.unwrap();
        let eye = target
            + if parameters.view == View::Outside {
                Vec3::new(0., 0.2, 2.)
            } else {
                Vec3::new(0., 0.2, 0.4)
            };
        let look = if parameters.view == View::Front {
            target
        } else {
            eye + (eye - target)
        };
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(look);
        ensure!(
            self.tracer.set_camera_pose_looking_at(eye, look),
            "flower bench camera"
        );
        self.reset_camera_movement_input();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn benchmark_input_is_bounded_and_explicit() {
        let p = Parameters::parse("128,away,32,heads").unwrap();
        assert_eq!(
            (p.count, p.view, p.resolution, p.heads_only),
            (128, View::Away, 32, true)
        );
        for bad in [
            "",
            "2048,away,32,heads",
            "128,unknown,32,heads",
            "128,front,0,heads",
            "128,front,32,unknown",
        ] {
            assert!(Parameters::parse(bad).is_err(), "{bad}");
        }
    }
}
