# Chunk Queue Refactor and Moisture Drying Plan

## 背景

当前 terrain moisture drying 已经可以工作，但初版实现每个 world tick 都 dispatch 一次全 atlas dry shader。全 atlas 尺寸约为 `768 x 256 x 768`，约 151M voxels；release hidden perf 临时测得单次 dry dispatch 约 3.7ms。即使 wet voxel 很少，shader 仍需要扫描完整 atlas，因此会在 tick 帧产生明显 spike。

我们希望把 dry 更新分摊到 chunk 粒度：周期性 enqueue 所有 terrain chunks，然后每帧最多处理一个 chunk。这样每次 shader 只扫描单 chunk region，降低单帧峰值，并让更新更平滑。

同时，项目已有多个 chunk queue 类型：

- `ChunkWorkQueue`
- `LatestChunkQueue<T>`
- `GrowingFloraQueue`

这些 queue 有重复的“去重、pending 顺序、nearest/FIFO pop”逻辑。后续 moisture drying 也需要 queue，因此先整理 queue 设计，避免继续复制。

## 当前 Queue 现状

### `ChunkWorkQueue`

路径：`src/util/chunk_work_queue.rs`

职责：底层 chunk pending queue。

当前能力：

- `push(chunk_id)`：去重 enqueue。
- `remove(chunk_id)`。
- `pop_nearest_to(...)` / `pop_nearest_to_if(...)`：按 focus 距离优先，并带 aging 防饿死。
- `pop_next()`：FIFO pop，但当前只在 `#[cfg(test)]` 下可用。

问题：

- 生产代码不能直接使用 FIFO / round-robin pop。
- 选择策略散落在方法名里，不够显式。

### `LatestChunkQueue<T>`

路径：`src/util/latest_chunk_queue.rs`

职责：异步/延迟 chunk work 的 latest-work wrapper。

当前使用场景：

- `deferred_chunk_rebuilds`
- `deferred_terrain_sdf_source_refreshes`
- `deferred_terrain_sdf_collider_rebuilds`
- `deferred_water_terrain_cache_rebuilds`

它不仅是 queue，还维护：

- 每 chunk 最新 payload。
- revision。
- active/inflight revision。
- complete 后 stale result 防护。
- active 期间新 work 到达时，完成后自动 requeue。

结论：这个语义对异步 rebuild 很重要，不应该简单删除。

### `GrowingFloraQueue`

路径：`src/util/growing_flora_queue.rs`

职责：flora growth chunk queue。

当前能力：

- chunk 去重。
- 保存并刷新 `last_flora_tick` payload。
- nearest pop。
- test-only FIFO pop。

问题：

- 与 `ChunkWorkQueue` 重复维护 `HashMap + VecDeque` pending 结构。
- 选择策略与底层 queue 重复。

## 设计目标

1. DRY：chunk 去重、pending 顺序、pop 策略只实现一次。
2. 保留业务语义：异步 latest/revision queue 和简单 gameplay queue 不强行混成一个大而全类型。
3. 显式 pop 模式：调用方清楚选择 FIFO/round-robin 还是 nearest-with-aging。
4. 支持 predicate：可以跳过尚未 ready 的 payload/chunk，且不误删 pending work。
5. 为 moisture drying 提供低成本 round-robin queue。
6. 保持现有行为兼容：terrain rebuild / water collider / flora growth 不应发生调度行为回归，除非明确迁移。

## 建议架构

### 1. 抽象底层 pop 模式

在 `ChunkWorkQueue` 增加生产可用的 FIFO pop，并显式建模 pop 策略：

```rust
pub(crate) enum ChunkPopMode {
    Fifo,
    NearestWithAging {
        focus: Vec3,
        chunk_extent: UVec3,
    },
}
```

建议 API：

```rust
impl ChunkWorkQueue {
    pub(crate) fn pop(&mut self, mode: ChunkPopMode) -> Option<UVec3>;
    pub(crate) fn pop_if(
        &mut self,
        mode: ChunkPopMode,
        is_ready: impl FnMut(UVec3) -> bool,
    ) -> Option<UVec3>;
}
```

保留现有 `pop_nearest_to` / `pop_nearest_to_if` 作为兼容薄 wrapper，或分阶段迁移后删除。

### 2. 保留 `LatestChunkQueue<T>` 作为 latest/revision wrapper

`LatestChunkQueue<T>` 继续存在，因为它表达异步 work 的关键状态机。

改进方向：

- 内部继续使用 `ChunkWorkQueue`。
- 新增 `pop(mode)` / `pop_if(mode, ...)`。
- 保留原 `pop_nearest_to...` 作为兼容 wrapper。
- 如需要 FIFO latest work，可直接调用 `pop(ChunkPopMode::Fifo)`。

### 3. 收敛 `GrowingFloraQueue`

可选方案 A：把 `GrowingFloraQueue` 改成薄 wrapper，内部使用 `ChunkWorkQueue + HashMap<UVec3, u32>`。

可选方案 B：如果后续 `LatestChunkQueue<T>` 支持 non-inflight/simple 模式过于复杂，则保留 `GrowingFloraQueue` 名称，但共享底层 pop 策略。

建议采用方案 A：保留业务类型名，避免调用方语义丢失，同时消除重复 pop 逻辑。

### 4. 新增 moisture drying queue

Moisture drying 需要的是简单 round-robin/FIFO：

- 每 20 个 world tick enqueue 所有 terrain chunks。
- 每帧最多 pop 一个 chunk。
- pop 模式：`ChunkPopMode::Fifo`。
- 对 popped chunk dispatch dry shader，仅扫描该 chunk 的 atlas region。

这个需求可以直接使用 `ChunkWorkQueue`，不需要 latest/revision wrapper。

