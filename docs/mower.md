# Cursor-guided retro mower

- Select **Mower** in the bottom toolbar or press **L**.
- Operate in Orbit Edit (visible/free cursor) mode, at least 0.7 world units from the orbit focus. Free-fly, walking and close-up orbit views cannot place or drive it; the toolbar icon dims outside the allowed view.
- Left-click supported voxel terrain to place the session's mower. Hold the button and drag to guide it. After release, grab it near the deck and drag again. Clicking elsewhere does not teleport it.
- The pointer is a destination. Planar travel is capped at 30 voxels/second (0.1171875 world units/second). Slow pointer movement is followed without overshooting. Long frames are capped to avoid jumps.
- Release, change tools/modes, enter UI, rotate/pan the camera, or lose window focus to stop. The mower remains visible where it stopped.
- Wheels follow voxel terrain. The mower stops at unsupported gaps and steps/unevenness above four voxels. It cannot drive on an empty model roof; add enough soil first.
- The 22-voxel-wide cutting sweep follows the **actual machine path**, not the pointer path. It uses the shared flora trim operation, reducing grass growth and clamping authored flower/plant growth to at most 160/255. Already-short plants are never enlarged. Plant roots, lifetime identities and seeds survive; terrain, trees and climbing vines are untouched.
- There is one mower per session. Its position is not saved. Plant growth changes use the existing flora/world save behavior.

## Rendering

The immutable conventional triangle model and a 16-byte position/yaw instance are rasterized into the existing color/depth pass, then use the existing scene composition and final post-processing. The mower does not use terrain stamps, voxel occupancy, a UI image or a separate post-processing effect. Like the restaurant fixed-model shading, this initial prop uses authored stepped sun/sky lighting; it does not add a new DDGI/shadow occluder.

## Validation

Pure unit tests cover bounded pursuit, slow tracking, frame-rate independence, invalid/long frames, model winding/grounding and authored-plant trimming without identity changes.

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_MOWER_VALIDATE=1 cargo run --release -- --hidden --mute \
  --screenshot player-default target/mower.png --screenshot-delay 2 --auto-exit 6
cargo run --release -- --tail-latest-log 200
```

The opt-in fixture runs on default generated terrain, prepares a grass patch, drives through real pointer-placement/update methods, reads flora/terrain back and checks speed, shortened growth, unchanged plant counts/terrain, release stopping and close-view rejection. It leaves the model in a diagnostic crop for the screenshot. Look for `[MOWER][CHECK]` and inspect the run log for errors. Do not combine this fixture with a saved world or another fixed scene.
