//! Independent bake-only CPU oracle. No instances, frame rendering or shader fallback.
use super::model_pixel_cache::{source, Spec, Triangle};
use super::model_pixel_repair;
use anyhow::{ensure, Result};
#[cfg(test)]
use bytemuck::Zeroable;
use glam::{Mat4, Vec2, Vec3, Vec4};
fn intersect(
    origin: Vec3,
    direction: Vec3,
    triangle: &Triangle,
    position_error: f32,
) -> Option<f32> {
    let a = Vec3::from_slice(&triangle.a);
    let e1 = Vec3::from_slice(&triangle.e1);
    let e2 = Vec3::from_slice(&triangle.e2);
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = origin - a;
    let reciprocal = 1.0 / det;
    let u = s.dot(p) * reciprocal;
    let q = s.cross(e1);
    let v = direction.dot(q) * reciprocal;
    let t = e2.dot(q) * reciprocal;
    // Diagnostic-only spatial uncertainty, not conservative rendering. Tiny
    // leaves amplify CPU/GPU float rounding in inverse-pose rays. Convert the
    // position envelope into each edge's barycentric units, not a blanket UV slack.
    let area = e1.cross(e2).length();
    let allowance = position_error / area;
    (u >= -1e-6 - allowance * e2.length()
        && v >= -1e-6 - allowance * e1.length()
        && u + v <= 1.000001 + allowance * (e2 - e1).length()
        && t > 0.)
        .then_some(t)
}

fn reference_depth(
    origin: Vec3,
    direction: Vec3,
    triangles: &[Triangle],
    depth: impl Fn(f32) -> f32,
    observed: f32,
) -> Option<(f32, bool)> {
    let nearest = |error| {
        triangles
            .iter()
            .filter_map(|t| intersect(origin, direction, t, error))
            .min_by(f32::total_cmp)
    };
    if let Some(value) = nearest(0.)
        .map(&depth)
        .filter(|v| (*v - observed).abs() < 0.00002)
    {
        return Some((value, false));
    }
    // At a silhouette, roundoff may select the front face on GPU and a different
    // rear face on CPU (not just turn a hit into a miss). Check possible edge hits,
    // but never accept geometry hidden behind a definite interior hit.
    let envelope = 16. * f32::EPSILON * origin.length().max(1.);
    let cutoff = nearest(-envelope).unwrap_or(f32::INFINITY) + 2. * envelope;
    triangles
        .iter()
        .filter_map(|t| intersect(origin, direction, t, envelope))
        .filter(|t| *t <= cutoff)
        .map(depth)
        .min_by(|a, b| (a - observed).abs().total_cmp(&(b - observed).abs()))
        .map(|d| (d, true))
}

/// Forward error for verifying projection *identity*, not cell membership. The
/// existing spatial roundoff envelope (16 machine epsilons) is propagated through
/// the camera dot products and perspective divide. It never enters `plan`'s
/// conservative edge tests or the independent depth tolerance.
fn projection_roundoff(ndc: Vec3, vp: Mat4) -> Vec3 {
    let world_h = vp.inverse() * ndc.extend(1.);
    let world = world_h.truncate() / world_h.w;
    let magnitude =
        Mat4::from_cols_array(&vp.to_cols_array().map(f32::abs)) * world.abs().extend(1.);
    let error = magnitude * (16. * f32::EPSILON);
    let w = world_h.w.recip().abs();
    (error.truncate() + ndc.abs() * error.w) / (w - error.w).max(f32::MIN_POSITIVE)
}

