//! Event-only GPU-to-physics handoff. No second CPU wind solver and no per-frame
//! readback: detach from the last rendered held pose and its corresponding velocity.
use super::*;
use glam::{UVec3, Vec2, Vec3};
use re_flora_vkn::{execute_one_time_command, BufferUse, VulkanContext};

impl VegetationResponse {
    pub fn fruit_handoff(
        &self,
        context: &VulkanContext,
        allocator: Allocator,
        gui: &crate::generated::gpu_structs::GuiInput,
        roots: &[UVec3],
    ) -> Result<Vec<(Vec3, Vec3)>> {
        let zero = || vec![(Vec3::ZERO, Vec3::ZERO); roots.len()];
        if roots.is_empty()
            || !self.enabled
            || self.comparison == "legacy"
            || self.grid.grid[3] < 3.
            || self.previous_output.is_none()
        {
            return Ok(zero());
        }
        let count = (self.grid.shape[0] * self.grid.shape[1]) as u64;
        let bytes = count * STATE_BYTES;
        let readback = Buffer::new_sized(
            context.device().clone(),
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            bytes,
        );
        let source = self.previous_output.as_ref().unwrap();
        let started = std::time::Instant::now();
        execute_one_time_command(
            context.device(),
            context.command_pool(),
            &context.get_general_queue(),
            |cmd| -> Result<()> {
                source.record_copy_to_buffer(cmd, &readback, bytes, bytes * 2, 0);
                cmd.use_buffer(&readback, BufferUse::HostRead);
                Ok(())
            },
        )?;
        let bytes = readback.read_back_range(0, bytes)?;
        let states: &[[f32; 20]] = bytemuck::try_cast_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("fruit response readback: {e}"))?;
        let result = roots
            .iter()
            .map(|&root| {
                let qv = sample_held_field(self.grid, states, root);
                let offset = |q: Vec2| {
                    fruit_offset(
                        q,
                        gui.fruit_swing_length_voxels,
                        gui.fruit_swing_max_angle_radians,
                    )
                };
                let q = Vec2::new(qv[0], qv[1]);
                let velocity = Vec2::new(qv[2], qv[3]);
                // Differentiate only the pendulum geometry, never reintegrate motion.
                let dt = 0.001;
                (
                    offset(q),
                    (offset(q + velocity * dt) - offset(q - velocity * dt)) / (2. * dt),
                )
            })
            .collect();
        log::info!("[VEGETATION_RESPONSE][FRUIT_HANDOFF] count={} readback_bytes={} elapsed_us={} source=last_published_gpu_pose", roots.len(), bytes.len(), started.elapsed().as_micros());
        Ok(result)
    }
}

fn sample_held_field(info: ResponseInfo, states: &[[f32; 20]], root: UVec3) -> [f32; 4] {
    let mut seed = root.x ^ (root.y << 10) ^ (root.z << 20);
    seed ^= seed >> 16;
    seed ^= seed << 5;
    seed ^= seed >> 11;
    let bucket_offset = 4 + (seed % 4) as usize * 4;
    let width = info.shape[0] as usize;
    let depth = info.shape[1] as usize;
    let cell = ((Vec2::new(root.x as f32, root.z as f32) / 256.
        - Vec2::new(info.grid[0], info.grid[1]))
        / info.grid[2])
        .clamp(
            Vec2::ZERO,
            Vec2::new((width - 1) as f32, (depth - 1) as f32),
        );
    let lo = cell.floor().as_uvec2();
    let hi = (lo + glam::UVec2::ONE).min(glam::UVec2::new(width as u32 - 1, depth as u32 - 1));
    let f = cell - cell.floor();
    std::array::from_fn(|axis| {
        let at = |x: u32, z: u32| states[x as usize + z as usize * width][bucket_offset + axis];
        let low = at(lo.x, lo.y) * (1. - f.x) + at(hi.x, lo.y) * f.x;
        let high = at(lo.x, hi.y) * (1. - f.x) + at(hi.x, hi.y) * f.x;
        low * (1. - f.y) + high * f.y
    })
}

fn fruit_offset(q: Vec2, length: f32, max_angle: f32) -> Vec3 {
    let magnitude = q.length();
    let angle = magnitude / (1. + magnitude) * max_angle.max(0.);
    let direction = if magnitude > 0.000001 {
        q / magnitude
    } else {
        Vec2::ZERO
    };
    Vec3::new(
        direction.x * angle.sin(),
        1. - angle.cos(),
        direction.y * angle.sin(),
    ) * length.max(0.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fruit_handoff_samples_all_four_corners_and_preserves_pivot_length() {
        let info = ResponseInfo {
            grid: [0., 0., 1., 3.],
            shape: [2, 2, 1, 12],
        };
        let states = (0..4).map(|index| [index as f32; 20]).collect::<Vec<_>>();
        assert_eq!(
            sample_held_field(info, &states, UVec3::new(128, 37, 128)),
            [1.5; 4]
        );
        assert_eq!(
            sample_held_field(info, &states, UVec3::new(256, 500, 256)),
            [3.; 4]
        );
        assert_eq!(fruit_offset(Vec2::ZERO, 2., 1.), Vec3::ZERO);
        for q in [Vec2::X, Vec2::new(-3., 2.)] {
            assert!(((fruit_offset(q, 2., 1.) - Vec3::Y * 2.).length() - 2.).abs() < 1e-5);
        }
    }
}
