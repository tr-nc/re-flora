//! Explicit real-GPU diagnostic, called only after the previous frame completed.
//! Reads production tiles and checks actual hit depths against CPU triangle rays.
use super::*;
use glam::{Mat4, Vec2, Vec4};
use re_flora_vkn::{execute_one_time_command, BufferUse, VulkanContext};

/// Explicit native regression: the published variant/pose that exposed the
/// conservative boundary disagreements. No simulation or camera overrides.
/// Kept here with the oracle, never enabled by normal play or the live A/B runner.
pub(super) fn apply_coverage_fixture(
    instances: &mut Vec<Instance>,
    leaf_first: u32,
    triangles_per_leaf: usize,
) -> Result<()> {
    let Ok(fixture) = std::env::var("RE_FLORA_MODEL_COVERAGE_FIXTURE") else {
        return Ok(());
    };
    ensure!(
        matches!(
            fixture.as_str(),
            "leaf-boundary" | "leaf-small-boundary" | "leaf-pose-sweep"
        ) && std::env::var("RE_FLORA_LEAF_MODEL_REVIEW").as_deref() == Ok("b"),
        "model coverage fixture requires a known leaf boundary and RE_FLORA_LEAF_MODEL_REVIEW=b"
    );
    let Some(instance) = instances
        .iter()
        .find(|i| i.metadata[3] & LEAF_MODEL_FLAG != 0)
        .copied()
    else {
        anyhow::bail!("model coverage fixture requires a published leaf");
    };
    instances.clear();
    if fixture == "leaf-pose-sweep" {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let cases = coverage_pose_cases(instance, leaf_first, triangles_per_leaf);
        let batches = cases.len().div_ceil(64);
        let requested = NEXT.fetch_add(1, Ordering::Relaxed);
        let batch = requested.min(batches - 1);
        instances.extend_from_slice(&cases[batch * 64..((batch + 1) * 64).min(cases.len())]);
        if requested < batches {
            log::info!(
                "[MODEL-COVERAGE-SWEEP] batch={}/{} cases={} saved_config_unchanged=true",
                batch + 1,
                batches,
                cases.len()
            );
        }
    } else {
        instances.push(captured_coverage_instance(
            instance,
            leaf_first,
            triangles_per_leaf,
            fixture == "leaf-small-boundary",
        ));
    }
    Ok(())
}

fn captured_coverage_instance(
    mut instance: Instance,
    leaf_first: u32,
    triangles_per_leaf: usize,
    small: bool,
) -> Instance {
    instance.position_size = [0.8934032, 1.5581794, 1.4821042, 0.00390625];
    instance.lighting = [0.64829177, 0.3265856, 0.45352986, -0.51707864];
    instance.view_orientation = instance.lighting;
    instance.metadata = [
        leaf_first + 18 * triangles_per_leaf as u32,
        triangles_per_leaf as u32,
        64,
        LEAF_MODEL_FLAG,
    ];
    if small {
        // Captured variant 0 at the live runner's 0.25 size, not a scaled
        // substitute for the 64px case. Pixel (5,10) exposes a distinct context.
        instance.position_size = [1.1692841, 1.3915664, 1.4927582, 0.0009765625];
        instance.lighting = [-0.209657, -0.660767, 0.047330074, -0.7191597];
        instance.view_orientation = instance.lighting;
        instance.metadata = [leaf_first, triangles_per_leaf as u32, 16, LEAF_MODEL_FLAG];
    }
    instance
}

/// Explicit diagnostic-only, bounded, unscreened phases plus representable
/// neighbors of both captures. Runtime time/load cannot select favorable poses.
fn coverage_pose_cases(template: Instance, first: u32, count: usize) -> Vec<Instance> {
    let mut cases = Vec::new();
    for small in [false, true] {
        let base = captured_coverage_instance(template, first, count, small);
        for axis in 0..3 {
            for ulps in [-16i32, -4, -1, 0, 1, 4, 16] {
                let mut instance = base;
                instance.position_size[axis] = f32::from_bits(
                    (base.position_size[axis].to_bits() as i64 + i64::from(ulps)) as u32,
                );
                cases.push(instance);
            }
        }
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            for phase in 0..16 {
                let rotation =
                    glam::Quat::from_axis_angle(axis, phase as f32 * std::f32::consts::TAU / 16.);
                let pose = (rotation * glam::Quat::from_array(base.lighting))
                    .normalize()
                    .to_array();
                for resolution in [8, 16, 64] {
                    for scale in [0.25, 1., 4.] {
                        let mut instance = base;
                        instance.lighting = pose;
                        instance.view_orientation = pose;
                        instance.metadata[2] = resolution;
                        instance.position_size[3] = 0.00390625 * scale;
                        cases.push(instance);
                    }
                }
            }
        }
    }
    cases
}

