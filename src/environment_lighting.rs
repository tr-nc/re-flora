use glam::Vec3;
use std::f32::consts::PI;

mod authored_sky {
    include!(concat!(env!("OUT_DIR"), "/sky_environment_data.rs"));
}

use authored_sky::{SKY_COLOR_ALTITUDES, SKY_COLOR_BOTTOM, SKY_COLOR_TOP};

pub(crate) const SH_COEFFICIENT_COUNT: usize = 9;
pub(crate) const IRRADIANCE_SH_BAND_FACTORS: [f32; SH_COEFFICIENT_COUNT] = [
    PI,
    2.0 * PI / 3.0,
    2.0 * PI / 3.0,
    2.0 * PI / 3.0,
    PI / 4.0,
    PI / 4.0,
    PI / 4.0,
    PI / 4.0,
    PI / 4.0,
];
const SKY_PROJECTION_SAMPLE_COUNT: usize = 2048;
const GOLDEN_ANGLE: f32 = 2.399_963_1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct EnvironmentIrradiance {
    pub coefficients: [Vec3; SH_COEFFICIENT_COUNT],
    pub revision: u32,
}

impl Default for EnvironmentIrradiance {
    fn default() -> Self {
        Self {
            coefficients: [Vec3::ZERO; SH_COEFFICIENT_COUNT],
            revision: 0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct EnvironmentLightingCache {
    current: EnvironmentIrradiance,
    last_sun_direction_bits: Option<[u32; 3]>,
}

impl EnvironmentLightingCache {
    pub fn update(&mut self, sun_direction: Vec3) -> EnvironmentIrradiance {
        let sun_direction = sun_direction.normalize_or_zero();
        let direction_bits = [
            sun_direction.x.to_bits(),
            sun_direction.y.to_bits(),
            sun_direction.z.to_bits(),
        ];
        if self.last_sun_direction_bits != Some(direction_bits) {
            self.current.coefficients = project_environment_irradiance(|direction| {
                if direction.y > 0.0 {
                    sky_radiance(direction, sun_direction)
                } else {
                    Vec3::ZERO
                }
            });
            self.current.revision = self.current.revision.wrapping_add(1).max(1);
            self.last_sun_direction_bits = Some(direction_bits);
        }
        self.current
    }
}

fn sky_colors(sun_altitude: f32) -> (Vec3, Vec3) {
    if sun_altitude <= SKY_COLOR_ALTITUDES[0] {
        return (SKY_COLOR_TOP[0], SKY_COLOR_BOTTOM[0]);
    }
    for upper in 1..SKY_COLOR_ALTITUDES.len() {
        if sun_altitude < SKY_COLOR_ALTITUDES[upper] {
            let lower = upper - 1;
            let t = (sun_altitude - SKY_COLOR_ALTITUDES[lower])
                / (SKY_COLOR_ALTITUDES[upper] - SKY_COLOR_ALTITUDES[lower]);
            return (
                SKY_COLOR_TOP[lower].lerp(SKY_COLOR_TOP[upper], t),
                SKY_COLOR_BOTTOM[lower].lerp(SKY_COLOR_BOTTOM[upper], t),
            );
        }
    }
    (
        SKY_COLOR_TOP[SKY_COLOR_TOP.len() - 1],
        SKY_COLOR_BOTTOM[SKY_COLOR_BOTTOM.len() - 1],
    )
}

fn view_altitude_factor(altitude: f32) -> f32 {
    if altitude <= -1.0 {
        0.0
    } else if altitude < -0.15 {
        (altitude + 1.0) / 0.85 * 0.03
    } else if altitude < 0.0 {
        0.03 + (altitude + 0.15) / 0.15 * (0.55 - 0.03)
    } else if altitude < 0.08 {
        0.55 + altitude / 0.08 * (0.72 - 0.55)
    } else if altitude < 0.2 {
        0.72 + (altitude - 0.08) / 0.12 * (0.86 - 0.72)
    } else if altitude < 0.4 {
        0.86 + (altitude - 0.2) / 0.2 * (0.96 - 0.86)
    } else if altitude < 1.0 {
        0.96 + (altitude - 0.4) / 0.6 * (1.0 - 0.96)
    } else {
        1.0
    }
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn henyey_greenstein_phase(cosine: f32, eccentricity: f32) -> f32 {
    let eccentricity_squared = eccentricity * eccentricity;
    let base = 1.0 + eccentricity_squared - 2.0 * eccentricity * cosine;
    (1.0 - eccentricity_squared) / (4.0 * PI * base * base.sqrt())
}

fn sky_radiance(view_direction: Vec3, sun_direction: Vec3) -> Vec3 {
    let sun_altitude = sun_direction.y;
    let (top_color, bottom_color) = sky_colors(sun_altitude);
    let blend_factor = smoothstep01(view_altitude_factor(view_direction.y));
    let base_sky_color = bottom_color.lerp(top_color, blend_factor);

    let eccentricity = 0.76 + (0.82 - 0.76) * (1.0 - sun_altitude.abs()).clamp(0.0, 1.0);
    let halo_phase = henyey_greenstein_phase(view_direction.dot(sun_direction), eccentricity);
    let eccentricity_squared = eccentricity * eccentricity;
    let one_minus_eccentricity = 1.0 - eccentricity;
    let peak = (1.0 - eccentricity_squared)
        / (4.0 * PI * one_minus_eccentricity * one_minus_eccentricity * one_minus_eccentricity);
    let normalized_halo = halo_phase / peak;
    let halo_strength =
        normalized_halo * (0.35 + (0.18 - 0.35) * (sun_altitude * 2.0).clamp(0.0, 1.0));

    let sun_blend_factor = smoothstep01(view_altitude_factor(sun_altitude));
    let halo_color = bottom_color.lerp(top_color, sun_blend_factor);
    base_sky_color.lerp(halo_color, halo_strength.clamp(0.0, 1.0))
}

// Sloan real-SH ordering adapted to the project's Y-up world:
// [Y00, Y1-1(z), Y10(y), Y11(x), Y2-2(xz), Y2-1(zy),
//  Y20(3y^2-1), Y21(xy), Y22(x^2-z^2)].
pub(crate) fn sh_basis(direction: Vec3) -> [f32; SH_COEFFICIENT_COUNT] {
    let direction = direction.normalize_or_zero();
    let x = direction.x;
    let y = direction.y;
    let z = direction.z;
    [
        0.282_094_8,
        0.488_602_52 * z,
        0.488_602_52 * y,
        0.488_602_52 * x,
        1.092_548_5 * x * z,
        1.092_548_5 * z * y,
        0.315_391_57 * (3.0 * y * y - 1.0),
        1.092_548_5 * x * y,
        0.546_274_24 * (x * x - z * z),
    ]
}

fn project_environment_irradiance(radiance: impl Fn(Vec3) -> Vec3) -> [Vec3; SH_COEFFICIENT_COUNT] {
    let mut coefficients = [Vec3::ZERO; SH_COEFFICIENT_COUNT];
    let sample_weight = 4.0 * PI / SKY_PROJECTION_SAMPLE_COUNT as f32;

    for sample_index in 0..SKY_PROJECTION_SAMPLE_COUNT {
        let y = 1.0 - 2.0 * (sample_index as f32 + 0.5) / SKY_PROJECTION_SAMPLE_COUNT as f32;
        let radius = (1.0 - y * y).max(0.0).sqrt();
        let angle = GOLDEN_ANGLE * sample_index as f32;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let direction = Vec3::new(radius * cos_angle, y, radius * sin_angle);
        let sample_radiance = radiance(direction);
        let basis = sh_basis(direction);
        for (coefficient, basis_value) in coefficients.iter_mut().zip(basis) {
            *coefficient += sample_radiance * (basis_value * sample_weight);
        }
    }

    for (coefficient, factor) in coefficients.iter_mut().zip(IRRADIANCE_SH_BAND_FACTORS) {
        *coefficient *= factor;
    }
    coefficients
}

#[cfg(test)]
fn evaluate_environment_irradiance(
    coefficients: &[Vec3; SH_COEFFICIENT_COUNT],
    normal: Vec3,
) -> Vec3 {
    let normal = if normal.length_squared() > 1.0e-8 {
        normal.normalize()
    } else {
        Vec3::Y
    };
    let basis = sh_basis(normal);
    coefficients
        .iter()
        .zip(basis)
        .fold(Vec3::ZERO, |sum, (coefficient, basis_value)| {
            sum + *coefficient * basis_value
        })
        .max(Vec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_close(actual: Vec3, expected: Vec3, tolerance: f32) {
        let difference = (actual - expected).abs();
        assert!(
            difference.max_element() <= tolerance,
            "actual={actual:?} expected={expected:?} difference={difference:?}"
        );
    }

    fn direct_diffuse_irradiance(
        radiance: impl Fn(Vec3) -> Vec3,
        normal: Vec3,
        sample_count: usize,
    ) -> Vec3 {
        let mut irradiance = Vec3::ZERO;
        let sample_weight = 4.0 * PI / sample_count as f32;
        for sample_index in 0..sample_count {
            let y = 1.0 - 2.0 * (sample_index as f32 + 0.5) / sample_count as f32;
            let radius = (1.0 - y * y).max(0.0).sqrt();
            let angle = GOLDEN_ANGLE * sample_index as f32;
            let (sin_angle, cos_angle) = angle.sin_cos();
            let direction = Vec3::new(radius * cos_angle, y, radius * sin_angle);
            irradiance += radiance(direction) * (normal.dot(direction).max(0.0) * sample_weight);
        }
        irradiance
    }

    #[test]
    fn constant_environment_reconstructs_constant_diffuse_irradiance() {
        let radiance = Vec3::new(0.2, 0.4, 0.8);
        let coefficients = project_environment_irradiance(|_| radiance);
        let expected = radiance * PI;
        for normal in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::X,
            -Vec3::Y,
            -Vec3::Z,
            Vec3::new(1.0, 2.0, 3.0).normalize(),
        ] {
            assert_vec3_close(
                evaluate_environment_irradiance(&coefficients, normal),
                expected,
                0.003,
            );
        }
    }

    #[test]
    fn upper_hemisphere_environment_has_expected_orientation() {
        let coefficients = project_environment_irradiance(|direction| {
            if direction.y > 0.0 {
                Vec3::ONE
            } else {
                Vec3::ZERO
            }
        });
        let upward = evaluate_environment_irradiance(&coefficients, Vec3::Y).x;
        let horizontal = evaluate_environment_irradiance(&coefficients, Vec3::X).x;
        let downward = evaluate_environment_irradiance(&coefficients, -Vec3::Y).x;

        assert!((upward - PI).abs() < 0.02, "upward={upward}");
        assert!(
            (horizontal - PI * 0.5).abs() < 0.02,
            "horizontal={horizontal}"
        );
        assert!(downward < 0.02, "downward={downward}");
    }

    #[test]
    fn directional_lobe_rotates_with_its_axis() {
        let project_lobe = |axis: Vec3| {
            project_environment_irradiance(|direction| Vec3::splat(direction.dot(axis).max(0.0)))
        };
        let x_coefficients = project_lobe(Vec3::X);
        let z_coefficients = project_lobe(Vec3::Z);

        let x_on_x = evaluate_environment_irradiance(&x_coefficients, Vec3::X).x;
        let x_on_z = evaluate_environment_irradiance(&x_coefficients, Vec3::Z).x;
        let z_on_z = evaluate_environment_irradiance(&z_coefficients, Vec3::Z).x;
        let z_on_x = evaluate_environment_irradiance(&z_coefficients, Vec3::X).x;

        assert!((x_on_x - z_on_z).abs() < 0.01);
        assert!((x_on_z - z_on_x).abs() < 0.01);
        assert!(x_on_x > x_on_z * 1.8);
    }

    #[test]
    fn irradiance_evaluation_clamps_negative_reconstruction() {
        let mut coefficients = [Vec3::ZERO; SH_COEFFICIENT_COUNT];
        coefficients[0] = Vec3::splat(-1.0);
        assert_eq!(
            evaluate_environment_irradiance(&coefficients, Vec3::Y),
            Vec3::ZERO
        );
    }

    #[test]
    fn cache_revision_changes_only_with_environment_direction() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(Vec3::Y);
        let unchanged = cache.update(Vec3::Y);
        let changed = cache.update(Vec3::Z);

        assert_eq!(first.revision, 1);
        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(changed.revision, first.revision + 1);
        assert_ne!(changed.coefficients, first.coefficients);
    }

    #[test]
    fn projected_sky_matches_direct_numerical_diffuse_integration() {
        let sun_direction = Vec3::new(0.25, 0.75, -0.35).normalize();
        let sky = |direction: Vec3| {
            if direction.y > 0.0 {
                sky_radiance(direction, sun_direction)
            } else {
                Vec3::ZERO
            }
        };
        let coefficients = project_environment_irradiance(sky);

        for normal in [
            Vec3::Y,
            Vec3::X,
            Vec3::Z,
            Vec3::new(1.0, 1.0, 0.0).normalize(),
            Vec3::new(0.0, -0.25, 1.0).normalize(),
        ] {
            let projected = evaluate_environment_irradiance(&coefficients, normal);
            let direct = direct_diffuse_irradiance(sky, normal, 32_768);
            let absolute_error = (projected - direct).abs();
            assert!(
                absolute_error.max_element() < 0.025,
                "normal={normal:?} projected={projected:?} direct={direct:?} \
                 absolute_error={absolute_error:?}"
            );
        }
    }

    #[test]
    fn terrain_and_raster_consumers_share_the_probe_sampler_contract() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");

        assert!(shared.contains("worldPosition * lighting.environment_probe_world_to_grid_scale"));
        assert!(shared.contains("environment_probe_coefficients.data[probeIndex]"));
        assert!(shared.contains("environment_probe_summaries.data[probeIndex]"));
        assert!(shared.contains("environmentProbeSurfaceVisibility("));
        assert!(shared.contains("environmentProbeAxialHitDistance("));
        assert!(shared.contains("saturate(dot(normal, surfaceToProbeDirection))"));
        assert!(shared.contains("hasNearestTrustedProbe"));
        assert!(terrain.contains("sampleEnvironmentIrradiance("));
        assert!(raster.contains("sampleEnvironmentIrradiance("));
        for consumer in [
            include_str!("../shader/slang/flora.vert.slang"),
            include_str!("../shader/slang/flora_lod.vert.slang"),
            include_str!("../shader/slang/leaves.vert.slang"),
            include_str!("../shader/slang/leaves_lod.vert.slang"),
        ] {
            assert!(consumer.contains("import flora_vertex;"));
        }
    }
}
