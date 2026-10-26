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

## Fixed half-size placement (2026-10-27 follow-up)

The player approved **0.5×** and requested removal of the size slider.
All new lanterns now use that fixed scale: **15×25×15 voxels**, with 1,381 Rock
and 27 Emissive cells. Existing terrain lanterns retain their saved size.
There is no remaining Debug size control; old saved slider values are discarded
on load and removed on explicit Save, without rewriting files during loading.
The separate default-on instant-mature tree checkbox remains available.

The normal placement, clearance, collision, lighting and terrain-save paths are
unchanged. Native review modes `1`, `load` and `edit` now exercise the fixed
half-size lantern; `small`/`load-small` remain compatible aliases.
`load-original` checks an existing historical 1× fixture without resizing it.

Follow-up checks passed: three lantern tests, legacy-setting retirement/tree-toggle
persistence and exhaustive generic Save tests; formatting, cargo check and Release
build. Native default placement/save/reload confirms 1,381 stone/27 emissive cells,
open window and physical roof hit. Native editing removes all 27 core cells;
ordinary hidden/muted smoke and run-log inspection show no ERROR/panic/VUID and
shutdown `failures=0`. Full-suite and subjective acceptance were not rerun.

## Historical size preset (introduced then removed on 2026-10-27)

**R → Construction Placement → Stone lantern scale (new lanterns)**, or search
**new lanterns**. Range **0.5–2**, default **1** preserves the previous size.
Debug Save persists the preset; it applies to subsequent placements until changed.
Already placed lanterns are ordinary stored terrain and are never resized.

The scale is sampled once into a placement plan. All cuboid boundaries, the
foundation allowance and empty window clearance are scaled together and rounded
to voxel boundaries. Shared boundaries stay shared; the minimum 0.5 retains
nonempty pillars/core and four open windows. This is voxel-quantized resizing,
not smooth subvoxel scaling. Invalid/nonfinite sizes are rejected before editing.

Follow-up verification: three lantern unit tests cover seven representative
scales, window obstruction, footprint limits and invalid inputs; old-config
migration and the exhaustive generic settings save/reload test pass. Hidden
Release `RE_FLORA_LANTERN_REVIEW=small` and `load-small` runs saved/reloaded
the actual half-size terrain lantern: **1,381 Rock / 27 Emissive voxels**, open
window and physical roof ray verified. Half size has a 15×25×15 voxel bound.
At that stage the review's `1`/`load` modes retained the original 1× fixture;
the fixed-size follow-up above supersedes those mode meanings.
Inspected `target/lantern-size-control.png`; check/Release/hidden smoke pass.
No manual visual approval or new performance claim; no world-save format changes.

## Original-size evidence

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

Current reproduction uses the fixed half-size lantern (use `env -u WAYLAND_DISPLAY`
on this machine if needed); the original-size screenshots above remain historical:

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
