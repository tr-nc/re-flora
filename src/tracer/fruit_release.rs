//! Sparse, frame-owned held-fruit handoff. Four published float4 corners per fruit,
//! not a whole response field and not a dedicated GPU submission/wait.
use super::Tracer;
use crate::flora::apple::{attached_center, attached_rotation, STEM_TIP_VOXELS};
use crate::tree_gen::pose::BranchPose;
use anyhow::{ensure, Result};
use glam::{Quat, UVec3, Vec2, Vec3};
use re_flora_vkn::{
    vk, AcquiredFrame, Buffer, BufferUsage, BufferUse, FrameReadbackProbe, MemoryLocation,
};
use std::collections::VecDeque;

const LIMIT: usize = 256;
const BYTES_PER_FRUIT: u64 = 64;
const STATE_BYTES: u64 = 112;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FruitReleaseTicket(u64);
#[derive(Clone, Copy, Debug)]
pub(crate) struct FruitReleasePose {
    pub offset: Vec3,
    pub velocity: Vec3,
    pub rotation: Quat,
    pub angular_velocity: Vec3,
}
struct Request {
    ticket: FruitReleaseTicket,
    root: UVec3,
    generation: u64,
}
struct Sample {
    request: Request,
    blend: Vec2,
    motion: Option<(BranchPose, BranchPose, f32)>,
    pivot: Vec3,
    scale: f32,
    max_angle: f32,
}
struct Flight {
    samples: Vec<Sample>,
    readback: Option<Buffer>,
    probe: FrameReadbackProbe,
}
#[derive(Default)]
pub(super) struct FruitReleases {
    next_ticket: u64,
    queued: VecDeque<Request>,
    flights: Vec<Option<Flight>>,
    answers: Vec<(FruitReleaseTicket, u64, FruitReleasePose)>,
}

fn corners(grid: [f32; 4], shape: [u32; 4], root: UVec3) -> ([u64; 4], Vec2) {
    let cell = ((Vec2::new(root.x as f32, root.z as f32) / 256. - Vec2::new(grid[0], grid[1]))
        / grid[2])
        .clamp(
            Vec2::ZERO,
            Vec2::new((shape[0] - 1) as f32, (shape[1] - 1) as f32),
        );
    let lo = cell.floor().as_uvec2();
    let hi = (lo + glam::UVec2::ONE).min(glam::UVec2::new(shape[0] - 1, shape[1] - 1));
    let field = u64::from(shape[0]) * u64::from(shape[1]);
    let bucket = u64::from(super::vegetation_response::instance_seed(root) % 4);
    let at = |x: u32, y: u32| {
        (field + u64::from(x) + u64::from(y) * u64::from(shape[0])) * STATE_BYTES + 16 + bucket * 16
    };
    (
        [
            at(lo.x, lo.y),
            at(hi.x, lo.y),
            at(lo.x, hi.y),
            at(hi.x, hi.y),
        ],
        cell - cell.floor(),
    )
}
fn interpolate(q: [[f32; 4]; 4], f: Vec2) -> [f32; 4] {
    std::array::from_fn(|i| {
        let low = q[0][i] * (1. - f.x) + q[1][i] * f.x;
        let high = q[2][i] * (1. - f.x) + q[3][i] * f.x;
        low * (1. - f.y) + high * f.y
    })
}
fn decode(sample: &Sample, q: [f32; 4]) -> Result<FruitReleasePose> {
    held_pose(
        sample.request.root,
        sample.pivot,
        sample.scale,
        q,
        sample.max_angle,
        sample.motion,
    )
}

