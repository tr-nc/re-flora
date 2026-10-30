# Async grass: speculative work, atomic publication

Status: feature-branch decision; does not authorize asynchronous terrain or paid planting.

The synchronous brush API retains its “return means complete” contract. Only interactive,
unlimited Grass Mix input accepts a deferred request. Scripted fixtures, finite inventory,
removal, trim and terrain operations must not silently inherit deferred semantics.
The saved Debug → Brush Editing checkbox defaults to unchecked (synchronous). It gates only
interactive admission; disabling drains accepted paint at the next frame boundary or before
another synchronous edit. It must not discard accepted endpoints or change stroke cadence.

## Ownership constraints

- The application owns one accepted request and its bounded retries. Do not replace a queued
  endpoint: drain before accepting another request or before an explicitly ordered edit/save.
  Shutdown may cancel unpublished unlimited paint, but must drain GPU residency.
- GPU recording has one shared implementation for synchronous and asynchronous paths.
  Async jobs own private uniforms, input instance/growth snapshots, output and scratch images.
  A version check is **not** permission to race CPU writes with GPU reads of live buffers.
- Material validity comes from the existing complete **Visible Terrain Publication** revision,
  supplied at submission and publication. Submit between world publications, on the same
  ordered general queue as material writes. Do not substitute the atlas's generic write count:
  moisture and soil-cover writes are not changes to planting material/geometry.
- Instance and growth-potential validity require allocation identity, content version and live
  counts. Successful mapped writes and submitted GPU writes advance buffer versions; abandoned
  recordings and host-read declarations do not. This is conflict detection, not synchronization.
- Complete every chunk and all fallible readback/hydration preparation before swapping any live
  output. Hydration comes from current live stores keyed by root, not the submission snapshot.
  The CPU-readable grass stream is copied in the shared GPU edit submission and swapped with
  its draw stream at publication; retaining the old readback would mix generations. Snapshot
  CPU-authored growth potential from its retained CPU publication, not mapped uncached VRAM.
  Retire old draw/hydration buffers through the frame owner; GPU completion alone does not prove
  that an old rendered frame has stopped using them.

A single request is limited to eight chunks; larger brushes use the synchronous path. Two
optimistic retries bound conflict churn before synchronous completion. These bounds avoid an
unbounded request/resource backlog without changing stroke interpolation or dropping accepted
work. The private 256³ R32UI surface/occupancy pair costs 128 MiB, plus bounded per-chunk buffers.
This memory and snapshot-copy cost is intentional isolation, not a free performance win.

## Deliberately separate decisions

Removing a wait does not reduce GPU work or establish input-to-display improvement. Measure
submit and finish CPU costs together, publication latency and frame/presentation behavior.
First-use allocation and synchronous conflict/backpressure paths remain relevant.

Terrain async requires a separate coherent atlas/surface/contree/picking/physics/lighting
publication contract. Paid planting additionally needs reservation/settlement ownership.
Neither follows merely by adding another mode to the grass transaction.