fn leaf_review_can_be_empty(
    leaf_review: bool,
    checked: usize,
    original: usize,
    planned: usize,
) -> bool {
    leaf_review && checked == 0 && original == 0 && planned == 0
}

fn bounds(instance: &Instance, view: Mat4, projection: Mat4) -> Vec4 {
    let center = view.transform_point3(Vec3::from_slice(&instance.position_size));
    let r = instance.position_size[3] * (1.53125 * 0.5);
    let near = projection.inverse() * Vec4::new(0., 0., 0., 1.);
    let near_z = near.z / near.w;
    if center.z - r >= near_z {
        return Vec4::new(2., 2., 3., 3.);
    }
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let p = Vec4::new(
            center.x + if i & 1 == 0 { -r } else { r },
            center.y + if i & 2 == 0 { -r } else { r },
            if i & 4 == 0 {
                center.z - r
            } else {
                (center.z + r).min(near_z)
            },
            1.,
        );
        let clip = projection * p;
        let ndc = Vec2::new(clip.x, clip.y) / clip.w;
        lo = lo.min(ndc);
        hi = hi.max(ndc);
    }
    Vec4::new(lo.x, lo.y, hi.x, hi.y)
}

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
    const _: () = assert!(1 + MAX_TRIANGLES * 13 <= 4096);
    ensure!(
        count <= MAX_TRIANGLES && evidence.len() >= 1 + count * 13,
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
                    let error = projection_roundoff(cpu_ndc, vp).truncate();
                    ensure!(
                        (cpu_ndc.truncate() - gpu_ndc).abs().cmple(error).all(),
                        "GPU projected vertex/frame mismatch"
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

impl ButterflyMeshRenderer {
    pub fn validate_completed_tiles(
        &mut self,
        context: &VulkanContext,
        allocator: Allocator,
        resources: &crate::tracer::resources::TracerResources,
    ) -> Result<()> {
        self.validation_calls = self.validation_calls.wrapping_add(1);
        let leaf_review = std::env::var_os("RE_FLORA_LEAF_MODEL_REVIEW").is_some()
            && self.previous_leaf_mode.is_some_and(|(enabled, ..)| enabled);
        if (!leaf_review && std::env::var_os("RE_FLORA_BUTTERFLY_MESH_REVIEW").is_none())
            || self.compute_count == 0
            || (self.previous_mode == self.validated_mode
                && (!leaf_review || self.previous_leaf_mode == self.validated_leaf_mode)
                && !self.validation_calls.is_multiple_of(16)
                && std::env::var("RE_FLORA_MODEL_COVERAGE_FIXTURE").as_deref()
                    != Ok("leaf-pose-sweep"))
        {
            return Ok(());
        }
        let mode_changed = self.previous_mode != self.validated_mode
            || (leaf_review && self.previous_leaf_mode != self.validated_leaf_mode);
        let mode = self.previous_mode.unwrap();
        let reference_base = self
            .reference_tile_offset()
            .expect("native review reference tiles");
        let byte_count = u64::from(reference_base * 4 + self.compute_count) * 4096 * 16;
        let readback = Buffer::new_sized(
            context.device().clone(),
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            byte_count,
        );
        execute_one_time_command(
            context.device(),
            context.command_pool(),
            &context.get_general_queue(),
            |cmd| -> Result<()> {
                resources
                    .butterfly_mesh
                    .butterfly_pixel_tiles
                    .record_copy_to_buffer(cmd, &readback, byte_count, 0, 0);
                cmd.use_buffer(&readback, BufferUse::HostRead);
                Ok(())
            },
        )?;
        let bytes = readback.read_back()?;
        let pixels: &[[f32; 4]] =
            bytemuck::try_cast_slice(&bytes).map_err(|e| anyhow::anyhow!("tile ABI: {e}"))?;
        let camera_bytes = resources.uniforms.camera_info.read_back()?;
        let camera: crate::generated::gpu_structs::CameraInfo =
            bytemuck::pod_read_unaligned(&camera_bytes);
        let view = Mat4::from_cols_array_2d(&camera.view_mat);
        let projection = Mat4::from_cols_array_2d(&camera.proj_mat);
        let vp = projection * view;
        let inverse = vp.inverse();
        if mode_changed && std::env::var_os("RE_FLORA_MODEL_COVERAGE_FIXTURE").is_some() {
            let output = std::env::var("RE_FLORA_MODEL_COVERAGE_OUTPUT")
                .unwrap_or_else(|_| "target/improve-delivery/v3/coverage-boundary".into());
            let directory = std::path::Path::new(&output);
            std::fs::create_dir_all(directory)?;
            for (name, tile) in [("final", 0), ("center", reference_base as usize)] {
                std::fs::write(
                    directory.join(format!("{name}.bin")),
                    bytemuck::cast_slice(&pixels[tile * 4096..(tile + 1) * 4096]),
                )?;
            }
        }
        let n = self.resolution;
        let directory = std::path::Path::new(if leaf_review {
            "target/leaf-model-review/tiles"
        } else {
            "target/butterfly-resume/game-tiles"
        });
        std::fs::create_dir_all(directory)?;
        let mut hits = Vec::new();
        let mut checked = 0;
        let mut checked_leaf = 0;
        let mut roundoff_boundary_hits = 0;
        let mut original_samples = 0;
        let mut repaired_samples = 0;
        let mut components_before = 0;
        let mut components_after = 0;
        let mut max_depth_error = 0f32;
        for (index, instance) in self
            .instances
            .iter()
            .take(self.compute_count as usize)
            .enumerate()
        {
            let n = instance.metadata[2];
            let leaf = instance.metadata[3] & LEAF_MODEL_FLAG != 0;
            let rect = bounds(instance, view, projection);
            let mut image = image::RgbaImage::new(n, n);
            // Build the independent CPU coverage plan from the exact GPU center
            // ownership. Its rays are validated below; reclassifying centers with
            // differently rounded CPU unprojection spuriously changes ownership
            // on tiny leaf silhouettes.
            let mesh_first = instance.metadata[0] as usize;
            let mesh_end = mesh_first + instance.metadata[1] as usize;
            let axes = model_pose_axes(instance.lighting);
            let scale = instance.position_size[3] * (1.53125 / 3.4);
            let triangles: Vec<_> = self.triangles[mesh_first..mesh_end]
                .iter()
                .map(|t| {
                    let a = Vec3::from_slice(&t.a);
                    let points = [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)];
                    (
                        if leaf || t.e1[3] < 0. { 1 } else { 2 },
                        if leaf {
                            points.map(|p| {
                                Vec3::from_slice(&instance.position_size)
                                    + scale * (axes[0] * p.x + axes[1] * p.y + axes[2] * p.z)
                            })
                        } else {
                            points
                        },
                    )
                })
                .collect();
            let (_, cpu_groups) =
                model_pixel_repair::project(&triangles, vp, rect, n as usize, |_, _, _| None);
            let projection_at = (reference_base as usize * 4 + index) * 4096;
            let center_clip = vp * Vec3::from_slice(&instance.position_size).extend(1.);
            let groups = checked_projected_groups(
                &cpu_groups,
                &pixels[projection_at..projection_at + 4096],
                mesh_first,
                mesh_end - mesh_first,
                rect,
                vp,
                center_clip.z / center_clip.w,
                n,
            )?;
            let mut owners = vec![0; n as usize * n as usize];
            for y in 0..n {
                for x in 0..n {
                    let at = index * 4096 + y as usize * 64 + x as usize;
                    if pixels[reference_base as usize * 4096 + at][3] < 1. {
                        let tri =
                            pixels[reference_base as usize * 2 * 4096 + at][3].to_bits() as usize;
                        ensure!(
                            (mesh_first..mesh_end).contains(&tri),
                            "GPU reference triangle outside model range"
                        );
                        owners[(y * n + x) as usize] = triangles[tri - mesh_first].0;
                    }
                }
            }
            let oracle = model_pixel_repair::plan(&owners, &groups, n as usize);
            // Reconstruct the sparse color expressions independently from this
            // frame's GPU center-only references, including hidden parent nodes.
            let mut expressions: Vec<[f32; 4]> = Vec::new();
            let mut expected_repairs = vec![None; (n * n) as usize];
            let mut seed_depths = vec![None; (n * n) as usize];
            for node in &oracle.nodes {
                let endpoint = |r: u32| {
                    if r & model_pixel_repair::EXPRESSION != 0 {
                        let mut value = expressions[(r & !model_pixel_repair::EXPRESSION) as usize];
                        value[3] = if value[3] > 1. { 1. } else { 0. };
                        value
                    } else {
                        pixels[(reference_base as usize + index) * 4096
                            + (r / n) as usize * 64
                            + (r % n) as usize]
                    }
                };
                if node.links[1] == model_pixel_repair::HIDDEN {
                    // Coverage nodes shade their source triangle directly; their
                    // output is not an endpoint expression.
                    expressions.push(node.color_depth);
                    if node.links[0] != model_pixel_repair::HIDDEN {
                        seed_depths[node.links[0] as usize] = Some(node.color_depth[3]);
                    }
                    continue;
                }
                let a = endpoint(node.links[1]);
                let b = endpoint(node.links[2]);
                let t = f32::from_bits(node.links[3]);
                let mut value = node.color_depth;
                for k in 0..3 {
                    value[k] = a[k] + (b[k] - a[k]) * t;
                }
                if a[3] >= 1. || b[3] >= 1. {
                    value[3] = 2.;
                }
                expressions.push(value);
                if node.links[0] != model_pixel_repair::HIDDEN {
                    expected_repairs[node.links[0] as usize] = Some(value);
                }
            }
            let mut count = 0;
            let mut original_mask = vec![false; (n * n) as usize];
            let mut final_mask = original_mask.clone();
            for y in 0..n {
                for x in 0..n {
                    let pixel = pixels[index * 4096 + y as usize * 64 + x as usize];
                    ensure!(pixel.iter().all(|v| v.is_finite()), "nonfinite tile texel");
                    ensure!((0.0..=1.0).contains(&pixel[3]), "invalid tile depth");
                    let original = pixels
                        [(reference_base as usize + index) * 4096 + y as usize * 64 + x as usize];
                    ensure!(
                        original.iter().all(|v| v.is_finite()),
                        "nonfinite reference texel"
                    );
                    original_mask[(y * n + x) as usize] = original[3] < 1.;
                    final_mask[(y * n + x) as usize] = pixel[3] < 1.;
                    if original[3] < 1. {
                        original_samples += 1;
                        ensure!(
                            original.map(f32::to_bits) == pixel.map(f32::to_bits),
                            "repair changed existing RGBA/depth: instance={index} pixel={x},{y} original={original:?} final={pixel:?}"
                        );
                    }
                    if original[3] >= 1. {
                        if seed_depths[(y * n + x) as usize].is_some() {
                            ensure!(
                                pixel[3] < 1.,
                                "GPU omitted conservative coverage: instance={index} pixel={x},{y}"
                            );
                        }
                        if let Some(expected) =
                            expected_repairs[(y * n + x) as usize].filter(|v| v[3] < 1.)
                        {
                            ensure!(
                                pixel[3] < 1.,
                                "GPU omitted a valid repair: instance={index} pixel={x},{y}"
                            );
                            for k in 0..3 {
                                ensure!((expected[k]-pixel[k]).abs()<=1e-5*expected[k].abs().max(1.),"repair endpoint color mismatch: instance={index} pixel={x},{y}");
                            }
                        }
                        if pixel[3] < 1. {
                            repaired_samples += 1;
                        }
                    }
                    if pixel[3] < 1.0 {
                        count += 1;
                        // All hits, not just one convenient pixel, exercise the production
                        // instance index, framing, ray direction and nearest-triangle depth.
                        let coverage_seed = seed_depths[(y * n + x) as usize];
                        let uv = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32;
                        let ndc = Vec2::new(rect.x, rect.y)
                            + Vec2::new(rect.z - rect.x, rect.w - rect.y) * uv;
                        let near = inverse * Vec4::new(ndc.x, ndc.y, 0., 1.);
                        let far = inverse * Vec4::new(ndc.x, ndc.y, 1., 1.);
                        let origin = near.truncate() / near.w;
                        let direction = (far.truncate() / far.w - origin).normalize();
                        let first = instance.metadata[0] as usize;
                        let end = first + instance.metadata[1] as usize;
                        let (ray_origin, ray_direction, scale) = if leaf {
                            let axes = model_pose_axes(instance.lighting);
                            let scale = instance.position_size[3] * (1.53125 / 3.4);
                            (
                                model_local_vector(
                                    axes,
                                    origin - Vec3::from_slice(&instance.position_size),
                                ) / scale,
                                model_local_vector(axes, direction),
                                scale,
                            )
                        } else {
                            (origin, direction, 1.)
                        };
                        let expected_depth = if original[3] < 1. {
                            let at = index * 4096 + y as usize * 64 + x as usize;
                            let gpu_origin =
                                Vec3::from_slice(&pixels[reference_base as usize * 2 * 4096 + at]);
                            let gpu_direction =
                                Vec3::from_slice(&pixels[reference_base as usize * 3 * 4096 + at]);
                            let origin_error = 128.
                                * f32::EPSILON
                                * (origin.length()
                                    + Vec3::from_slice(&instance.position_size).length())
                                .max(1.)
                                / scale;
                            ensure!(
                                gpu_origin.distance(ray_origin) <= origin_error,
                                "GPU reference ray origin/frame mismatch"
                            );
                            ensure!(
                                gpu_direction.distance(ray_direction) <= 128. * f32::EPSILON,
                                "GPU reference ray direction/frame mismatch"
                            );
                            let depth = |distance: f32| {
                                let p = gpu_origin + gpu_direction * distance;
                                let p = if leaf {
                                    Vec3::from_slice(&instance.position_size)
                                        + scale * (axes[0] * p.x + axes[1] * p.y + axes[2] * p.z)
                                } else {
                                    p
                                };
                                let clip = vp * p.extend(1.);
                                clip.z / clip.w
                            };
                            let (expected,boundary)=reference_depth(gpu_origin,gpu_direction,&self.triangles[first..end],depth,pixel[3])
                                .ok_or_else(||anyhow::anyhow!("GPU center hit outside CPU precision envelope: instance={index} pixel={x},{y}"))?;
                            roundoff_boundary_hits += usize::from(boundary);
                            expected
                        } else if let Some(depth) = coverage_seed {
                            depth
                        } else {
                            oracle.nodes.iter().find(|node|node.links[0]==y*n+x)
                                .ok_or_else(||anyhow::anyhow!("GPU addition absent in geometry plan: instance={index} pixel={x},{y}"))?.color_depth[3]
                        };
                        let error = (pixel[3] - expected_depth).abs();
                        max_depth_error = max_depth_error.max(error);
                        if error >= 0.00002 {
                            let at = index * 4096 + y as usize * 64 + x as usize;
                            let capture = serde_json::json!({"pixel":[x,y],"resolution":n,"original":original,"final":pixel,
                                "expected_depth":expected_depth,"bounds":rect.to_array(),"view_projection":vp.to_cols_array(),
                                "position_size":instance.position_size,"orientation":instance.lighting,"leaf":leaf,
                                "gpu_ray_origin":pixels[reference_base as usize*2*4096+at],
                                "gpu_ray_direction":pixels[reference_base as usize*3*4096+at],
                                "triangles":self.triangles[first..end].iter().map(|t|(t.a,t.e1,t.e2)).collect::<Vec<_>>()});
                            std::fs::write(
                                "target/model-depth-mismatch.json",
                                serde_json::to_vec_pretty(&capture)?,
                            )?;
                        }
                        ensure!(error < 0.00002, "GPU/CPU depth mismatch {error}: instance={index} pixel={x},{y} center={} gpu={} cpu={expected_depth}; capture=target/model-depth-mismatch.json",original[3]<1.,pixel[3]);
                        checked += 1;
                        checked_leaf += usize::from(leaf);
                        // Diagnostic only: unclipped HDR stays in GPU storage; the small
                        // PNG uses clamped linear RGB, not the game's display transform.
                        let rgb = [0, 1, 2].map(|i| (pixel[i].clamp(0., 1.) * 255.).round() as u8);
                        image.put_pixel(x, y, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
                    }
                }
            }
            let before = model_pixel_repair::label(&original_mask, n as usize).1;
            let after = model_pixel_repair::label(&final_mask, n as usize).1;
            // Conservatively recovered features can be disconnected from the
            // center-sampled mask; more visible components are not a failure.
            components_before += before;
            components_after += after;
            hits.push(count);
            if mode_changed {
                image.save(directory.join(format!(
                    "{n}px-{}fps-shadow{}-trans{}-{index:02}.png",
                    mode.1,
                    u32::from(mode.2),
                    (f32::from_bits(mode.3) * 100.).round() as u32
                )))?;
            }
        }
        // Rotating/falling fixtures eventually leave the camera. Their empty
        // frames are valid, after the mode's required visible sample passed.
        if checked == 0 && !mode_changed {
            return Ok(());
        }
        // The falling fixture moves through the camera. A mode can switch after
        // all leaves have exited view: no original hits and no planned coverage
        // means there is nothing to validate this frame, not a renderer failure.
        if leaf_review_can_be_empty(leaf_review, checked, original_samples, self.repair_added) {
            self.validated_mode = self.previous_mode;
            self.validated_leaf_mode = self.previous_leaf_mode;
            return Ok(());
        }
        ensure!(
            checked > 0,
            "fixture dispatched but generated no visible mesh samples"
        );
        log::info!("[BUTTERFLY-MESH-CHECK] tile={n}x{n} active={} checked_hits={checked} max_depth_error={max_depth_error:.9} hits={hits:?}",self.count());
        log::info!("[MODEL-REPAIR-CHECK] original_samples={original_samples} original_changed=0 added={repaired_samples} planned={} components={components_before}->{components_after} group_components={}->{} nodes={} roundoff_boundary_hits={roundoff_boundary_hits}",self.repair_added,self.repair_before,self.repair_after,self.repair_nodes.len());
        if leaf_review {
            ensure!(
                checked_leaf > 0,
                "leaf fixture produced no checked leaf samples"
            );
            // The frozen regression overrides the published instance, not the
            // saved setting. Report the resolution actually sent to the GPU.
            let resolution = self
                .instances
                .iter()
                .find(|instance| instance.metadata[3] & LEAF_MODEL_FLAG != 0)
                .expect("checked leaf instance")
                .metadata[2];
            log::info!("[LEAF-MODEL-CHECK] mode=B resolution={resolution} active={} checked_hits={checked_leaf} max_depth_error={max_depth_error:.9} pose=published_quaternion", self.count() - self.tile_count);
        }
        self.validated_mode = self.previous_mode;
        self.validated_leaf_mode = self.previous_leaf_mode;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_pose_sweep_is_bounded_and_keeps_both_captured_inputs() {
        let template = Instance::zeroed();
        let cases = coverage_pose_cases(template, 0, 32);
        assert_eq!(cases.len(), 906);
        for small in [false, true] {
            let captured = captured_coverage_instance(template, 0, 32, small);
            assert!(cases
                .iter()
                .any(|i| bytemuck::bytes_of(i) == bytemuck::bytes_of(&captured)));
        }
        for instance in cases {
            assert!(glam::Quat::from_array(instance.lighting).is_normalized());
            assert!([8, 16, 64].contains(&instance.metadata[2]));
            assert!([0.0009765625, 0.00390625, 0.015625].contains(&instance.position_size[3]));
        }
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
    fn leaf_review_only_accepts_empty_visibility_without_original_or_planned_samples() {
        assert!(leaf_review_can_be_empty(true, 0, 0, 0));
        for (leaf, checked, original, planned) in [
            (false, 0, 0, 0),
            (true, 1, 0, 0),
            (true, 0, 1, 0),
            (true, 0, 0, 1),
        ] {
            assert!(!leaf_review_can_be_empty(leaf, checked, original, planned));
        }
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
        let depth = |t| t / 2000.;
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
    #[test]
    fn projection_bounds_remain_finite_across_near_plane_and_eye() {
        let projection = Mat4::perspective_rh(0.8, 1.6, 0.001, 10.);
        for z in [-2., -0.1, -0.03, -0.024, -0.023, -0.02, -0.001, 0., 0.02] {
            let instance = Instance {
                position_size: [0., 0., z, 0.03],
                ..Instance::zeroed()
            };
            let rect = bounds(&instance, Mat4::IDENTITY, projection);
            assert_eq!(
                rect,
                model_pixel_repair::tile_bounds(
                    Vec3::from_slice(&instance.position_size),
                    instance.position_size[3],
                    Mat4::IDENTITY,
                    projection
                )
            );
            assert!(rect.is_finite());
            assert!(rect.z > rect.x && rect.w > rect.y);
        }
    }
}
