//! Explicit real-GPU diagnostic, called only after the previous frame completed.
//! Reads production tiles and checks actual hit depths against CPU triangle rays.
use super::*;
use glam::{Mat4, Vec2, Vec4};
use re_flora_vkn::{execute_one_time_command, BufferUse, VulkanContext};

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

fn intersect(origin: Vec3, direction: Vec3, triangle: &Triangle) -> Option<f32> {
    let a = Vec3::from_slice(&triangle.a);
    let e1 = Vec3::from_slice(&triangle.e1);
    let e2 = Vec3::from_slice(&triangle.e2);
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = origin - a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = direction.dot(q) / det;
    let t = e2.dot(q) / det;
    (u >= -1e-6 && v >= -1e-6 && u + v <= 1.000001 && t > 0.).then_some(t)
}

impl ButterflyMeshRenderer {
    pub fn validate_completed_tiles(
        &mut self,
        context: &VulkanContext,
        allocator: Allocator,
        resources: &crate::tracer::resources::TracerResources,
    ) -> Result<()> {
        let leaf_review = std::env::var_os("RE_FLORA_LEAF_MODEL_REVIEW").is_some()
            && self.previous_leaf_mode.is_some_and(|(enabled, ..)| enabled);
        if (!leaf_review && std::env::var_os("RE_FLORA_BUTTERFLY_MESH_REVIEW").is_none())
            || self.compute_count == 0
            || (self.previous_mode == self.validated_mode
                && (!leaf_review || self.previous_leaf_mode == self.validated_leaf_mode))
        {
            return Ok(());
        }
        let mode = self.previous_mode.unwrap();
        let byte_count = u64::from(self.compute_count) * 4096 * 16;
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
            let mut count = 0;
            for y in 0..n {
                for x in 0..n {
                    let pixel = pixels[index * 4096 + y as usize * 64 + x as usize];
                    ensure!(pixel.iter().all(|v| v.is_finite()), "nonfinite tile texel");
                    ensure!((0.0..=1.0).contains(&pixel[3]), "invalid tile depth");
                    if pixel[3] < 1.0 {
                        count += 1;
                        // All hits, not just one convenient pixel, exercise the production
                        // instance index, framing, ray direction and nearest-triangle depth.
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
                            let inverse = Quat::from_array(instance.lighting).conjugate();
                            let scale = instance.position_size[3] * (1.53125 / 3.4);
                            (
                                inverse * (origin - Vec3::from_slice(&instance.position_size))
                                    / scale,
                                inverse * direction,
                                scale,
                            )
                        } else {
                            (origin, direction, 1.)
                        };
                        let distance = self.triangles[first..end]
                            .iter()
                            .filter_map(|t| intersect(ray_origin, ray_direction, t))
                            .min_by(f32::total_cmp)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "GPU hit absent on CPU: instance={index} pixel={x},{y}"
                                )
                            })?;
                        let clip = vp * (origin + direction * distance * scale).extend(1.);
                        let error = (pixel[3] - clip.z / clip.w).abs();
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
            hits.push(count);
            image.save(directory.join(format!(
                "{n}px-{}fps-shadow{}-trans{}-{index:02}.png",
                mode.1,
                u32::from(mode.2),
                (f32::from_bits(mode.3) * 100.).round() as u32
            )))?;
        }
        ensure!(
            checked > 0,
            "fixture dispatched but generated no visible mesh samples"
        );
        log::info!("[BUTTERFLY-MESH-CHECK] tile={n}x{n} active={} checked_hits={checked} max_depth_error={max_depth_error:.9} hits={hits:?}",self.count());
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
    fn projection_bounds_remain_finite_across_near_plane_and_eye() {
        let projection = Mat4::perspective_rh(0.8, 1.6, 0.001, 10.);
        for z in [-2., -0.1, -0.03, -0.024, -0.023, -0.02, -0.001, 0., 0.02] {
            let instance = Instance {
                position_size: [0., 0., z, 0.03],
                ..Instance::zeroed()
            };
            let rect = bounds(&instance, Mat4::IDENTITY, projection);
            assert!(rect.is_finite());
            assert!(rect.z > rect.x && rect.w > rect.y);
        }
    }
}
