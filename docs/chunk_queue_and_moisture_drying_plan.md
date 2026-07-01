# Chunk Queue Refactor and Moisture Drying Plan

## 背景

当前 terrain moisture drying 已经可以工作，但初版实现每个 world tick 都 dispatch 一次全 atlas dry shader。全 atlas 尺寸约为 `768 x 256 x 768`，约 151M voxels；release hidden perf 临时测得单次 dry dispatch 约 3.7ms。即使 wet voxel 很少，shader 仍需要扫描完整 atlas，因此会在 tick 帧产生明显 spike。

我们希望把 dry 更新分摊到 chunk 粒度：按固定 round-robin cursor 每帧处理一个 terrain chunk。这样每次 shader 只扫描单 chunk region，降低单帧峰值，并让每个 chunk 的更新频率稳定。

同时，项目已有多个 chunk queue 类型：

- `ChunkWorkQueue`
- `LatestChunkQueue<T>`
- `GrowingFloraQueue`

这些 queue 有重复的“去重、pending 顺序、nearest/FIFO pop”逻辑。早期 moisture drying 方案也考虑过 queue，因此先整理 queue 设计，避免继续复制。

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
5. 为 chunk 粒度 gameplay work 提供低成本 FIFO/round-robin building blocks。
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

### 4. Moisture drying round-robin

Moisture drying 最终改为固定 cursor 的 round-robin，而不是 pending queue：

- 每帧推进到下一个 terrain chunk。
- 当前默认 `CHUNK_DIM = 3 x 1 x 3`，因此 9 帧内每个 chunk 都会被访问一次。
- 对当前 chunk dispatch dry shader，仅扫描该 chunk 的 atlas region。

这个需求不需要 latest/revision wrapper，也不需要周期性 enqueue；简单 cursor 能保证每个 chunk 的访问频率稳定。

## Moisture Drying 改进计划

### 初版行为

初版 dry 逻辑：

- 每 world tick 执行一次 dry pass。
- dry pass 扫完整 atlas。
- 每个 moisture > 0 voxel 有 `0.02` 概率降低 1 个湿度等级。

问题：

- 单次 dispatch 扫完整 atlas，约 3.7ms。
- 每 tick 跑，会在 tick 帧产生 spike。
- 概率变小只能减少写入，不能减少读扫描成本。

### 目标行为

- 不再按 world tick 周期 enqueue。
- 每帧按 fixed round-robin cursor 处理 1 个 chunk。
- dry shader 参数改为 chunk offset/dim。
- shader 只扫描该 chunk 的 atlas region。
- 一轮固定访问 `CHUNK_DIM.x * CHUNK_DIM.y * CHUNK_DIM.z` 个 chunks。

当前默认 `CHUNK_DIM = 3 x 1 x 3`，即 9 chunks，因此每个 chunk 每 9 帧访问一次。

### 概率语义

需要明确设计选择：

- 每次 chunk visit 内，每个 wet voxel 独立随机判定。
- 当前使用 `0.01` 作为每次 chunk visit 的概率。
- 因为当前有 9 个 chunks，每个 chunk 每 9 帧访问一次；这个节奏比“每 20 world tick enqueue 一轮，再由队列分摊”更稳定，也不会因为 queue 去重导致访问次数被吞掉。

```rust
const TERRAIN_MOISTURE_DRY_PROBABILITY_PER_CHUNK_VISIT: f32 = 0.01;
const TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME: usize = 1;
```

## Implementation Checklist

### Phase 1: Queue API cleanup

- [x] Make FIFO pop available in production `ChunkWorkQueue`.
- [x] Add explicit `ChunkPopMode` enum.
- [x] Add `ChunkWorkQueue::pop(mode)`.
- [x] Add `ChunkWorkQueue::pop_if(mode, predicate)`.
- [x] Keep existing `pop_nearest_to` wrappers temporarily for compatibility.
- [x] Add tests for production FIFO mode.
- [x] Add tests for nearest-with-aging through `ChunkPopMode`.
- [x] Add tests that `pop_if(Fifo, not_ready)` preserves pending work.

