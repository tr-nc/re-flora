# Voxel stone lantern

Select **Lantern** at the end of the normal toolbar and left-click reasonably
level ground. Placement uses the captured, revision-checked terrain pick, rejects
tree hits, steep surfaces, world-edge clipping and occupied stone/window space.
The two footing courses may embed in existing soil/rock on a gentle grade; other
materials are not overwritten. There is no separate hidden raster prop.

The original procedural recipe in `src/app/core/stone_lantern.rs` builds a stepped
plinth, pedestal, open four-window chamber, pagoda-like roof, finial and small
lamp stand. Fifteen stone parts contain **9,847 Rock voxels** and one luminous
core contains **150 Emissive voxels**. The total bounding size is 29×49×29 voxels.
No imported/generated image or external model is used.

Both materials go through `WorldEditTransaction` and the ordinary Visible Terrain
Publication owner. Terrain meshing, grass reconciliation, CPU query/collision
caches, emissive-voxel lighting, shadow invalidation, collider scheduling and
snapshot persistence therefore see the same terrain. CPU query caches remain
asynchronous in normal gameplay; only the opt-in review waits for them. The
existing Edit tool can carve or remove the lantern, including its light core.
The light is currently always lit, using existing emissive material controls and
the global light budget. Construction resource pricing is not implemented here.

## Evidence

- `cargo test lantern`: three tests (recipe/window/core, obstruction/footing,
  actual toolbar/discrete-pointer mapping). Five text-toolbar tests also pass.
- `cargo check`, formatting and Release build passed.
- Native fixture verifies every authored atlas cell, an open window and a roof
  ray against the settled CPU physical terrain. The ordinary terrain edit path
  removes all 150 core cells.
- Saved and loaded the actual binary terrain snapshot. Reload reproduced all
  9,847 stone and 150 emissive cells, open windows and the roof ray.
- Inspected day/night screenshots: `target/garden-lantern-day.png` and
  `target/garden-lantern-night.png`; the latter comes from the saved-world load.
- Final placement/load/edit logs had no ERROR, panic or VUID. Early fixture
  attempts correctly rejected uneven footing; the foundation clearance contract
  was corrected, and the CPU ray check now waits for its asynchronous dependency.
- Walking into the object has not been manually playtested. The physical query
  and normal collision publication path were checked, not human movement feel.

Reproduction (use `env -u WAYLAND_DISPLAY` on this machine if needed):

```sh
RE_FLORA_LANTERN_REVIEW=1 cargo run --release -- --hidden --mute \
  --terrain-save target/garden-lantern.rflterrain \
  --screenshot garden-arrival target/garden-lantern-day.png --screenshot-delay 2 --auto-exit 3
RE_FLORA_LANTERN_REVIEW=load RE_FLORA_NIGHT_REVIEW=lantern cargo run --release -- --hidden --mute \
  --terrain-load target/garden-lantern.rflterrain \
  --screenshot garden-arrival target/garden-lantern-night.png --screenshot-delay 3 --auto-exit 4
RE_FLORA_LANTERN_REVIEW=edit cargo run --release -- --hidden --mute --auto-exit 1
```

Review environment variables are unsaved; only the explicit save command writes
the isolated snapshot. No visible game is launched automatically.
