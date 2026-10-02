# Cursor-guided retro mower

- Select **Mower** in the bottom toolbar or press **L**.
- Operate in Orbit Edit (visible/free cursor) mode after the camera transition finishes. There is no separate mower zoom-distance threshold: returning from walking enables it immediately. Free-fly, walking and camera transitions cannot place or drive it; the toolbar icon dims outside the allowed mode.
- Each left-button press spawns a fresh temporary mower at the pointed walkable surface (voxel terrain, an empty model roof, sidewalk or road). Hold and drag to guide it. Release destroys it immediately; the next press starts a new mower at the new pointer position. There is no persistent mower to grab or relocate.
- The pointer is a destination. Planar travel is capped at 30 voxels/second (0.1171875 world units/second). Rotation is capped at 120°/second along the shortest quaternion arc, including heading, pitch and roll. Travel follows the chassis heading and slows while turning; sudden reversals turn the machine before it can drive the other way. Near the destination it progressively slows instead of projecting the entire remaining distance in one frame. Within 0.25 voxel it parks and preserves its arrived heading; the held pointer must move over 0.5 voxel from that arrival target to resume. This small hysteresis prevents picking/physics roundoff from causing endless steering. Slope alignment can finish against a cached support frame without repeatedly changing the idle target. Long frames are capped to avoid jumps.
- Release, change tools/modes, enter UI, rotate/pan the camera, leave the window or lose window focus to cancel and destroy the temporary mower.
- Movement and picking use the player's static collision world, including fixed triangle models. The shared capsule solver handles slopes, sliding, small steps and ground snapping. The mower has its own body dimensions, but the same single-step limit (16 voxels) and maximum climb angle (60°) as walking. Tall walls and unsupported drops remain blocked. No soil is required to drive on a bare roof or road.
- The 22-voxel-wide cutting sweep follows the **actual machine path**, not the pointer path. It uses the shared flora trim operation, reducing grass growth and clamping authored flower/plant growth to at most 160/255. Already-short plants are never enlarged. Plant roots, lifetime identities and seeds survive; terrain, trees and climbing vines are untouched.
- There is at most one mower, only for the duration of a pointer hold. Plant growth changes use the existing flora/world save behavior.

## Shared movement

`src/app/core/physics/surface_motion.rs` owns world/voxel unit conversion and capsule profiles. Player walking and mower movement both call Rapier's existing character solver through this seam; player velocity, gravity and camera behavior remain unchanged. The mower uses its own capsule dimensions and keeps planar cursor pursuit speed-limited. Its picking ray uses the same static query broad-phase as movement, not the editable-soil bounds or Contree-only terrain picking. Dynamic fruit and sensors are excluded.

The four wheel contacts query this same collision world to fit a support plane. This produces a stable slope normal over voxel stairs rather than snapping to each vertical/horizontal voxel face. The render quaternion tilts model geometry and shading normals together. Wheel clearance controls the render origin independently of the upright collision capsule, so the model follows slopes without changing the proven player collision solver. Missing/steep support retains the previous tilt instead of inventing an unstable normal.

Flora cutting remains a separate concern: empty model floors do not trigger flora rebuilds, and actual voxel planting/growth rules are unchanged.

## Rendering

The immutable conventional triangle model and a 28-byte position/quaternion instance are rasterized into the existing color/depth pass, then use the existing scene composition and final post-processing. The mower does not use terrain stamps, voxel occupancy, a UI image or a separate post-processing effect. Like the restaurant fixed-model shading, this initial prop uses authored stepped sun/sky lighting; it does not add a new DDGI/shadow occluder.

## Validation

Deterministic tests cover bounded pursuit, slow tracking, frame-rate independence, invalid/long frames, model winding/grounding and authored-plant trimming without identity changes. Multi-frame guidance regressions cover a short drag followed by a stationary held pointer, exact idle stability, pointer jitter rejection and resuming after a new destination. Shared-movement regression fixtures cover fixed roofs/roads, a five-voxel bump, real voxel bumps/stair slopes, tall-wall rejection and unsupported gaps. The physics crate's player movement suite covers the unchanged walking solver.

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_MOWER_VALIDATE=1 cargo run --release -- --hidden --mute \
  --screenshot player-default target/mower.png --screenshot-delay 2 --auto-exit 6
cargo run --release -- --tail-latest-log 200
RE_FLORA_MOWER_SURFACE_VALIDATE=1 cargo run --release -- --rooftop-poc \
  --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

The opt-in fixture runs on default generated terrain, prepares a grass patch, drives through real pointer-placement/update methods, reads flora/terrain back and checks travel/turn speed, shortened growth, unchanged plant counts/terrain, release destruction, fresh-press respawning, walking/transition rejection and actual pointer placement after returning from walking to the edit camera. After checking release/destruction, it spawns and holds a fresh mower in a diagnostic crop for the screenshot. Look for `[MOWER][CHECK]` and inspect the run log for errors. Do not combine this fixture with a saved world or another fixed scene.

The separate `RE_FLORA_MOWER_SURFACE_VALIDATE` fixture requires `--rooftop-poc` with no soil. It uses actual pointer placement and per-frame pursuit to drive on the bare roof and the road outside the voxel-editing bounds. It also runs a tiny-drag stationary-hold check through actual pointer picking, collision movement and render-pose updates, then verifies that moving the still-held pointer resumes movement. Look for `[MOWER][SURFACE_CHECK]`, `[MOWER][HOLD_CHECK]` and absence of errors. Before the shared-movement fix, this fixture failed with `bare fixed-model roof rejected mower placement`.
