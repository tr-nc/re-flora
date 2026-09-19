//! GPU hierarchy integration with a compact, versioned CPU physics publication.
//! Submit before GUI work; consume before surface/physics publication. Only joint
//! state crosses back to the CPU, never wood vertices. The private GPU resources
//! are not used by rendering, so edits cannot invalidate an in-flight solver job.
use anyhow::{ensure, Result};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline, DescriptorPool,
    Extent3D, GpuJobToken, GpuProfiler, MemoryLocation, PipelineStage, ShaderModule, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use std::time::Instant;

use super::pose::{GpuPoseJoint, GpuPoseState, PoseVersion, TreePose, TreeStiffness};
use crate::{resource::Resource, wind_field::WindFieldFrame};

#[derive(ResourceContainer)]
struct Resources {
    tree_pose_joints: Resource<Buffer>,
    tree_pose_state: Resource<Buffer>,
    tree_pose_ranges: Resource<Buffer>,
    tree_pose_step: Resource<Buffer>,
    wind_field_info: Resource<Buffer>,
}

struct Resident {
    resources: Resources,
    seed: Buffer,
    readback: Buffer,
    pipeline: ComputePipeline,
    _pool: DescriptorPool,
    versions: Vec<(u32, PoseVersion)>,
    state_bytes: u64,
}

impl Resident {
    fn new(
        ctx: &VulkanContext,
        allocator: Allocator,
        shader: &ShaderModule,
        trees: &[(u32, &TreePose)],
    ) -> Result<Self> {
        let buffer = |bytes: usize, flags, memory| {
            Buffer::new_sized(
                ctx.device().clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                memory,
                bytes.max(16) as u64,
            )
        };
        let host = |bytes| {
            Resource::new(buffer(
                bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
            ))
        };
        let mut joints = Vec::<GpuPoseJoint>::new();
        let mut states = Vec::<GpuPoseState>::new();
        let mut ranges = Vec::new();
        for &(_, pose) in trees {
            let start = u32::try_from(joints.len())?;
            let count = u32::try_from(pose.branches().len())?;
            let tree_joints = pose.gpu_joints();
            let levels = tree_joints
                .iter()
                .map(|j| j.parent[1] + 1)
                .max()
                .unwrap_or(0);
            ranges.push([start, count, levels, 0]);
            joints.extend(tree_joints);
            states.extend(pose.gpu_state());
        }
        let _ = u32::try_from(joints.len())?;
        let state_bytes = std::mem::size_of_val(states.as_slice());
        let resources = Resources {
            tree_pose_joints: host(std::mem::size_of_val(joints.as_slice())),
            tree_pose_state: Resource::new(buffer(
                state_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuOnly,
            )),
            tree_pose_ranges: host(std::mem::size_of_val(ranges.as_slice())),
            tree_pose_step: host(16),
            wind_field_info: Resource::new(buffer(
                std::mem::size_of::<WindFieldFrame>(),
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
            )),
        };
        resources.tree_pose_joints.fill(&joints)?;
        resources.tree_pose_ranges.fill(&ranges)?;
        let seed = buffer(
            state_bytes,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        );
        seed.fill(&states)?;
        let readback = buffer(
            state_bytes,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        );
        let pool = DescriptorPool::new(ctx.device())?;
        let pipeline = ComputePipeline::new(ctx.device(), shader, &pool, &[&resources]);
        Ok(Self {
            resources,
            seed,
            readback,
            pipeline,
            _pool: pool,
            versions: trees
                .iter()
                .map(|(id, pose)| (*id, pose.version()))
                .collect(),
            state_bytes: state_bytes as u64,
        })
    }
}

struct Pending {
    job: GpuJobToken,
    trees: Vec<(u32, PoseVersion, usize)>,
    reference: Option<Vec<GpuPoseState>>,
}

pub(crate) struct PoseUpdate {
    pub tree: u32,
    pub source: PoseVersion,
    pub state: Vec<GpuPoseState>,
}

