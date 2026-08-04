use crate::{CommandBuffer, PipelineStage, TimestampQueryPool, VulkanContext};
use ash::prelude::VkResult;

pub struct GpuProfiler {
    query_pool: TimestampQueryPool,
    frame_slots: Vec<GpuProfilerFrameSlot>,
    queries_per_frame: u32,
    max_scopes_per_frame: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuScopeToken {
    scope_index: usize,
}

#[derive(Clone, Debug)]
pub struct GpuProfilerFrameResults {
    pub scopes: Vec<GpuScopeResult>,
    pub dropped_scope_count: u32,
    pub timestamp_period_ns: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GpuScopeResult {
    pub name: &'static str,
    pub start_ticks: u64,
    pub end_ticks: u64,
    pub duration_ns: f64,
}

pub struct GpuProfilerFrame<'a> {
    profiler: &'a mut GpuProfiler,
    frame_slot: usize,
}

pub struct GpuJobProfiler {
    query_pool: TimestampQueryPool,
    slots: Vec<GpuJobProfilerSlot>,
    next_generation: u64,
    dropped_scope_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobScopeToken {
    slot_index: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct GpuJobScopeResult {
    pub name: &'static str,
    pub queue: crate::QueueLane,
    pub start_ticks: u64,
    pub end_ticks: u64,
    pub duration_ns: f64,
}

struct GpuJobProfilerSlot {
    in_use: bool,
    generation: u64,
    name: &'static str,
    queue: crate::QueueLane,
    ended: bool,
}

struct GpuProfilerFrameSlot {
    scopes: Vec<GpuScopeMetadata>,
    written_query_count: u32,
    dropped_scope_count: u32,
}

#[derive(Clone, Copy)]
struct GpuScopeMetadata {
    name: &'static str,
    start_query: u32,
    end_query: u32,
    ended: bool,
}

impl GpuProfiler {
    pub fn maybe_new(
        vulkan_ctx: &VulkanContext,
        frame_slot_count: usize,
        max_scopes_per_frame: usize,
        log_tag: &str,
    ) -> Option<Self> {
        if frame_slot_count == 0 || max_scopes_per_frame == 0 {
            return None;
        }

        let queries_per_frame = max_scopes_per_frame.checked_mul(2)?.try_into().ok()?;
        let max_query_count = frame_slot_count
            .checked_mul(max_scopes_per_frame)?
            .checked_mul(2)?
            .try_into()
            .ok()?;
        let query_pool = TimestampQueryPool::maybe_new(vulkan_ctx, max_query_count, log_tag)?;

        let mut frame_slots = Vec::with_capacity(frame_slot_count);
        for _ in 0..frame_slot_count {
            frame_slots.push(GpuProfilerFrameSlot {
                scopes: Vec::with_capacity(max_scopes_per_frame),
                written_query_count: 0,
                dropped_scope_count: 0,
            });
        }

        Some(Self {
            query_pool,
            frame_slots,
            queries_per_frame,
            max_scopes_per_frame,
        })
    }

    pub fn frame_slot_count(&self) -> usize {
        self.frame_slots.len()
    }

    pub fn max_scopes_per_frame(&self) -> usize {
        self.max_scopes_per_frame
    }

    pub fn timestamp_period_ns(&self) -> f32 {
        self.query_pool.timestamp_period_ns()
    }

    pub fn begin_frame<'a>(
        &'a mut self,
        frame_slot: usize,
        cmdbuf: &CommandBuffer,
    ) -> Option<GpuProfilerFrame<'a>> {
        let first_query = self.first_query_for_frame(frame_slot)?;
        let slot = self.frame_slots.get_mut(frame_slot)?;
        slot.scopes.clear();
        slot.written_query_count = 0;
        slot.dropped_scope_count = 0;
        self.query_pool
            .record_reset_range(cmdbuf, first_query, self.queries_per_frame);

        Some(GpuProfilerFrame {
            profiler: self,
            frame_slot,
        })
    }

