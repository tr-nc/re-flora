# Visual leaves and static GPU input publication

The user explicitly retired individual canopy-leaf detachment, weakening, empty sockets and
regrowth. Canopy leaves remain attached (except authored tree replacement/removal); wind response,
flutter, shading and shadow motion remain GPU-driven. Falling leaves are independent, bounded
visual particles sampled from canopy-cluster emitters, not transferred canopy identities.

## Step 1: retire real leaf lifecycle

Removed CPU canopy state, per-frame detachment/growth scans and uploads, precise GPU-to-flight
handoffs, associated shader pipeline, ecology missing-socket rules and leaf-coverage audio scaling.
Removed the four saved lifecycle controls; old saves load without implicit disk writes and omit
retired controls on explicit Save. Structural canopy/audio generation and tree transactions remain.
Existing decorative emission resumes; its small active particle pool is not a state per attached
leaf, and cannot empty the canopy or grow capacity to accommodate canopy-sized detachments.

Baseline: `target/tree-scaling-leaf-baseline-01`, two uncontended Release runs, size-16 six-tree
workload with per-count GPU validation: **22.113 / 21.860 ms**. Saved executable:
`target/tree-scaling-before-visual-leaves`. The geometry/settings are identical to the previous
repair workload. VFX intentionally changes from true detachment to decorative emission.

Validation so far: lifecycle/wind saved-config migration regressions; 61 particle/emitter tests;
42 vegetation transaction tests; full hidden Garden Tree/Conifer raster smoke and exact rejected
placement rollback; shader/Rust check and Release build. Shader inventory decreases 133 → 132.
Generated config/ABI changes come only from check/build. No whole-suite green claim.

Step-1 replay `target/tree-scaling-visual-only-01` passes twice without contention, errors or
cliffs: six-tree **21.066 / 21.150 ms**. This is only a roughly 1 ms improvement, not a claim that
all remaining CPU preparation disappeared. Hidden short launch and audio adapter tests also pass.

## Step 2: persistent immutable leaf input layout

Each tree publishes its compact leaf stream as immutable `Arc<[TreeLeafInstance]>`.
The response layout checks only per-tree identity, exact placement and Arc identity plus the
preceding grass/flower state count. Holding the Arc prevents allocator-address recycling from
being mistaken for equality. No per-leaf scan, HashMap rebuild or leaf-input upload occurs for
unchanged frames. Tree changes assemble new offsets and previous-GPU-state mappings once.

Inputs now reside in device-local storage buffers. Managed transfer copies refresh only the
small grass/flower prefix each frame; the leaf suffix uploads once per layout revision per
in-flight frame bank. The staging capacity is explicitly included in the logged allocation
size (288 bytes/state + 32-byte header). All new allocations are fallible; integer sizing is
checked. Published GPU states remain ping-ponged and continuous; reset/remap/direct-self-index
are explicit shader modes. The push-constant ABI remains **128 bytes**, including the new modes,
rather than raising the device minimum. The CPU cache retains no motion, growth or lifetime.

Opt-in layout/upload logs in both replay runs show **six rebuilds/uploads** through six placements,
**nine** through nine placements (one frame bank on this configuration), rather than per-frame
uploads; every logged input buffer is `DEVICE_LOCAL`.

| Same size-16 six-tree Release scene | Run 1 | Run 2 |
|---|---:|---:|
| Before retirement | 22.113 ms | 21.860 ms |
| Lifecycle retired only | 21.066 ms | 21.150 ms |
| Static input layout + staged GPU residency | **17.190 ms** | **17.100 ms** |
| CPU command recording, before | 10.072 ms | 9.951 ms |
| CPU command recording, final | **1.666 ms** | **1.668 ms** |
| Nested CPU prepass recording, before | 8.927 ms | 8.822 ms |
| Nested CPU prepass recording, final | **0.591 ms** | **0.584 ms** |
| True GPU frame, before | 8.392 ms | 8.099 ms |
| True GPU frame, final | 7.213 ms | 7.300 ms |

Reports: `target/tree-scaling-static-leaves-01` and earlier paths above. Scope medians use
matching settled sample frame IDs; nested scopes are not summed. This isolates the large
remaining prepass cost to the removed leaf preparation work; it does not attribute it to biology.
FIFO presentation now limits this scene around 58 FPS; GPU solver still takes ~0.10 ms and was
not removed. A nine-tree, two-run correctness/scaling replay
`target/tree-scaling-static-leaves-nine-01` gives **16.995 / 17.075 ms** (GPU **8.743 / 8.434 ms**).
No new controlled nine-tree before/after claim: the attempt
`target/tree-scaling-leaf-baseline-nine-01` fails at startup because the old executable requires
the now-retired GUI fields. That failed run is excluded, not timed as evidence.

Validation: 9 vegetation-response logic/ABI tests; direct GPU bit-exact comparison against the
former scalar remapping through unchanged frames, prefix movement, removal/reordering, new
leaf and history reset; full real-GPU response acceptance plus complete raster Garden/Conifer
smoke, restoration and exact transaction rollback. Config-loader subset has 40 passes and one
failure in untouched `older_saves_get_disabled_supersampling_and_saved_choices_survive_reload`
(expected 0, actual 1); both changed retirement migrations pass. The previously failing full
suite remains unclaimed. Falling-leaf pose/optics tests pass; manual appearance is not reviewed.

Final rebuild/replay `target/tree-scaling-static-leaves-final-01` repeats the result at
**17.160 / 17.132 ms**, with complete per-count GPU validation and no errors/cliffs/contention.
Final checks: formatting/check, 9 response tests, 61 particle tests, 42 vegetation tests,
15 raster tests, full GPU response/raster/rollback smoke and a short hidden launch with its log
inspected. Release binary is rebuilt in this worktree. Generated GPU structs were regenerated
by check/build; no generated file was edited by hand.
