//! Event-only GPU-to-physics handoff. No second CPU wind solver and no per-frame
//! readback: detach from the last rendered held pose and its corresponding velocity.
use super::*;
use glam::{UVec3, Vec2};
use re_flora_vkn::{execute_one_time_command, BufferUse, VulkanContext};

impl VegetationResponse {
    pub fn fruit_handoff(
        &self,
        context: &VulkanContext,
        allocator: Allocator,
        roots: &[UVec3],
    ) -> Result<Vec<[f32; 4]>> {
        let zero = || vec![[0.; 4]; roots.len()];
        if roots.is_empty()
            || !self.enabled
            || self.comparison == "legacy"
            || self.grid.grid[3] < 2.
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
                source.record_copy_to_buffer(cmd, &readback, bytes, bytes, 0);
                cmd.use_buffer(&readback, BufferUse::HostRead);
                Ok(())
            },
        )?;
        let bytes = readback.read_back_range(0, bytes)?;
        let states: &[[f32; STATE_FLOATS]] = bytemuck::try_cast_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("fruit response readback: {e}"))?;
        let result = roots
            .iter()
            .map(|&root| sample_held_field(self.grid, states, root))
            .collect();
        log::info!("[VEGETATION_RESPONSE][FRUIT_HANDOFF] count={} readback_bytes={} elapsed_us={} source=last_published_gpu_pose", roots.len(), bytes.len(), started.elapsed().as_micros());
        Ok(result)
    }
}

fn sample_held_field(info: ResponseInfo, states: &[[f32; STATE_FLOATS]], root: UVec3) -> [f32; 4] {
    let seed = instance_seed(root);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fruit_handoff_samples_all_four_corners() {
        let info = ResponseInfo {
            grid: [0., 0., 1., 3.],
            shape: [2, 2, 1, 12],
        };
        let states = (0..4)
            .map(|index| [index as f32; STATE_FLOATS])
            .collect::<Vec<_>>();
        assert_eq!(
            sample_held_field(info, &states, UVec3::new(128, 37, 128)),
            [1.5; 4]
        );
        assert_eq!(
            sample_held_field(info, &states, UVec3::new(256, 500, 256)),
            [3.; 4]
        );
    }
}