    pub fn begin_scope(
        &mut self,
        frame_slot: usize,
        cmdbuf: &CommandBuffer,
        name: &'static str,
        stage: PipelineStage,
    ) -> Option<GpuScopeToken> {
        let first_query = self.first_query_for_frame(frame_slot)?;
        let slot = self.frame_slots.get_mut(frame_slot)?;
        let scope_index = slot.scopes.len();
        if scope_index >= self.max_scopes_per_frame {
            slot.dropped_scope_count = slot.dropped_scope_count.saturating_add(1);
            return None;
        }

        let local_start_query = (scope_index * 2) as u32;
        let local_end_query = local_start_query + 1;
        let absolute_start_query = first_query + local_start_query;
        self.query_pool
            .record_timestamp(cmdbuf, stage, absolute_start_query);
        slot.written_query_count = slot.written_query_count.max(local_start_query + 1);
        slot.scopes.push(GpuScopeMetadata {
            name,
            start_query: local_start_query,
            end_query: local_end_query,
            ended: false,
        });

        Some(GpuScopeToken { scope_index })
    }

    pub fn end_scope(
        &mut self,
        frame_slot: usize,
        cmdbuf: &CommandBuffer,
        token: GpuScopeToken,
        stage: PipelineStage,
    ) {
        let Some(first_query) = self.first_query_for_frame(frame_slot) else {
            return;
        };
        let Some(slot) = self.frame_slots.get_mut(frame_slot) else {
            return;
        };
        let Some(scope) = slot.scopes.get_mut(token.scope_index) else {
            return;
        };
        if scope.ended {
            return;
        }

        let absolute_end_query = first_query + scope.end_query;
        self.query_pool
            .record_timestamp(cmdbuf, stage, absolute_end_query);
        slot.written_query_count = slot.written_query_count.max(scope.end_query + 1);
        scope.ended = true;
    }

    pub fn try_collect_frame(
        &self,
        frame_slot: usize,
    ) -> VkResult<Option<GpuProfilerFrameResults>> {
        let Some(slot) = self.frame_slots.get(frame_slot) else {
            return Ok(None);
        };
        if slot.written_query_count == 0 {
            return Ok(Some(GpuProfilerFrameResults {
                scopes: Vec::new(),
                dropped_scope_count: slot.dropped_scope_count,
                timestamp_period_ns: self.timestamp_period_ns(),
            }));
        }

        let Some(first_query) = self.first_query_for_frame(frame_slot) else {
            return Ok(None);
        };
        let Some(timestamps) = self
            .query_pool
            .try_read_u64_range(first_query, slot.written_query_count)?
        else {
            return Ok(None);
        };

        let timestamp_period_ns = self.timestamp_period_ns();
        let mut scopes = Vec::with_capacity(slot.scopes.len());
        for scope in &slot.scopes {
            if !scope.ended {
                continue;
            }
            let start_index = scope.start_query as usize;
            let end_index = scope.end_query as usize;
            let Some((&start_ticks, &end_ticks)) = timestamps.get(start_index).zip(timestamps.get(end_index)) else {
                continue;
            };
            if end_ticks < start_ticks {
                continue;
            }
            scopes.push(GpuScopeResult {
                name: scope.name,
                start_ticks,
                end_ticks,
                duration_ns: (end_ticks - start_ticks) as f64 * timestamp_period_ns as f64,
            });
        }

        Ok(Some(GpuProfilerFrameResults {
            scopes,
            dropped_scope_count: slot.dropped_scope_count,
            timestamp_period_ns,
        }))
    }

    fn first_query_for_frame(&self, frame_slot: usize) -> Option<u32> {
        if frame_slot >= self.frame_slots.len() {
            return None;
        }
        (frame_slot as u32).checked_mul(self.queries_per_frame)
    }
}

impl GpuJobProfiler {
    pub fn maybe_new(
        vulkan_ctx: &VulkanContext,
        max_scopes: usize,
        log_tag: &str,
    ) -> Option<Self> {
        if max_scopes == 0 {
            return None;
        }
        let max_query_count = max_scopes.checked_mul(2)?.try_into().ok()?;
        let query_pool = TimestampQueryPool::maybe_new(vulkan_ctx, max_query_count, log_tag)?;
        let mut slots = Vec::with_capacity(max_scopes);
        for _ in 0..max_scopes {
            slots.push(GpuJobProfilerSlot {
                in_use: false,
                generation: 0,
                name: "",
                queue: crate::QueueLane::General,
                ended: false,
            });
        }

        Some(Self {
            query_pool,
            slots,
            next_generation: 1,
            dropped_scope_count: 0,
        })
    }

    pub fn max_scopes(&self) -> usize {
        self.slots.len()
    }

    pub fn dropped_scope_count(&self) -> u32 {
        self.dropped_scope_count
    }

    pub fn timestamp_period_ns(&self) -> f32 {
        self.query_pool.timestamp_period_ns()
    }

