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

Static response-input reuse is a separate next step. Step 1 alone does not remove the current
per-frame identity remapping; its CPU/GPU simulation state must remain coherent through tree
publication, removal, replacement, restoration and live wind tuning.