/// Decode the same clipped vertices used by the real producer, never its hit
/// mask. Independently check their model/camera identity against CPU projection,
/// retain CPU vertex depths, and let the unmodified CPU planner decide coverage.
#[allow(clippy::too_many_arguments)]
fn checked_projected_groups(
    expected: &[model_pixel_repair::Group],
    evidence: &[[f32; 4]],
    first: usize,
    count: usize,
    bounds: Vec4,
    vp: Mat4,
    center_depth: f32,
    n: u32,
) -> Result<Vec<model_pixel_repair::Group>> {
    ensure!(
        evidence.len() >= 1 + count * 13,
        "invalid GPU projection evidence length"
    );
    let observed_bounds = Vec4::from_array(evidence[0]);
    ensure!(
        observed_bounds.is_finite(),
        "nonfinite GPU projection bounds"
    );
    for (a, b) in [
        (
            Vec2::new(bounds.x, bounds.y),
            Vec2::new(observed_bounds.x, observed_bounds.y),
        ),
        (
            Vec2::new(bounds.z, bounds.w),
            Vec2::new(observed_bounds.z, observed_bounds.w),
        ),
    ] {
        let depth = if center_depth.is_finite() {
            center_depth.clamp(0., 1.)
        } else {
            0.
        };
        let error = projection_roundoff(a.extend(depth), vp).truncate();
        ensure!(
            (a - b).abs().cmple(error).all(),
            "GPU projection bounds/frame mismatch"
        );
    }
    let mut groups = expected.to_vec();
    for source in 0..count {
        let at = 1 + source * 13;
        let header = evidence[at];
        ensure!(
            header[0].is_finite()
                && (0.0..=12.0).contains(&header[0])
                && header[0].fract() == 0.
                && header[1].to_bits() as usize == first + source
                && header[2] == n as f32,
            "GPU projected triangle identity mismatch"
        );
        let vertices = header[0] as usize;
        let expected_count = groups
            .iter()
            .flat_map(|g| &g.sources)
            .filter(|s| **s as usize == source)
            .count();
        ensure!(
            vertices.saturating_sub(2) == expected_count,
            "GPU projected clipping topology mismatch"
        );
        let mut fan = 0;
        for group in &mut groups {
            for (triangle, &s) in group.triangles.iter_mut().zip(&group.sources) {
                if s as usize != source {
                    continue;
                }
                for (point, vertex) in triangle.iter_mut().zip([0, fan + 1, fan + 2]) {
                    let observed = Vec3::from_slice(&evidence[at + 1 + vertex]);
                    ensure!(observed.is_finite(), "nonfinite GPU projected vertex");
                    let cpu_ndc = Vec3::new(
                        bounds.x + point[0] as f32 / n as f32 * (bounds.z - bounds.x),
                        bounds.y + point[1] as f32 / n as f32 * (bounds.w - bounds.y),
                        point[2] as f32,
                    );
                    let gpu_ndc = Vec2::new(observed_bounds.x, observed_bounds.y)
                        + observed.truncate() / n as f32
                            * Vec2::new(
                                observed_bounds.z - observed_bounds.x,
                                observed_bounds.w - observed_bounds.y,
                            );
                    // Tile XY -> NDC includes subtraction of the bounds origin. Near NDC
                    // zero that absolute representation error dominates the camera error.
                    // This verifies coordinates only; never enlarge the coverage epsilon.
                    let tile_error = (cpu_ndc.truncate().abs() + Vec2::ONE) * (4. * f32::EPSILON);
                    let error = projection_roundoff(cpu_ndc, vp).truncate() + tile_error;
                    ensure!(
                        (cpu_ndc.truncate() - gpu_ndc).abs().cmple(error).all(),
                        "GPU projected vertex/frame mismatch source={source} fan={fan} cpu={cpu_ndc:?} gpu={gpu_ndc:?} error={error:?}"
                    );
                    ensure!(
                        (observed.z - point[2] as f32).abs() < 0.00002,
                        "GPU projected depth mismatch"
                    );
                    // Only the producer's tile-space coordinates replace the CPU
                    // reprojection. Surface depths remain independently derived.
                    point[0] = f64::from(observed.x);
                    point[1] = f64::from(observed.y);
                }
                fan += 1;
            }
        }
    }
    Ok(groups)
}

