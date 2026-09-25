//! Explicit real-GPU diagnostic, called only after the previous frame completed.
//! Reads production tiles and checks actual hit depths against CPU triangle rays.
use super::*;
use glam::{Mat4, Vec2, Vec4};
use re_flora_vkn::{execute_one_time_command, BufferUse, VulkanContext};

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
                && !self.validation_calls.is_multiple_of(16))
        {
            return Ok(());
        }
        let mode_changed = self.previous_mode != self.validated_mode
            || (leaf_review && self.previous_leaf_mode != self.validated_leaf_mode);
        let mode = self.previous_mode.unwrap();
        let reference_base = self
            .reference_tile_offset()
            .expect("native review reference tiles");
        let byte_count = u64::from(reference_base * 3 + self.compute_count) * 4096 * 16;
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
            let (_, groups) =
                model_pixel_repair::project(&triangles, vp, rect, n as usize, |_, _, _| None);
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
                            "repair changed existing RGBA/depth: instance={index} pixel={x},{y}"
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
                        ensure!(error < 0.00002, "GPU/CPU depth mismatch {error}");
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
            log::info!("[LEAF-MODEL-CHECK] mode=B resolution={} active={} checked_hits={checked_leaf} max_depth_error={max_depth_error:.9} pose=published_quaternion", self.previous_leaf_mode.unwrap().1, self.count() - self.tile_count);
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
