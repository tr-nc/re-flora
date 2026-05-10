# Deferred chunk rebuild plan

## Background

Dragging GUI tree sliders currently replaces the debug tree immediately on every value change.
The tree replacement path clears/stamps voxels, then synchronously rebuilds all affected chunks.
For the current benchmark case this is usually 8 chunks.

Recent benchmark logs show the cost is dominated by synchronous rebuild work:

```text
[PERF][TREE_BENCH_SUMMARY] samples 3 avg 41.33ms max 45.15ms
[PERF][MESH_REBUILD] chunks 8 rebuilt 8 total 34.55ms surface 17.08ms contree 10.71ms scene_tex 6.75ms
```

`make_surface` itself is roughly 1 to 2ms per chunk, but the full replacement rebuild includes:

- surface generation across 8 chunks
- contree rebuild across 8 chunks
- scene texture update across 8 chunks
- trunk voxel stamping and small tree bookkeeping costs

The practical stutter is caused by doing all chunk rebuilds in one frame.

## Existing related logic

`src/builder/contree/mod.rs` already has a latest-only per-chunk queue for CPU chunk cache readback.
It uses:

- `cpu_chunk_cache_states: HashMap<UVec3, CpuChunkCacheState>`
- `pending_cpu_chunk_cache_queue: VecDeque<UVec3>`
- one active GPU fence job
- per-chunk revisions
- replacement of the latest source while work is pending or active

This is the same conceptual pattern we want for deferred mesh rebuilds.

Important caveat: the existing `dirty_again` flag appears to be set when a newer source arrives during an inflight job, but it does not appear to be consumed later. Before reusing this pattern, confirm and fix that stale-inflight behavior.

## Goal

Extract a reusable latest-only chunk work queue and use it to defer interactive tree rebuilds across frames.

Desired behavior:

- enqueue rebuild requests by chunk id
- replace stale pending work with the newest request for the same chunk
- process a small amount of rebuild work per frame
- if a chunk changes again while active work is running, requeue the newest revision after completion
- keep normal boot/loading/model placement paths synchronous for now

## Proposed module

Add a small reusable utility, for example:

```text
src/util/latest_chunk_queue.rs
```

Potential API:

```rust
pub(crate) struct LatestChunkQueue<T> { ... }

impl<T> LatestChunkQueue<T> {
    pub(crate) fn push(&mut self, chunk_id: UVec3, payload: T) -> u64;
    pub(crate) fn pop_next(&mut self) -> Option<LatestChunkWork<T>>;
    pub(crate) fn complete(&mut self, chunk_id: UVec3, revision: u64);
    pub(crate) fn len(&self) -> usize;
    pub(crate) fn is_empty(&self) -> bool;
}

pub(crate) struct LatestChunkWork<T> {
    pub(crate) chunk_id: UVec3,
    pub(crate) revision: u64,
    pub(crate) payload: T,
}
```

The exact names can change. Keep it small and independent of Vulkan.

## Migration steps

### 1. Extract and test queue logic

- Move the generic queue semantics out of `ContreeBuilder` into the new utility.
- Add unit tests for:
  - enqueue then pop
  - duplicate enqueue before pop keeps latest payload
  - enqueue while active marks dirty/latest
  - completing stale active work requeues latest revision
  - completing latest work marks it done

### 2. Migrate CPU chunk cache readback queue

- Replace the local queue state in `ContreeBuilder` with the reusable queue if practical.
- Preserve the current one-active-job behavior.
- Fix the suspected `dirty_again` gap during migration.

### 3. Add deferred interactive rebuild queue

Add app-level state for deferred rebuilds, likely in `App`:

```rust
deferred_chunk_rebuilds: LatestChunkQueue<ChunkRebuildRequest>
```

Start with a tiny payload:

```rust
struct ChunkRebuildRequest;
```

For GUI tree replacement only:

- keep tree compile and voxel stamping immediate
- enqueue affected chunk ids instead of synchronously rebuilding all of them
- process at most one rebuild chunk per frame initially

### 4. Frame processing

In the redraw loop, after interactive edits and before rendering, process one queued rebuild:

```rust
if let Some(work) = self.deferred_chunk_rebuilds.pop_next() {
    rebuild_one_chunk(work.chunk_id);
    self.deferred_chunk_rebuilds.complete(work.chunk_id, work.revision);
}
```

Use the existing one-chunk rebuild path:

```rust
world_ops::mesh_generate_chunks(..., vec![chunk_id])
```

This is still synchronous per chunk, but spreads the 8 chunk rebuild across multiple frames.

### 5. Logging

Add focused logs:

```text
[PERF][DEFERRED_REBUILD] chunk UVec3(...) total ...ms remaining ... revision ...
[PERF][DEFERRED_REBUILD_QUEUE] enqueued ... pending ...
```

Keep `[PERF][MESH_REBUILD]` logs for phase breakdown.

## Expected result

Instead of one slider tick blocking for roughly 35 to 45ms, it should usually block for one chunk rebuild per frame, likely around 4 to 8ms depending on chunk content and variance.

If the user drags continuously, pending rebuilds for the same chunks should collapse to the latest version, avoiding wasted old work.

## Risks and notes

- Visual updates may appear chunk-by-chunk over several frames.
- CPU ray cache and GPU scene may be temporarily stale while deferred work drains.
- If one chunk rebuild is still too expensive, add a time budget or move more work behind fences later.
- Do not make boot/loading synchronous rebuilds deferred yet; keep the first implementation scoped to interactive tree GUI replacement.
- The queue utility should not know about Vulkan, builders, fences, or app state.