#[derive(Default)]
pub(crate) struct PoseTimings {
    pub submit_us: f64,
    pub wait_us: f64,
    pub readback_us: f64,
    pub gpu_us: Option<f64>,
    pub readback_bytes: u64,
}

pub(crate) struct GpuTreePoseSolver {
    ctx: VulkanContext,
    allocator: Allocator,
    shader: ShaderModule,
    resident: Option<Resident>,
    pending: Option<Pending>,
    profiler: Option<GpuProfiler>,
    pub timings: PoseTimings,
}

impl GpuTreePoseSolver {
    pub fn new(ctx: VulkanContext, allocator: Allocator) -> Result<Self> {
        let shader =
            ShaderModule::from_precompiled(ctx.device(), "shader/trees/tree_pose.comp", "main")
                .map_err(anyhow::Error::msg)?;
        Ok(Self {
            ctx,
            allocator,
            shader,
            resident: None,
            pending: None,
            profiler: None,
            timings: PoseTimings::default(),
        })
    }

    pub fn submit<'a>(
        &mut self,
        trees: impl Iterator<Item = (u32, &'a TreePose)>,
        wind: &WindFieldFrame,
        dt: f32,
        stiffness: TreeStiffness,
        validate: bool,
        profile: bool,
    ) -> Result<()> {
        ensure!(
            self.pending.is_none(),
            "tree pose job must be consumed before resubmission"
        );
        TreePose::validate_step(wind, dt)?;
        self.timings = PoseTimings::default();
        if profile && self.profiler.is_none() {
            self.profiler = GpuProfiler::maybe_new(&self.ctx, 1, 1, "TREE_POSE");
        }
        let trees: Vec<_> = trees.collect();
        if dt == 0. || trees.is_empty() {
            return Ok(());
        }
        let started = Instant::now();
        let versions: Vec<_> = trees
            .iter()
            .map(|(id, pose)| (*id, pose.version()))
            .collect();
        let reset = self
            .resident
            .as_ref()
            .is_none_or(|r| r.versions != versions);
        if reset {
            self.resident = Some(Resident::new(
                &self.ctx,
                self.allocator.clone(),
                &self.shader,
                &trees,
            )?);
        }
        let resident = self.resident.as_ref().unwrap();
        let resources = &resident.resources;
        resources.wind_field_info.fill_uniform(wind)?;
        resources.tree_pose_step.fill(&[[
            u32::try_from(trees.len())?,
            dt.to_bits(),
            stiffness.compliance_scale.to_bits(),
            stiffness.frequency_scale.to_bits(),
        ]])?;
        let reference = if validate {
            let mut states = Vec::new();
            for &(_, pose) in &trees {
                let mut reference = pose.clone();
                reference.advance_with_stiffness(wind, dt, stiffness)?;
                states.extend(reference.gpu_state());
            }
            Some(states)
        } else {
            None
        };
        let command = CommandBuffer::new(self.ctx.device(), self.ctx.command_pool());
        command.begin(true);
        let scope = self.profiler.as_mut().and_then(|profiler| {
            profiler.begin_frame(0, &command);
            profiler.begin_scope(0, &command, "tree_pose.job", PipelineStage::ALL_COMMANDS)
        });
        for buffer in [&resources.wind_field_info, &resources.tree_pose_step] {
            command.use_buffer(buffer, BufferUse::HostWrite);
        }
        if reset {
            for buffer in [&resources.tree_pose_ranges, &resources.tree_pose_joints] {
                command.use_buffer(buffer, BufferUse::HostWrite);
            }
            if resident.state_bytes != 0 {
                command.use_buffer(&resident.seed, BufferUse::HostWrite);
                resident.seed.record_copy_to_buffer(
                    &command,
                    &resources.tree_pose_state,
                    resident.state_bytes,
                    0,
                    0,
                );
            }
        }
        resident.pipeline.record(
            &command,
            Extent3D::new(
                u32::try_from(trees.len())?
                    .checked_mul(64)
                    .ok_or_else(|| anyhow::anyhow!("too many tree pose workgroups"))?,
                1,
                1,
            ),
            None,
        );
        if resident.state_bytes != 0 {
            resources.tree_pose_state.record_copy_to_buffer(
                &command,
                &resident.readback,
                resident.state_bytes,
                0,
                0,
            );
            command.use_buffer(&resident.readback, BufferUse::HostRead);
        }
        if let (Some(profiler), Some(scope)) = (self.profiler.as_mut(), scope) {
            profiler.end_scope(0, &command, scope, PipelineStage::ALL_COMMANDS);
        }
        command.end();
        let job = command.submit_gpu_job(&self.ctx.get_general_queue(), "tree.pose")?;
        self.pending = Some(Pending {
            job,
            reference,
            trees: trees
                .iter()
                .map(|(id, pose)| (*id, pose.version(), pose.branches().len()))
                .collect(),
        });
        self.timings.submit_us = started.elapsed().as_secs_f64() * 1e6;
        self.timings.readback_bytes = resident.state_bytes;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Vec<PoseUpdate>> {
        let Some(pending) = self.pending.take() else {
            return Ok(Vec::new());
        };
        let started = Instant::now();
        pending.job.wait_complete()?;
        let waited = started.elapsed();
        let resident = self.resident.as_mut().unwrap();
        let bytes = resident.readback.read_back_range(0, resident.state_bytes)?;
        let states: Vec<GpuPoseState> = bytes
            .chunks_exact(std::mem::size_of::<GpuPoseState>())
            .map(bytemuck::pod_read_unaligned)
            .collect();
        if let Some(reference) = pending.reference {
            ensure!(
                states.len() == reference.len(),
                "GPU hierarchy output count mismatch"
            );
            let mut error = 0.0_f32;
            for (actual, expected) in states.iter().zip(&reference) {
                for (&a, &b) in bytemuck::cast_slice::<_, f32>(std::slice::from_ref(actual))
                    .iter()
                    .zip(bytemuck::cast_slice::<_, f32>(std::slice::from_ref(
                        expected,
                    )))
                {
                    ensure!(a.is_finite(), "nonfinite GPU hierarchy result");
                    error = error.max((a - b).abs());
                }
            }
            ensure!(
                error < 2e-5,
                "GPU hierarchy differs from CPU integrator: {error}"
            );
            log::info!(
                "[TREE][GPU_POSE] joints={} max_error={} readback_bytes={} wait_us={}",
                states.len(),
                error,
                resident.state_bytes,
                waited.as_micros()
            );
        }
        let mut offset = 0;
        let mut output = Vec::with_capacity(pending.trees.len());
        for (tree, source, count) in pending.trees {
            output.push(PoseUpdate {
                tree,
                source,
                state: states[offset..offset + count].to_vec(),
            });
            offset += count;
        }
        resident.versions = output
            .iter()
            .map(|update| (update.tree, update.source.advanced()))
            .collect();
        self.timings.wait_us = waited.as_secs_f64() * 1e6;
        self.timings.readback_us = (started.elapsed() - waited).as_secs_f64() * 1e6;
        self.timings.gpu_us = self
            .profiler
            .as_ref()
            .map(|profiler| profiler.try_collect_frame(0))
            .transpose()?
            .flatten()
            .and_then(|results| results.scopes.first().map(|s| s.duration_ns / 1000.));
        Ok(output)
    }

    /// Explicit shutdown/cancellation consumes ownership, even if edits made the
    /// result obsolete. Never abandon a managed GPU job or publish stale state.
    pub fn discard(&mut self) -> Result<()> {
        if let Some(pending) = self.pending.take() {
            pending.job.wait_complete()?;
        }
        self.resident = None;
        Ok(())
    }
}

impl Drop for GpuTreePoseSolver {
    fn drop(&mut self) {
        if let Err(error) = self.discard() {
            log::error!("Failed to drain tree pose solver: {error:#}");
        }
    }
}