// The renderer's closest-edge choice is explicitly float32: reconstructing a
// closest point in tile coordinates can round two squared distances to a tie.
// A float64 minimizer can select a different edge and depth (captured below).
fn projected_depth(t: &[[f64; 3]; 3], pixel: Vec2) -> f32 {
    let p = t.map(|v| Vec2::new(v[0] as f32, v[1] as f32));
    let ab = p[1] - p[0];
    let ac = p[2] - p[0];
    let ap = pixel - p[0];
    let area = ab.x * ac.y - ab.y * ac.x;
    let mut weights = Vec3::X;
    if area.abs() > 1e-10 {
        let u = (ap.x * ac.y - ap.y * ac.x) / area;
        let v = (ab.x * ap.y - ab.y * ap.x) / area;
        if u >= 0. && v >= 0. && u + v <= 1. {
            weights = Vec3::new(1. - u - v, u, v);
            return weights.dot(Vec3::new(t[0][2] as f32, t[1][2] as f32, t[2][2] as f32));
        }
    }
    let mut best = f32::INFINITY;
    for i in 0..3 {
        let j = (i + 1) % 3;
        let edge = p[j] - p[i];
        let u = ((pixel - p[i]).dot(edge) / edge.length_squared().max(1e-20)).clamp(0., 1.);
        let delta = p[i] + edge * u - pixel;
        let distance = delta.length_squared();
        if distance < best {
            best = distance;
            weights = Vec3::ZERO;
            weights[i] = 1. - u;
            weights[j] = u;
        }
    }
    weights.dot(Vec3::new(t[0][2] as f32, t[1][2] as f32, t[2][2] as f32))
}

fn bake_coverage_depths(groups: &[model_pixel_repair::Group], n: usize) -> Vec<Option<f32>> {
    let mut depths: Vec<Option<f32>> = vec![None; n * n];
    for t in groups.iter().flat_map(|g| &g.triangles) {
        let lo = [0, 1].map(|a| t.iter().map(|v| v[a]).fold(f64::INFINITY, f64::min));
        let hi = [0, 1].map(|a| t.iter().map(|v| v[a]).fold(f64::NEG_INFINITY, f64::max));
        let start = [0, 1].map(|a| (lo[a] - 1. - 0.0001).ceil().max(0.));
        let end = [0, 1].map(|a| (hi[a] + 0.0001).floor().min(n as f64 - 1.));
        if (0..2).any(|a| end[a] < start[a]) {
            continue;
        }
        let winding = if (t[1][0] - t[0][0]) * (t[2][1] - t[0][1])
            - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])
            >= 0.
        {
            1.
        } else {
            -1.
        };
        for y in start[1] as usize..=end[1] as usize {
            for x in start[0] as usize..=end[0] as usize {
                if (0..3).all(|i| {
                    let j = (i + 1) % 3;
                    let dx = t[j][0] - t[i][0];
                    let dy = t[j][1] - t[i][1];
                    winding * (dx * (y as f64 + 0.5 - t[i][1]) - dy * (x as f64 + 0.5 - t[i][0]))
                        >= -(0.5 + 0.0001) * (dx.abs() + dy.abs())
                }) {
                    let depth = projected_depth(t, Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                    let current = &mut depths[y * n + x];
                    if current.is_none_or(|d| depth < d) {
                        *current = Some(depth);
                    }
                }
            }
        }
    }
    depths
}

/// Two canonical directions per authored source, at the actual requested resolution.
/// Full bit validation covers every other direction in the bake compute pass.
pub(super) fn layout(spec: Spec) -> (usize, usize) {
    let sources = source();
    let ranges: Vec<_> = sources
        .ranges
        .iter()
        .filter(|r| r[2] == spec.kind)
        .collect();
    let triangles = ranges.iter().map(|r| r[1] as usize).max().unwrap();
    (
        spec.resolution.pow(2) as usize * 5 + 1 + triangles * 13,
        ranges.len() * 2,
    )
}

