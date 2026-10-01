# Cursor-guided retro mower

- Select **Mower** in the bottom toolbar or press **L**.
- Operate in Orbit Edit (visible/free cursor) mode, at least 0.7 world units from the orbit focus. Free-fly, walking and close-up orbit views cannot place or drive it; the toolbar icon dims outside the allowed view.
- Left-click a supported walkable surface (voxel terrain, an empty model roof, sidewalk or road) to place the session's mower. Hold the button and drag to guide it. After release, grab it near the deck and drag again. Clicking elsewhere does not teleport it.
- The pointer is a destination. Planar travel is capped at 30 voxels/second (0.1171875 world units/second). Slow pointer movement is followed without overshooting. Long frames are capped to avoid jumps.
- Release, change tools/modes, enter UI, rotate/pan the camera, or lose window focus to stop. The mower remains visible where it stopped.
- Movement and picking use the player's static collision world, including fixed triangle models. The shared capsule solver handles slopes, sliding, small steps and ground snapping. The mower has its own body dimensions, but the same single-step limit (16 voxels) and maximum climb angle (60°) as walking. Tall walls and unsupported drops remain blocked. No soil is required to drive on a bare roof or road.
- The 22-voxel-wide cutting sweep follows the **actual machine path**, not the pointer path. It uses the shared flora trim operation, reducing grass growth and clamping authored flower/plant growth to at most 160/255. Already-short plants are never enlarged. Plant roots, lifetime identities and seeds survive; terrain, trees and climbing vines are untouched.
- There is one mower per session. Its position is not saved. Plant growth changes use the existing flora/world save behavior.

## Shared movement

`src/app/core/physics/surface_motion.rs` owns world/voxel unit conversion and capsule profiles. Player walking and mower movement both call Rapier's existing character solver through this seam; player velocity, gravity and camera behavior remain unchanged. The mower uses its own capsule dimensions and keeps planar cursor pursuit speed-limited. Its picking ray uses the same static query broad-phase as movement, not the editable-soil bounds or Contree-only terrain picking. Dynamic fruit and sensors are excluded.

Flora cutting remains a separate concern: empty model floors do not trigger flora rebuilds, and actual voxel planting/growth rules are unchanged.

## Rendering

The immutable conventional triangle model and a 16-byte position/yaw instance are rasterized into the existing color/depth pass, then use the existing scene composition and final post-processing. The mower does not use terrain stamps, voxel occupancy, a UI image or a separate post-processing effect. Like the restaurant fixed-model shading, this initial prop uses authored stepped sun/sky lighting; it does not add a new DDGI/shadow occluder.

## Validation

Deterministic tests cover bounded pursuit, slow tracking, frame-rate independence, invalid/long frames, model winding/grounding and authored-plant trimming without identity changes. Shared-movement regression fixtures cover fixed roofs/roads, a five-voxel bump, real voxel bumps/stair slopes, tall-wall rejection and unsupported gaps. The physics crate's player movement suite covers the unchanged walking solver.

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

The opt-in fixture runs on default generated terrain, prepares a grass patch, drives through real pointer-placement/update methods, reads flora/terrain back and checks speed, shortened growth, unchanged plant counts/terrain, release stopping and close-view rejection. It leaves the model in a diagnostic crop for the screenshot. Look for `[MOWER][CHECK]` and inspect the run log for errors. Do not combine this fixture with a saved world or another fixed scene.

The separate `RE_FLORA_MOWER_SURFACE_VALIDATE` fixture requires `--rooftop-poc` with no soil. It uses actual pointer placement and per-frame pursuit to drive on the bare roof and the road outside the voxel-editing bounds. Look for `[MOWER][SURFACE_CHECK]` and absence of errors. Before the shared-movement fix, this fixture failed with `bare fixed-model roof rejected mower placement`.
