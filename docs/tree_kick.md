# Walking tree kick

Walk near visible tree wood and aim the center reticle at it. `K · Kick` appears below the reticle
while in reach and ready; press **K** once. Holding K does not auto-repeat. Editing/free-flight,
GUI keyboard/pointer ownership, camera transitions, stale scene answers and out-of-reach hits
cannot kick. Cooldown is player-wide. This action shakes the tree; it does not damage wood,
change physical collision, deliberately harvest fruit, or add a foot animation.

## Saved tuning

Debug → **Tree Kick** groups all controls by the interaction concern. Use the normal Debug **Save**:

| Setting | Default | Range |
|---|---:|---:|
| Tree kick impulse | 1.6 rad/s | 0–4 |
| Tree kick shake speed | 3 Hz | 1–7 |
| Tree kick damping | 0.22 | 0.1–0.9 |
| Tree kick reach | 44 voxels | 16–100 |
| Tree kick cooldown | 0.35 s | 0.15–1.5 |

Higher impulse is stronger; higher frequency is faster; lower damping rings longer. Frequency and
damping apply live to the transient response. Zero impulse disables the action. 256 terrain voxels
are one garden world unit. Old saves acquire missing declarations in memory, without being rewritten
on Load; existing values survive Save/reload. Kick response itself is transient, not a saved tree fact.

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
- Existing frame-owned pick, posed rest-space edits, leaf response, replacement/removal, Conifer,
  lighting and exact rollback smoke remain green.
- An intermediate smoke exposed a shader binding collision with imported `wind_field_info` at
  binding 4; root input now uses binding 5. That failing Vulkan run is excluded from validation.

Manual walking/K prompt and shake feel are not yet visually reviewed. The GPU smoke verifies the
nonzero response and rendering/collision correctness, not an actual OS keyboard-driven interaction.
Full repository test-suite success and performance acceptance are not claimed. No visible game was
automatically launched; the Release binary in this worktree is ready for user review.