    pub fn begin_scope(
        &mut self,
        cmdbuf: &CommandBuffer,
        name: &'static str,
        queue: crate::QueueLane,
        stage: PipelineStage,
    ) -> Option<GpuJobScopeToken> {
        let Some(slot_index) = self.slots.iter().position(|slot| !slot.in_use) else {
            self.dropped_scope_count = self.dropped_scope_count.saturating_add(1);
            return None;
        };

        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let first_query = (slot_index * 2) as u32;
        self.query_pool.record_reset_range(cmdbuf, first_query, 2);
        self.query_pool.record_timestamp(cmdbuf, stage, first_query);

        self.slots[slot_index] = GpuJobProfilerSlot {
            in_use: true,
            generation,
            name,
            queue,
            ended: false,
        };

        Some(GpuJobScopeToken {
            slot_index,
            generation,
        })
    }

    pub fn end_scope(
        &mut self,
        cmdbuf: &CommandBuffer,
        token: GpuJobScopeToken,
        stage: PipelineStage,
    ) {
        let Some(slot) = self.slots.get_mut(token.slot_index) else {
            return;
        };
        if !slot.in_use || slot.generation != token.generation || slot.ended {
            return;
        }
        self.query_pool
            .record_timestamp(cmdbuf, stage, (token.slot_index * 2 + 1) as u32);
        slot.ended = true;
    }

    pub fn try_collect_scope(
        &mut self,
        job: &crate::GpuJobToken,
        token: GpuJobScopeToken,
    ) -> VkResult<Option<GpuJobScopeResult>> {
        if !job.is_complete()? {
            return Ok(None);
        }
        self.collect_completed_scope(token)
    }

    pub fn collect_completed_scope(
        &mut self,
        token: GpuJobScopeToken,
    ) -> VkResult<Option<GpuJobScopeResult>> {
        let Some(slot) = self.slots.get(token.slot_index) else {
            return Ok(None);
        };
        if !slot.in_use || slot.generation != token.generation || !slot.ended {
            return Ok(None);
        }

        let name = slot.name;
        let queue = slot.queue;
        let first_query = (token.slot_index * 2) as u32;
        let timestamps = self.query_pool.read_u64_range(first_query, 2)?;
        self.slots[token.slot_index].in_use = false;

        let start_ticks = timestamps[0];
        let end_ticks = timestamps[1];
        if end_ticks < start_ticks {
            return Ok(None);
        }

        Ok(Some(GpuJobScopeResult {
            name,
            queue,
            start_ticks,
            end_ticks,
            duration_ns: (end_ticks - start_ticks) as f64
                * self.query_pool.timestamp_period_ns() as f64,
        }))
    }

    /// Releases a job scope whose result is intentionally discarded.
    ///
    /// Shutdown consumes the owning GPU job but must not perform profiler
    /// readback just to publish a result that no caller can observe.  The
    /// generation check keeps a stale token from releasing a newer scope.
    pub fn discard_scope(&mut self, token: GpuJobScopeToken) {
        let Some(slot) = self.slots.get_mut(token.slot_index) else {
            return;
        };
        if slot.in_use && slot.generation == token.generation {
            slot.in_use = false;
        }
    }
}

impl GpuProfilerFrame<'_> {
    pub fn begin_scope(
        &mut self,
        cmdbuf: &CommandBuffer,
        name: &'static str,
        stage: PipelineStage,
    ) -> Option<GpuScopeToken> {
        self.profiler
            .begin_scope(self.frame_slot, cmdbuf, name, stage)
    }

    pub fn end_scope(
        &mut self,
        cmdbuf: &CommandBuffer,
        token: GpuScopeToken,
        stage: PipelineStage,
    ) {
        self.profiler
            .end_scope(self.frame_slot, cmdbuf, token, stage);
    }

    pub fn dropped_scope_count(&self) -> u32 {
        self.profiler.frame_slots[self.frame_slot].dropped_scope_count
    }

    pub fn recorded_scope_count(&self) -> usize {
        self.profiler.frame_slots[self.frame_slot].scopes.len()
    }
}

impl GpuScopeResult {
    pub fn duration_us(&self) -> f64 {
        self.duration_ns / 1_000.0
    }

    pub fn duration_ms(&self) -> f64 {
        self.duration_ns / 1_000_000.0
    }
}

impl GpuJobScopeResult {
    pub fn duration_us(&self) -> f64 {
        self.duration_ns / 1_000.0
    }

    pub fn duration_ms(&self) -> f64 {
        self.duration_ns / 1_000_000.0
    }
}