pub(super) fn held_pose(
    root: UVec3,
    pivot: Vec3,
    scale: f32,
    q: [f32; 4],
    max_angle: f32,
    motion: Option<(BranchPose, BranchPose, f32)>,
) -> Result<FruitReleasePose> {
    ensure!(
        q.iter().all(|v| v.is_finite()),
        "invalid held-fruit response"
    );
    let displacement = Vec2::new(q[0], q[1]);
    let velocity = Vec2::new(q[2], q[3]);
    let rotation = |q| attached_rotation(q, max_angle);
    let offset = |q| attached_center(pivot, scale, rotation(q)) - root.as_vec3();
    let mut pose = FruitReleasePose {
        offset: offset(displacement),
        velocity: (offset(displacement + velocity * 0.001)
            - offset(displacement - velocity * 0.001))
            / 0.002,
        rotation: rotation(displacement),
        angular_velocity: (rotation(displacement + velocity * 0.001)
            * rotation(displacement - velocity * 0.001).conjugate())
        .to_scaled_axis()
            / 0.002,
    };
    if let Some((current, previous, dt)) = motion {
        let rest = (root.as_vec3() + pose.offset) / 256.;
        let world = current.transform_point(rest);
        pose.offset = world * 256. - root.as_vec3();
        pose.velocity = current.rotation * pose.velocity;
        pose.rotation = current.rotation * pose.rotation;
        pose.angular_velocity = current.rotation * pose.angular_velocity;
        if dt > 1e-6 {
            pose.velocity += (world - previous.transform_point(rest)) * 256. / dt;
            pose.angular_velocity +=
                (current.rotation * previous.rotation.conjugate()).to_scaled_axis() / dt;
        }
    }
    ensure!(
        pose.offset.is_finite()
            && pose.velocity.is_finite()
            && pose.rotation.is_finite()
            && pose.angular_velocity.is_finite(),
        "invalid fruit release pose"
    );
    Ok(pose)
}
impl FruitReleases {
    fn collect(&mut self, slot: usize) -> Result<()> {
        let Some(flight) = self.flights.get_mut(slot).and_then(Option::take) else {
            return Ok(());
        };
        let bytes = flight
            .readback
            .as_ref()
            .map(|b| b.read_back_range(0, flight.samples.len() as u64 * BYTES_PER_FRUIT))
            .transpose()?;
        for (index, sample) in flight.samples.iter().enumerate() {
            let q = if let Some(bytes) = &bytes {
                let start = index * BYTES_PER_FRUIT as usize;
                let q: [[f32; 4]; 4] =
                    bytemuck::pod_read_unaligned(&bytes[start..start + BYTES_PER_FRUIT as usize]);
                interpolate(q, sample.blend)
            } else {
                [0.; 4]
            };
            self.answers.push((
                sample.request.ticket,
                sample.request.generation,
                decode(sample, q)?,
            ));
        }
        log::info!("[TREE_KICK][FRUIT_HANDOFF] count={} readback_bytes={} frame_fence=true query_wait=false", flight.samples.len(), bytes.as_ref().map_or(0, Vec::len));
        Ok(())
    }
}
impl Tracer {
    pub(crate) fn request_fruit_release(
        &mut self,
        root: UVec3,
    ) -> Result<Option<FruitReleaseTicket>> {
        let queue = &mut self.fruit_releases;
        let occupied = queue.queued.len()
            + queue.answers.len()
            + queue
                .flights
                .iter()
                .flatten()
                .map(|f| f.samples.len())
                .sum::<usize>();
        if occupied >= LIMIT {
            return Ok(None);
        }
        queue.next_ticket = queue
            .next_ticket
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("fruit release ticket overflow"))?;
        let ticket = FruitReleaseTicket(queue.next_ticket);
        queue.queued.push_back(Request {
            ticket,
            root,
            generation: self.surface_picker.generation(),
        });
        Ok(Some(ticket))
    }
    pub(crate) fn poll_fruit_releases(
        &mut self,
    ) -> Result<Vec<(FruitReleaseTicket, Option<FruitReleasePose>)>> {
        for slot in 0..self.fruit_releases.flights.len() {
            if let Some(flight) = &self.fruit_releases.flights[slot] {
                if flight.probe.is_complete()? {
                    self.fruit_releases.collect(slot)?;
                }
            }
        }
        let generation = self.surface_picker.generation();
        Ok(self
            .fruit_releases
            .answers
            .drain(..)
            .map(|(ticket, source, pose)| (ticket, (source == generation).then_some(pose)))
            .collect())
    }
    pub(super) fn complete_fruit_release_slot(&mut self, slot: usize) -> Result<()> {
        self.fruit_releases.collect(slot)
    }
    pub(super) fn record_frame_fruit_releases(&mut self, frame: &AcquiredFrame) -> Result<()> {
        if self.fruit_releases.queued.is_empty() {
            return Ok(());
        }
        let slot = frame.frame_slot();
        self.fruit_releases
            .flights
            .resize_with(self.fruit_releases.flights.len().max(slot + 1), || None);
        ensure!(
            self.fruit_releases.flights[slot].is_none(),
            "fruit release slot reused before completion"
        );
        let gui_bytes = self.resources.uniforms.gui_input.read_back()?;
        let gui: crate::generated::gpu_structs::GuiInput = bytemuck::pod_read_unaligned(&gui_bytes);
        let response = &self.vegetation_response;
        let source = response.held_fruit_source();
        let readback = source
            .as_ref()
            .map(|_| {
                Buffer::try_new_sized(
                    self.vulkan_ctx.device().clone(),
                    self.allocator.clone(),
                    BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                    MemoryLocation::GpuToCpu,
                    self.fruit_releases.queued.len() as u64 * BYTES_PER_FRUIT,
                )
            })
            .transpose()?;
        let mut samples = Vec::new();
        for (index, request) in self.fruit_releases.queued.drain(..).enumerate() {
            let mut blend = Vec2::ZERO;
            if let Some((buffer, grid, shape)) = &source {
                let (offsets, f) = corners(*grid, *shape, request.root);
                blend = f;
                for (corner, offset) in offsets.into_iter().enumerate() {
                    buffer.record_copy_to_buffer(
                        frame.command_buffer(),
                        readback.as_ref().unwrap(),
                        16,
                        offset,
                        index as u64 * BYTES_PER_FRUIT + corner as u64 * 16,
                    );
                }
            }
            let motion =
                self.raster_trees
                    .gpu_wind_active
                    .then(|| {
                        let index =
                            self.raster_trees.attachments.iter().position(|a| {
                                a.anchor == request.root && a.fruit_pivot.is_some()
                            })?;
                        let current = *self.raster_trees.attachment_poses.get(index)?;
                        Some((
                            current,
                            self.raster_trees
                                .previous_attachment_poses
                                .get(index)
                                .copied()
                                .unwrap_or(current),
                            self.raster_trees.attachment_dt,
                        ))
                    })
                    .flatten();
            let pivot = self
                .raster_trees
                .attachments
                .iter()
                .find(|a| a.anchor == request.root && a.fruit_pivot.is_some())
                .and_then(|a| a.fruit_pivot)
                .unwrap_or(request.root.as_vec3() + STEM_TIP_VOXELS);
            samples.push(Sample {
                request,
                blend,
                motion,
                pivot,
                scale: gui.apple_size_scale,
                max_angle: gui.fruit_swing_max_angle_radians,
            });
        }
        if let Some(buffer) = &readback {
            frame
                .command_buffer()
                .use_buffer(buffer, BufferUse::HostRead);
        }
        self.fruit_releases.flights[slot] = Some(Flight {
            samples,
            readback,
            probe: frame.readback_probe(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn released_pose_preserves_the_stem_pivot_under_scale_wind_and_branch_motion() {
        let root = UVec3::splat(128);
        let pivot = Vec3::new(128.3, 131.2, 128.5);
        let branch = BranchPose {
            rotation: Quat::from_rotation_z(0.3),
            translation: Vec3::new(0.02, 0., 0.01),
        };
        for scale in [0.25, 0.5, 1., 2.] {
            for motion in [None, Some((branch, branch, 0.016))] {
                let pose =
                    held_pose(root, pivot, scale, [1.5, -0.7, 0.2, 0.4], 1., motion).unwrap();
                let tip = root.as_vec3() + pose.offset + pose.rotation * (STEM_TIP_VOXELS * scale);
                let expected =
                    motion.map_or(pivot, |(b, _, _)| b.transform_point(pivot / 256.) * 256.);
                assert!(tip.distance(expected) < 1e-4);
                assert!(pose.velocity.length() > 0. && pose.angular_velocity.length() > 0.);
            }
        }
    }

    #[test]
    fn sparse_handoff_reads_only_the_exact_held_bucket_and_four_corners() {
        let root = UVec3::new(128, 37, 128);
        let (offsets, f) = corners([0., 0., 1., 3.], [2, 2, 1, 12], root);
        let bucket = u64::from(super::super::vegetation_response::instance_seed(root) % 4);
        assert_eq!(
            offsets,
            std::array::from_fn(|i| (4 + i as u64) * STATE_BYTES + 16 + bucket * 16)
        );
        assert_eq!(f, Vec2::splat(0.5));
        assert_eq!(
            interpolate([[0.; 4], [1.; 4], [2.; 4], [3.; 4]], f),
            [1.5; 4]
        );
        assert_eq!(BYTES_PER_FRUIT, 64);
        let sample = Sample {
            request: Request {
                ticket: FruitReleaseTicket(1),
                root,
                generation: 2,
            },
            blend: f,
            motion: None,
            pivot: root.as_vec3() + STEM_TIP_VOXELS,
            scale: 1.,
            max_angle: 1.,
        };
        assert_eq!(decode(&sample, [0.; 4]).unwrap().offset, Vec3::ZERO);
        assert!(decode(&sample, [f32::NAN; 4]).is_err());
    }
}
