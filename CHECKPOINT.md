# Checkpoint

## Latest implementation

- Updated chunk work ordering so deferred work pops the chunk nearest to the player first, then proceeds outward.
- Added shared nearest-chunk ordering in `src/util/chunk_work_queue.rs` using chunk center distance and deterministic chunk-id tie breaks.
- Reused that ordering in:
  - `src/util/latest_chunk_queue.rs` for latest-revision chunk queues.
  - `src/util/growing_flora_queue.rs` for flora growth ticks.
- Applied nearest-to-player ordering to:
  - deferred chunk rebuilds in `App::process_deferred_chunk_rebuild`.
  - growing flora ticks in `App::update_growing_flora_chunk`.
  - contree GPU-to-CPU chunk cache transfer queue when polled from the app frame loop.

## Findings

- Chunk rebuilds and GPU-to-CPU chunk transfers are managed by separate queues.
- They do not share one queue, but they now share the same nearest-to-player ordering logic.
- Deferred chunk rebuilds are processed from `App` and normally run at most one rebuild per frame.
- GPU-to-CPU transfer/cache jobs are managed inside `ContreeBuilder` and allow only one active transfer/decode job at a time.
- A frame can generally do one transfer/cache submission or progress step and one deferred rebuild, but transfer completion is not guaranteed every frame because it is gated by the GPU copy fence, CPU decode in-flight state, and readback buffer availability.
- Frame order currently polls transfer/cache jobs before processing the deferred rebuild.
- A rebuild can enqueue a transfer/cache job as part of contree rebuilding.

## Open question

Are the transfer and chunk rebuild jobs intended to be completely separately managed per frame, or should they share one strict nearest-to-player sequence across both job types?

Current behavior is separate queues with shared nearest-to-player ordering. That means one frame can potentially make progress on both one transfer/cache job and one rebuild job.

Potential follow-up: transfer jobs submitted immediately from inside rebuild code currently use a fallback focus in internal paths. Queued transfers polled from the app frame loop use the real player position. If we need a fully strict nearest-to-player sequence for every transfer submission, pass the current player focus into the rebuild-triggered transfer enqueue/submit path too.
