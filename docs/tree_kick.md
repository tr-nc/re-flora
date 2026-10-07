# Walking tree kick

Walk near visible tree wood and aim the center reticle at it. `K · Kick` appears below the reticle
while in reach and ready; press **K** once. Holding K does not auto-repeat. Editing/free-flight,
GUI keyboard/pointer ownership, camera transitions, stale scene answers and out-of-reach hits
cannot kick. Cooldown is player-wide. Each accepted kick shakes that tree and requests one
burst of decorative falling leaves plus a fraction of its remaining mature fruit. It does not
damage wood, move physical tree collision, or add a foot animation.

## Saved tuning

Debug → **Tree Kick** groups all controls by the interaction concern. Use the normal Debug **Save**:

| Setting | Default | Range |
|---|---:|---:|
| Tree kick impulse | 0.2 rad/s | 0–4 |
| Tree kick shake speed | 2.55 Hz | 1–7 |
| Tree kick damping | 0.3 | 0.1–0.9 |
| Tree kick reach | 30 voxels | 16–100 |
| Tree kick cooldown | 0.35 s | 0.15–1.5 |
| Fruit release fraction | 0.25 | 0–1 |
| Decorative leaves per burst | 24 | 0–128 |

Higher impulse is stronger; higher frequency is faster; lower damping rings longer. Frequency and
damping apply live to the transient response. Zero impulse disables the action. 256 terrain voxels
are one garden world unit. Old saves acquire missing declarations in memory, without being rewritten
on Load; existing values survive Save/reload. Kick response itself is transient, not a saved tree fact.

## One-shot leaf and fruit release

Mature fruit remains attached at the end of the saved **Fruit Growth (held until kick)** control.
Registration, loading, advancing the growth control, wind and elapsed time cannot schedule a drop.
Only an accepted kick selects a subset of still-held, fully-grown fruit on its exact tree. Selection
rounds up (a nonzero fraction releases at least one), is capped at 256 fruits per kick, and never
selects a pending or already-dropped fruit twice. Zero fraction disables fruit release; zero leaf
count disables the leaf burst. Remaining canopy geometry is unchanged: falling leaves are the
existing independent decorative VFX, not a revived detachment/regrowth lifecycle. Ambient
wind-driven decorative leaves are unchanged.

Fruit stays visibly attached until collision along its fall path is ready and an asynchronous
handoff completes. Normal rendering copies only four published held-response float4 corners
(**64 bytes/fruit**) into frame-owned readback. The same frame's small attachment transforms,
pendulum geometry and velocities give its rigid body's starting pose. The acquisition seam
collects completed readbacks before fence reuse; buffer tracking keeps producers resident.
There is no dedicated kick/drop GPU submission or wait and no full-field readback. A changed tree
publication rejects an old handoff; changed fruit specs cancel pending release rather than dropping
a replacement crop. Falling bodies then use the existing gravity/contact/rendering path.

Dropped fruit is consumed for the current crop; it does not automatically respawn with time. The
explicit Debug growth scrub below that fruit's growth start rearms it. Saved world loads restore a
held crop; loose bodies and harvested state remain transient, as before. Opt-in drop/visual fixtures
now request explicit diagnostic kicks instead of relying on growth thresholds.

## Ownership and rendering

The prompt uses the existing nonblocking GPU hover. A press captures a fresh ray/settings and asks
for a frame-owned `Kick` pick: it never blocks for a dedicated query. The current publication's exact
triangle bindings resolve its canonical tree ID in O(3), rather than guessing the closest origin.
Mixed/unbound/out-of-range triangles are rejected. Terrain/model occlusion, scene-version checks,
250 ms intent age, current walking/reach checks and context cancellation apply before the impulse.
A quickly changed camera direction immediately hides old hover prompts. Only one kick is pending.

Each canonical `TreePose` owns one tiny damped root oscillator. It is stepped analytically on CPU
once per tree, then supplies a **32-byte root transform** to the existing GPU hierarchy job. The GPU
composes that anchored root transform with the original branch wind hierarchy, and all skinning,
normal/BVH updates and attachment/canopy consumers use the ordinary shared pose publication. It
never adds per-vertex CPU deformation, a new per-kick GPU wait, a tree publication or collider refit.
An impulse does not advance the skeleton's topology/version or reset the whole GPU solver allocation.
Oscillator energy bounds accumulated tilt to 0.2 rad, even under repeated kicks. Replacement creates
a fresh transient response. Wind off resets ambient branch deflection but does not suppress a kick.

## Validation and limits

- `cargo fmt --check`, `cargo check`, Release build and hidden muted launch/log inspection.
- Oscillator tests cover away/rebound/settle, frame-rate equivalence, repeated-impulse bounds and
  invalid/zero inputs; pose tests cover root anchoring, wind-off motion and generation preservation.
- Tests cover checked triangle ownership, reach/facing, migration without implicit disk writes,
  generic saved-field roundtrips, and single ownership of each Debug control.
- Hidden raster smoke injects an explicit impulse with wind off. GPU hierarchy matches its CPU
  oracle; actual wood moves while authored rest geometry and physical collision remain unchanged:
  `[TREE_KICK][GPU] passed wind_off=true moved=true rest_collision=true rest_geometry=true`.
  The diagnostic explicitly advances its transient response to rest before the unrelated wind-toggle
  fixtures; production input never performs that seek.
- Fruit tests (16), particle/app burst tests (7), kick migration, unified Save and Debug grouping
  pass. The Release raster smoke starts with **17/17 fruit held**, releases **5** and retains **12**
  from one kick, emits **24** leaves, and records a **320-byte** async handoff with
  `frame_fence=true query_wait=false`. Existing frame-owned pick, posed rest-space edits, leaf
  response, replacement/removal, Conifer, lighting and exact rollback smoke remain green.
  Evidence: `/tmp/tree-kick-release-smoke-02.out` and
  `target/re-flora-logs/re-flora-20261007-220949.345-2253812.log`.
- An intermediate smoke exposed a shader binding collision with imported `wind_field_info` at
  binding 4; root input now uses binding 5. That failing Vulkan run is excluded from validation.

Manual walking/K prompt and shake feel are not yet visually reviewed. The GPU smoke verifies the
nonzero response and rendering/collision correctness, not an actual OS keyboard-driven interaction.
Full repository test-suite success and performance acceptance are not claimed. No visible game was
automatically launched; the Release binary in this worktree is ready for user review.