pub(super) fn validate(spec: Spec, bytes: &[u8]) -> Result<()> {
    let data: &[[f32; 4]] =
        bytemuck::try_cast_slice(bytes).map_err(|e| anyhow::anyhow!("bake evidence ABI: {e}"))?;
    let (stride, cases) = layout(spec);
    ensure!(data.len() == stride * cases, "incomplete bake evidence");
    let s = source();
    let n = spec.resolution as usize;
    let mut checked = 0;
    let mut centers = 0;
    let mut coverage = 0;
    let mut max_error = 0f32;
    let bake_camera = Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::new(0., 0., -0.5, 0.),
        Vec4::new(0., 0., 0.5, 1.),
    );
    let bounds = Vec4::new(-1., -1., 1., 1.);
    for (shape, (index, range)) in s
        .ranges
        .iter()
        .enumerate()
        .filter(|(_, r)| r[2] == spec.kind)
        .enumerate()
    {
        for side in 0..2 {
            let view = if side == 0 { 0 } else { spec.views - 1 };
            let az = super::model_pixel_views::azimuths()[view as usize];
            let y = 1. - 2. * (view as f32 + 0.5) / spec.views as f32;
            let r = (1. - y * y).max(0.).sqrt();
            let z = Vec3::new(az[0] * r, y, az[1] * r);
            let up = if z.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
            let x = up.cross(z).normalize();
            let y = z.cross(x);
            let rotation = glam::Mat3::from_cols(x, y, z);
            let frame = s.frames[index];
            let pivot = Vec3::from_slice(&frame);
            let radius = frame[3];
            let local = Mat4::from_mat3(rotation.transpose()) * Mat4::from_translation(-pivot);
            let vp = bake_camera * Mat4::from_scale(Vec3::splat(1. / radius)) * local;
            let triangles = &s.triangles[range[0] as usize..(range[0] + range[1]) as usize];
            let points: Vec<_> = triangles
                .iter()
                .map(|t| {
                    let a = Vec3::from_slice(&t.a);
                    (
                        1,
                        [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)],
                    )
                })
                .collect();
            let (_, cpu_groups) =
                model_pixel_repair::project(&points, vp, bounds, n, |_, _, _| None);
            let base = (shape * 2 + side) * stride;
            let groups = checked_projected_groups(
                &cpu_groups,
                &data[base + n * n * 5..base + stride],
                range[0] as usize,
                range[1] as usize,
                bounds,
                vp,
                0.5,
                spec.resolution,
            )?;
            let owners: Vec<_> = (0..n * n)
                .map(|i| u32::from(data[base + i * 5 + 2][0] < 1.))
                .collect();
            let expected_coverage = bake_coverage_depths(&groups, n);
            for i in 0..n * n {
                let at = base + i * 5;
                let position = data[at];
                let normal = data[at + 1];
                let center = data[at + 2];
                ensure!(
                    position.iter().chain(&normal).all(|v| v.is_finite()),
                    "nonfinite baked surface"
                );
                ensure!((0.0..=1.).contains(&position[3]), "invalid canonical depth");
                ensure!(
                    (position[3] - center[2]).abs() < 0.00002,
                    "instrumented bake differs from stored depth"
                );
                let expected = if owners[i] != 0 {
                    centers += 1;
                    let triangle = center[1].to_bits();
                    ensure!(
                        (range[0]..range[0] + range[1]).contains(&triangle),
                        "bake center outside source range"
                    );
                    let origin = Vec3::from_slice(&data[at + 3]);
                    let direction = Vec3::from_slice(&data[at + 4]);
                    let ndc = Vec2::new((i % n) as f32 + 0.5, (i / n) as f32 + 0.5) / n as f32 * 2.
                        - Vec2::ONE;
                    let expected_origin = pivot + rotation * Vec3::new(ndc.x, ndc.y, 1.) * radius;
                    ensure!(
                        origin.distance(expected_origin)
                            < 128. * f32::EPSILON * expected_origin.length().max(1.),
                        "bake ray origin/frame mismatch"
                    );
                    ensure!(
                        direction.distance(-z) < 128. * f32::EPSILON,
                        "bake ray direction/frame mismatch"
                    );
                    let depth = |t: f32| {
                        let clip = vp * (origin + direction * t).extend(1.);
                        clip.z / clip.w
                    };
                    let (expected, _) =
                        reference_depth(origin, direction, triangles, depth, center[0])
                            .ok_or_else(|| {
                                anyhow::anyhow!("bake center outside independent CPU geometry")
                            })?;
                    ensure!(
                        (expected - center[0]).abs() < 0.00002,
                        "bake center depth mismatch"
                    );
                    ensure!(
                        center[2].to_bits() == center[0].to_bits(),
                        "coverage overwrote an occupied center"
                    );
                    Some(expected)
                } else {
                    expected_coverage[i]
                };
                ensure!(
                    (position[3] < 1.) == expected.is_some(),
                    "bake coverage mismatch kind={} source={index} view={view} pixel={},{}",
                    spec.kind,
                    i % n,
                    i / n
                );
                if let Some(expected) = expected {
                    let error = (position[3] - expected).abs();
                    max_error = max_error.max(error);
                    ensure!(error < 0.00002, "bake conservative depth mismatch {error} kind={} source={index} view={view} pixel={},{} gpu={} expected={expected} center={center:?} source_gpu={}",spec.kind,i%n,i/n,position[3],center[3].to_bits()-range[0]);
                    ensure!(
                        (Vec3::from_slice(&normal).length() - 1.).abs() < 0.00002,
                        "bake normal not unit length"
                    );
                    coverage += usize::from(owners[i] == 0);
                    checked += 1;
                }
            }
        }
    }
    ensure!(
        centers > 0 && coverage > 0,
        "bake diagnostic needs actual center and coverage samples"
    );
    log::info!("[MODEL_CACHE_GEOMETRY_CHECK] kind={} resolution={} views={} cases={cases} checked_hits={checked} centers={centers} coverage={coverage} max_depth_error={max_error:.9}",spec.kind,spec.resolution,spec.views);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bake_coverage_rejects_outside_degenerate_triangles_before_integer_clamping() {
        for coordinate in [-10., 10.] {
            let groups = [model_pixel_repair::Group {
                id: 1,
                sources: vec![0],
                triangles: vec![[[coordinate, coordinate, 0.5]; 3]],
            }];
            assert!(bake_coverage_depths(&groups, 8).iter().all(Option::is_none));
        }
    }
    #[test]
    fn closest_edge_keeps_float32_tile_reconstruction_before_depth_selection() {
        let triangle = [
            [22.33051300048828, 18.695873260498047, 0.36022424697875977],
            [23.33091926574707, 19.82186508178711, 0.29224061965942383],
            [22.9813289642334, 20.306640625, 0.28967487812042236],
        ];
        assert_eq!(
            projected_depth(&triangle, Vec2::new(22.5, 20.5)),
            triangle[2][2] as f32
        );
    }

    #[test]
    fn captured_leaf_boundary_uses_producer_projection_not_cpu_reprojection() {
        // Exact triangle 29 from the native red capture. Both center samplers
        // miss (11,26); only CPU/GPU perspective division rounding differs.
        let expected = vec![model_pixel_repair::Group {
            id: 1,
            sources: vec![0],
            triangles: vec![[
                [11.248796022613837, 25.61142196069578, 0.9694041609764099],
                [13.594012495069657, 26.658468305246924, 0.9694223403930664],
                [13.542512940626507, 26.797484463989964, 0.9694259762763977],
            ]],
        }];
        let bounds = Vec4::new(-0.33905077, -0.06143841, -0.31456983, -0.028007094);
        let vp = Mat4::from_cols_array(&[
            0.97427857,
            0.,
            0.,
            0.,
            0.,
            -1.7320508,
            0.,
            0.,
            0.,
            0.,
            -1.001001,
            -1.,
            -0.97427857,
            2.6846786,
            1.7917918,
            1.8,
        ]);
        let mut evidence = [[0.; 4]; 14];
        evidence[0] = [-0.3390508, -0.06143841, -0.31456983, -0.028007094];
        evidence[1] = [3., f32::from_bits(605), 64., 0.];
        evidence[2] = [11.248876, 25.611423, 0.9694041, 0.];
        evidence[3] = [13.594081, 26.658474, 0.9694223, 0.];
        evidence[4] = [13.54262, 26.797483, 0.9694259, 0.];
        let decode = |data: &[[f32; 4]]| {
            checked_projected_groups(&expected, data, 605, 1, bounds, vp, 0.969, 64)
        };
        let observed = decode(&evidence).unwrap();
        let covered = |groups: &[model_pixel_repair::Group], x: u32, y: u32| {
            model_pixel_repair::plan(&vec![0; 64 * 64], groups, 64)
                .nodes
                .iter()
                .any(|node| node.links[0] == y * 64 + x)
        };
        assert!(
            covered(&expected, 11, 26),
            "the old oracle must reproduce the false omission"
        );
        assert!(!covered(&observed, 11, 26));
        assert!(
            covered(&observed, 12, 26),
            "a real missing coverage cell must still fail"
        );
        for k in 0..3 {
            assert_eq!(
                observed[0].triangles[0][k][2], expected[0].triangles[0][k][2],
                "depths must remain CPU-derived"
            );
        }
        // Projection evidence is not trusted as a mask or arbitrary geometry.
        let mut bad = evidence;
        bad[1][1] = f32::from_bits(606);
        assert!(decode(&bad).is_err());
        bad = evidence;
        bad[1][0] = 0.;
        assert!(decode(&bad).is_err());
        bad = evidence;
        bad[2][0] += 1.;
        assert!(decode(&bad).is_err());
        bad = evidence;
        bad[2][2] += 0.001;
        assert!(decode(&bad).is_err());
        bad = evidence;
        bad[0][0] += 0.01;
        assert!(decode(&bad).is_err());
        bad = evidence;
        bad[2][1] = f32::NAN;
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn small_leaf_requires_observations_from_the_executed_coverage_loop() {
        // Triangle 30: homogeneous clips and bounds match, but a separately
        // compiled projection rounds division differently. Both f64 and f32
        // classify the old exported vertices as covered, the real ones outside.
        let mut expected = vec![model_pixel_repair::Group {
            id: 1,
            sources: vec![0],
            triangles: vec![[
                [5.997326374053955, 11.06486988067627, 0.9684357047080994],
                [5.986303329467773, 11.020524978637695, 0.968437135219574],
                [6.267250061035156, 10.604071617126465, 0.9684338569641113],
            ]],
        }];
        let vp = Mat4::from_cols_array(&[
            0.97427857,
            0.,
            0.,
            0.,
            0.,
            -1.7320508,
            0.,
            0.,
            0.,
            0.,
            -1.001001,
            -1.,
            -0.97427857,
            2.6846786,
            1.7917918,
            1.8,
        ]);
        let bounds = Vec4::new(0.5331397, 0.8867834, 0.5404943, 0.89956045);
        let mut data = [[0.; 4]; 14];
        data[0] = [0.5331397, 0.88678336, 0.5404943, 0.89956045];
        data[1] = [3., f32::from_bits(30), 16., 0.];
        data[2] = [5.997374, 11.064843, 0.96843576, 0.];
        data[3] = [5.9863524, 11.020508, 0.96843714, 0.];
        data[4] = [6.2672176, 10.6040945, 0.9684339, 0.];
        let observed =
            checked_projected_groups(&expected, &data, 30, 1, bounds, vp, 0.9684, 16).unwrap();
        let covers = |groups: &[model_pixel_repair::Group], x: u32, y: u32| {
            model_pixel_repair::plan(&[0; 256], groups, 16)
                .nodes
                .iter()
                .any(|node| node.links[0] == y * 16 + x)
        };
        assert!(covers(&expected, 5, 10));
        assert!(!covers(&observed, 5, 10));
        assert!(covers(&observed, 6, 10));
        // Nearby cells and both windings, not just a fortunate single pose.
        for shift in [-0.01, -0.001, 0., 0.001, 0.01] {
            for reverse in [false, true] {
                expected = observed.clone();
                for p in &mut expected[0].triangles[0] {
                    p[0] += shift;
                }
                if reverse {
                    expected[0].triangles[0].swap(1, 2);
                }
                assert_eq!(covers(&expected, 5, 10), shift < 0.);
                assert!(covers(&expected, 6, 10));
            }
        }
        // Identity/depth checks remain independent; an observer isn't a mask.
        let decode = |data: &[[f32; 4]]| {
            checked_projected_groups(&observed, data, 30, 1, bounds, vp, 0.9684, 16)
        };
        for lane in [0, 1, 2] {
            let mut wrong = data;
            wrong[2][lane] += 0.1;
            assert!(decode(&wrong).is_err());
        }
        let mut wrong = data;
        wrong[1][1] = f32::from_bits(31);
        assert!(decode(&wrong).is_err());
    }

    #[test]
    fn tiny_leaf_ray_boundary_uses_a_spatial_not_fixed_uv_tolerance() {
        // Captured 0.25x GPU center hit: strict CPU plane point lies ~3.7e-7
        // world units outside one edge, though its depth agrees within 3e-7.
        let origin = Vec3::new(-691.859, 93.85879, 301.6021);
        let direction = Vec3::new(0.90993404, -0.12223503, -0.39633125);
        let triangle = Triangle {
            a: [-0.029101437, 0.84375, 0.13554272, 0.],
            e1: [0.2248846, 0., 0.046566904, 0.],
            e2: [-0.005898563, 0.28125, 0.08335914, 0.],
            ..Triangle::zeroed()
        };
        let envelope = 16. * f32::EPSILON * origin.length();
        assert!(intersect(origin, direction, &triangle, 0.).is_none());
        assert!(intersect(origin, direction, &triangle, envelope).is_some());
        assert!(intersect(origin + Vec3::Y * 0.05, direction, &triangle, envelope).is_none());
    }
    #[test]
    fn boundary_reference_allows_front_edge_but_not_hidden_back_geometry() {
        let triangle = |a: Vec3, b: Vec3, c: Vec3| Triangle {
            a: a.extend(0.).to_array(),
            e1: (b - a).extend(0.).to_array(),
            e2: (c - a).extend(0.).to_array(),
            ..Triangle::zeroed()
        };
        let front = triangle(
            Vec3::new(0.001, -1., 0.),
            Vec3::new(1., -1., 0.),
            Vec3::new(0.001, 1., 0.),
        );
        let back = triangle(
            Vec3::new(-1., -1., -1.),
            Vec3::new(1., -1., -1.),
            Vec3::new(0., 1., -1.),
        );
        let mut hidden = back;
        hidden.a[2] = -2.;
        let triangles = [front, back, hidden];
        let origin = Vec3::new(0., 0., 1000.);
        let depth = |t: f32| t / 2000.;
        assert_eq!(
            reference_depth(origin, -Vec3::Z, &triangles, depth, 0.5).unwrap(),
            (0.5, true)
        );
        let hidden = reference_depth(origin, -Vec3::Z, &triangles, depth, 0.501)
            .unwrap()
            .0;
        assert!((hidden - 0.501).abs() > 0.00002);
        let behind_front = reference_depth(
            Vec3::new(0.25, -0.5, 1000.),
            -Vec3::Z,
            &triangles,
            depth,
            0.5005,
        )
        .unwrap()
        .0;
        assert!((behind_front - 0.5005).abs() > 0.00002);
    }
}
