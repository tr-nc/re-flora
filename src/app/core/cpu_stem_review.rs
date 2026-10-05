//! Opt-in CPU-owned path stress fixture; real band upload/raster/shadow paths.
//! Flower-like branches and climbing-like curves are authored on CPU every frame.
use super::App;
use crate::tracer::{path_bands, DynamicFruitRenderInstance, StemPathPoint};
use anyhow::{ensure, Result};
use glam::{Mat3, Quat, Vec2, Vec3};
use std::time::Instant;

#[derive(Debug)]
pub(super) struct CpuStemReview {
    pub(super) frame: u32,
    case: String,
    shape: String,
    plants: u32,
    experimental: bool,
    candidate: u32,
    roots: Vec<Vec3>,
}
impl CpuStemReview {
    pub(super) fn from_environment() -> Option<Self> {
        let case = std::env::var("RE_FLORA_CPU_STEM_REVIEW").ok()?;
        Some(Self::parse(&case).expect("CPU stem review case"))
    }
    fn parse(case: &str) -> Result<Self> {
        let parts: Vec<_> = case.split('-').collect();
        ensure!(
            parts.len() == 3,
            "expected <flower|vine|growth>-<1..2048>-<a|b>"
        );
        ensure!(
            ["flower", "vine", "growth"].contains(&parts[0]),
            "unknown CPU path shape"
        );
        let plants: u32 = parts[1].parse()?;
        ensure!(
            (1..=2048).contains(&plants),
            "CPU path population must be 1..2048"
        );
        ensure!(
            ["a", "b"].contains(&parts[2]),
            "CPU path mode must be a or b"
        );
        let candidate =
            std::env::var("RE_FLORA_STEM_BAND_MODE").map_or(Ok(1), |v| v.parse::<u32>())?;
        ensure!(
            candidate <= 1,
            "stem candidate must be 0 (analytic grass) or 1 (square bands)"
        );
        Ok(Self {
            frame: 0,
            case: case.into(),
            shape: parts[0].into(),
            plants,
            experimental: parts[2] == "b",
            candidate,
            roots: Vec::new(),
        })
    }
    pub(super) fn advance(&mut self, app: &mut App) -> Result<bool> {
        let frame = self.frame;
        self.frame += 1;
        ensure!(
            app.perf_logging && app.gpu_profiler.is_some(),
            "CPU stem review requires GPU --perf"
        );
        let focus_xz = Vec2::splat(0.85);
        if self.roots.is_empty() {
            let width = (self.plants as f32).sqrt().ceil() as u32;
            for i in 0..self.plants {
                let step = 0.8 / width as f32;
                let xz = focus_xz
                    + Vec2::new(
                        (i % width) as f32 - (width as f32 - 1.) * 0.5,
                        (i / width) as f32 - (width as f32 - 1.) * 0.5,
                    ) * step;
                self.roots.push(Vec3::new(
                    xz.x,
                    app.query_terrain_height_cpu(xz) + 0.001,
                    xz.y,
                ));
            }
        }
        let target = Vec3::new(0.85, app.query_terrain_height_cpu(focus_xz) + 0.07, 0.85);
        let near = std::env::var_os("RE_FLORA_CPU_STEM_NEAR").is_some();
        let offset = if near {
            Vec3::new(0.03, 0.04, 0.10)
        } else {
            Vec3::new(0.38, 0.55, 0.90)
        };
        app.camera_control.set_orbit_focus(target);
        ensure!(
            app.tracer
                .set_camera_pose_looking_at(target + offset, target),
            "CPU review camera"
        );
        app.debug_settings.adjustables.stem_band_mode.value = self.candidate;
        app.debug_settings.adjustables.cpu_stem_band_rendering.value = self.experimental;
        let start = Instant::now();
        let time = frame as f32 / 60.;
        let growing = self.shape == "growth";
        let visible_nodes = if growing {
            2 + (frame.min(300) * 22 / 300)
        } else {
            24
        };
        let mut bands = Vec::new();
        for (plant, &root) in self.roots.iter().enumerate() {
            let phase = plant as f32 * 1.37;
            let mut points = Vec::new();
            for i in 0..=visible_nodes {
                let t = i as f32 / 24.;
                let wind = (time * 2.1 + phase).sin() * 0.009 * t * t;
                let (offset, derivative) = if self.shape == "vine" {
                    let angle = t * 5. + phase;
                    (
                        Vec3::new(
                            angle.sin() * 0.035 * t + wind,
                            t * 0.18,
                            angle.cos() * 0.025 * t,
                        ),
                        Vec3::new(
                            0.035 * (angle.sin() + 5. * t * angle.cos()),
                            0.18,
                            0.025 * (angle.cos() - 5. * t * angle.sin()),
                        ),
                    )
                } else {
                    (
                        Vec3::new(t * t * 0.025 + wind, t * 0.18, 0.),
                        Vec3::new(t * 0.05, 0.18, 0.),
                    )
                };
                points.push(StemPathPoint {
                    position: root + offset,
                    rest_arc: t * 0.18,
                    tangent: derivative.normalize(),
                    side: Vec3::X,
                    parent: (i > 0).then(|| i as usize - 1),
                });
            }
            if self.shape == "flower" {
                for (attachment, sign) in [(8usize, -1.), (12, 1.)] {
                    let base = points[attachment];
                    let mut parent = attachment;
                    for i in 1..=6 {
                        let t = i as f32 / 6.;
                        let position =
                            base.position + Vec3::new(sign * 0.05 * t, 0.045 * t, 0.01 * t * t);
                        points.push(StemPathPoint {
                            position,
                            rest_arc: base.rest_arc + 0.06 * t,
                            tangent: Vec3::new(sign * 0.05, 0.045, 0.02 * t).normalize(),
                            side: Vec3::Z,
                            parent: Some(parent),
                        });
                        parent = points.len() - 1;
                    }
                }
            }
            bands.extend(path_bands(
                &points,
                0.18,
                8,
                0.65 / 256.,
                [Vec3::new(0.08, 0.24, 0.04), Vec3::new(0.55, 0.72, 0.12)],
            ));
        }
        let count = bands.len();
        let path_us = start.elapsed().as_micros();
        if self.experimental {
            app.tracer.show_climbing_plant_geometry(&[])?;
            app.tracer.show_cpu_stem_bands(&bands)?;
        } else {
            let blocks = bands
                .iter()
                .map(|band| {
                    let a = Vec3::from_slice(&band.a_radius[..3]);
                    let b = Vec3::from_slice(&band.b_radius[..3]);
                    let up = (b - a).normalize();
                    let preferred = Vec3::from_array(band.side_a);
                    let side = (preferred - up * preferred.dot(up))
                        .try_normalize()
                        .unwrap_or_else(|| up.any_orthonormal_vector());
                    let mut instance = DynamicFruitRenderInstance::new(
                        (a + b) * 0.5,
                        Quat::from_mat3(&Mat3::from_cols(side, up, side.cross(up))),
                        1.,
                    );
                    instance.dimensions = Vec3::new(
                        band.a_radius[3] * 2.,
                        (b - a).length(),
                        band.a_radius[3] * 2.,
                    );
                    instance.color = Vec3::from_array(band.color_srgb);
                    instance
                })
                .collect::<Vec<_>>();
            app.tracer.show_cpu_stem_bands(&[])?;
            app.tracer.show_climbing_plant_geometry(&blocks)?;
        }
        let upload_us = start.elapsed().as_micros() - path_us;
        log::info!("[CPU_STEM_TIMING] simulation_frame={frame} path_us={path_us} upload_us={upload_us} bands={count}");
        if frame == 120 {
            if let Some(directory) = std::env::var_os("RE_FLORA_GRASS_STEM_CAPTURE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory)?;
                *app.launch_owners.screenshot_mut() =
                    super::screenshot::ScreenshotRuntime::new(Some(crate::ScreenshotOptions {
                        path: directory
                            .join(format!("{}.png", self.case))
                            .to_string_lossy()
                            .into_owned(),
                        delay: 0.,
                        sequence: None,
                    }));
            }
        }
        if [0, 120, 420].contains(&frame) {
            log::info!("[CPU_STEM_REVIEW] case={} phase={} app_frame={} simulation_frame={frame} plants={} bands={count} candidate={} growth={growing} saved=false",
                self.case,if frame==0 { "start" } else if frame==120 { "sample" } else { "complete" },app.time_info.total_frame_count(),self.plants,self.candidate);
        }
        Ok(frame == 420)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_both_cpu_path_shapes_and_rejects_unbounded_population() {
        for shape in ["flower", "vine", "growth"] {
            let a = CpuStemReview::parse(&format!("{shape}-1024-a")).unwrap();
            let b = CpuStemReview::parse(&format!("{shape}-1024-b")).unwrap();
            assert_eq!(a.plants, b.plants);
            assert_eq!(a.shape, b.shape);
            assert!(!a.experimental && b.experimental);
        }
        assert!(CpuStemReview::parse("vine-0-b").is_err());
        assert!(CpuStemReview::parse("vine-2049-b").is_err());
    }
}