## Moisture Drying 改进计划

### 当前行为

当前 dry 逻辑：

- 每 world tick 执行一次 dry pass。
- dry pass 扫完整 atlas。
- 每个 moisture > 0 voxel 有 `0.02` 概率降低 1 个湿度等级。

问题：

- 单次 dispatch 扫完整 atlas，约 3.7ms。
- 每 tick 跑，会在 tick 帧产生 spike。
- 概率变小只能减少写入，不能减少读扫描成本。

### 目标行为

- 每 20 个 world tick enqueue 全部 chunks。
- 每帧最多处理 1 个 chunk。
- dry shader 参数改为 chunk offset/dim。
- shader 只扫描该 chunk 的 atlas region。
- 一轮最多处理 `CHUNK_DIM.x * CHUNK_DIM.y * CHUNK_DIM.z` 个 chunks。

当前默认 `CHUNK_DIM = 3 x 1 x 3`，即 9 chunks。

### 概率语义

需要明确设计选择：

- 如果仍使用 `0.02`：每 chunk 每 20 world tick 判定一次，整体干燥速度会比“每 tick 0.02”慢很多。
- 如果希望接近当前“每 tick 0.02”的累计效果，20 tick 一次的概率应约为：

```text
1 - (1 - 0.02)^20 ≈ 0.33
```

建议先保留可调常量，并在实际手感测试后确定：

```rust
const TERRAIN_MOISTURE_DRY_ENQUEUE_INTERVAL_WORLD_TICKS: u32 = 20;
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_VISIT: f32 = 0.02; // or 0.33 if preserving current speed
const TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME: usize = 1;
```

## Implementation Checklist

### Phase 1: Queue API cleanup

- [ ] Make FIFO pop available in production `ChunkWorkQueue`.
- [ ] Add explicit `ChunkPopMode` enum.
- [ ] Add `ChunkWorkQueue::pop(mode)`.
- [ ] Add `ChunkWorkQueue::pop_if(mode, predicate)`.
- [ ] Keep existing `pop_nearest_to` wrappers temporarily for compatibility.
- [ ] Add tests for production FIFO mode.
- [ ] Add tests for nearest-with-aging through `ChunkPopMode`.
- [ ] Add tests that `pop_if(Fifo, not_ready)` preserves pending work.

### Phase 2: `LatestChunkQueue<T>` integration

- [ ] Add `LatestChunkQueue::pop(mode)`.
- [ ] Add `LatestChunkQueue::pop_if(mode, predicate)`.
- [ ] Add payload-aware pop for explicit mode, equivalent to current `pop_nearest_to_if_payload`.
- [ ] Keep existing nearest APIs as wrappers.
- [ ] Validate terrain rebuild and water queues still behave the same.
- [ ] Add tests for FIFO latest pop.
- [ ] Add tests for active revision + newer work requeue with FIFO mode.

### Phase 3: `GrowingFloraQueue` deduplication

- [ ] Refactor `GrowingFloraQueue` to use `ChunkWorkQueue` internally for pending order/pop.
- [ ] Preserve payload semantics: duplicate push refreshes `last_flora_tick`.
- [ ] Preserve existing nearest behavior.
- [ ] Expose FIFO only if needed by future callers.
- [ ] Ensure all existing `GrowingFloraQueue` tests still pass.

### Phase 4: Moisture drying queue

- [ ] Add `moisture_dry_chunks: ChunkWorkQueue` to `App`.
- [ ] Replace per-tick full-atlas dry dispatch with enqueue accumulator.
- [ ] Every `TERRAIN_MOISTURE_DRY_ENQUEUE_INTERVAL_WORLD_TICKS`, enqueue all chunks in `0..CHUNK_DIM`.
- [ ] Each frame, pop at most `TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME` using FIFO.
- [ ] Dispatch dry shader for only that chunk.
- [ ] Decide and document final dry probability per chunk visit.

### Phase 5: Dry shader region support

- [ ] Update `apply_terrain_moisture_dry_tick` to accept `chunk_id` or explicit `atlas_offset + atlas_dim`.
- [ ] Fill uniform with chunk offset/dim instead of full atlas dim.
- [ ] Keep seed changes per chunk dispatch.
- [ ] Confirm shader already respects `offset + local` and works for subregions.
- [ ] Rename API to something like `apply_terrain_moisture_dry_region` or `apply_terrain_moisture_dry_chunk`.

### Phase 6: Validation and measurement

- [ ] `cargo fmt --check`.
- [ ] `cargo check`.
- [ ] `cargo test`.
- [ ] `cargo run --release -- --hidden --mute --auto-exit 0.5`.
- [ ] Inspect latest log for errors.
- [ ] Add temporary perf instrumentation or GPU scope for dry chunk dispatch.
- [ ] Compare before/after spike:
  - full atlas dry dispatch: ~3.7ms observed.
  - expected single chunk dispatch: roughly full cost / 9 for current world, plus dispatch overhead.
- [ ] Remove temporary perf/debug instrumentation before commit unless it is behind `--perf` and intended to stay.

## Risks / Notes

- Splitting by chunk smooths spikes but may make drying spatially wave-like if one chunk per frame is visibly staggered. Current chunk count is small, so this is likely acceptable.
- FIFO order should be stable and deterministic. If visual wave direction is noticeable, enqueue order can be randomized per cycle later, while still using FIFO pop.
- Full-atlas dry shader cost is mostly scan/read cost; reducing probability does not materially reduce dispatch cost.
- `LatestChunkQueue<T>` should not be deleted blindly: its revision/inflight state protects asynchronous terrain/water results from stale writes.
- Keep queue refactor separate from moisture behavior change where possible, so regressions are easier to isolate.