### Phase 2: `LatestChunkQueue<T>` integration

- [x] Add `LatestChunkQueue::pop(mode)`.
- [x] Add `LatestChunkQueue::pop_if(mode, predicate)`.
- [x] Add payload-aware pop for explicit mode, equivalent to current `pop_nearest_to_if_payload`.
- [x] Keep existing nearest APIs as wrappers.
- [x] Validate terrain rebuild and water queues still behave the same.
- [x] Add tests for FIFO latest pop.
- [x] Add tests for active revision + newer work requeue with FIFO mode.

### Phase 3: `GrowingFloraQueue` deduplication

- [x] Refactor `GrowingFloraQueue` to use `ChunkWorkQueue` internally for pending order/pop.
- [x] Preserve payload semantics: duplicate push refreshes `last_flora_tick`.
- [x] Preserve existing nearest behavior.
- [x] Expose FIFO only if needed by future callers.
- [x] Ensure all existing `GrowingFloraQueue` tests still pass.

### Phase 4: Moisture drying round-robin

- [x] Replace per-tick full-atlas dry dispatch with chunk-region dispatch.
- [x] Remove world-tick enqueue accumulator from moisture drying.
- [x] Add fixed `moisture_dry_chunk_cursor` to `App`.
- [x] Each frame, record at most `TERRAIN_MOISTURE_DRY_CHUNKS_PER_FRAME` chunk using round-robin order.
- [x] Dispatch dry shader for only that chunk.
- [x] Set and document final dry probability per chunk visit (`0.01`).

### Phase 5: Dry shader region support

- [x] Update `apply_terrain_moisture_dry_tick` to accept `chunk_id` or explicit `atlas_offset + atlas_dim`.
- [x] Fill uniform with chunk offset/dim instead of full atlas dim.
- [x] Keep seed changes per chunk dispatch.
- [x] Confirm shader already respects `offset + local` and works for subregions.
- [x] Rename API to something like `apply_terrain_moisture_dry_region` or `apply_terrain_moisture_dry_chunk`.

### Phase 6: Validation and measurement

- [x] `cargo fmt --check`.
- [x] `cargo check`.
- [x] `cargo test`.
- [x] `cargo run --release -- --hidden --mute --auto-exit 0.5`.
- [x] Inspect latest log for errors.
- [x] Add temporary perf instrumentation or GPU scope for dry chunk dispatch.
- [x] Compare before/after spike:
  - full atlas dry dispatch via synchronous one-time command: ~3.7ms observed.
  - single chunk dispatch via synchronous one-time command: 24 samples, avg ~2.39ms, median ~2.18ms, p95 ~3.07ms, max ~3.82ms.
  - non-blocking frame-recorded dry pass: CPU record 25 samples, avg ~0.018ms, median ~0.017ms, p95 ~0.024ms, max ~0.027ms; GPU frame scope samples showed `moisture_dry.pass` around ~194us.
  - the synchronous single-chunk path did not fall linearly because `execute_one_time_command` paid fixed submit/wait overhead; recording the pass into the normal frame command removes that CPU stall.
- [x] Remove temporary perf/debug instrumentation before commit unless it is behind `--perf` and intended to stay.

## Risks / Notes

- Splitting by chunk smooths spikes but may make drying spatially wave-like if one chunk per frame is visibly staggered. Current chunk count is small, so this is likely acceptable.
- Round-robin order is stable and deterministic. If visual wave direction is noticeable, cursor order can be randomized per cycle later while preserving one visit per chunk per cycle.
- Full-atlas dry shader cost is mostly scan/read cost; reducing probability does not materially reduce dispatch cost.
- `LatestChunkQueue<T>` should not be deleted blindly: its revision/inflight state protects asynchronous terrain/water results from stale writes.
- Keep queue refactor separate from moisture behavior change where possible, so regressions are easier to isolate.
