# Fresh Restorative Garden

An ordinary fresh launch now establishes a small editable garden, rather than the legacy
noise terrain and tuning tree. No GUI lighting, quality, camera library or saved-world
preference is rewritten.

## Startup ownership

| Launch | Terrain and vegetation |
| --- | --- |
| Ordinary fresh launch | Version 3 garden, seed `20260519` |
| `--garden-seed <u32>` | Same composition recipe with independently addressed individual variation |
| `--legacy-terrain` | Original procedural terrain and startup tuning tree |
| `--flat-terrain` | Existing flat soil debugging path |
| `--terrain-load <path>` | Saved layout/identities/growth win, **even when empty**; retired Test Trees migrate in place to Apple |
| Fixed scenes, connectivity/lighting/canopy/foliage/denoiser fixtures | Existing scene ownership, not the garden recipe |
| Opt-in `RE_FLORA_*REVIEW*`, `*BENCH*`, `*SMOKE*`, `*VALIDATE*` harnesses | Existing fixture path |

`--garden-seed` and `--legacy-terrain` reject loads and fixed scenes instead of silently
ignoring the request. Tree/flora benchmarks also retain their existing startup. An explicit
camera snapshot or screenshot owns the pose: the garden never ground-snaps an unrelated
requested snapshot. With no explicit camera, arrival enters collision-enabled walking at
realized ground height. Default snapshot filenames remain operation targets, not automatic loads.

The garden is generated only once, before startup saving. Runtime terrain publication,
planting, mowing and loading do not refill it. Save using the ordinary Terrain & Plants
controls or `--terrain-save <temporary-path>`. The existing snapshot format is unchanged;
loaded saves contain realized geometry, not an instruction to rerun the recipe. Retired
`garden_tree` / missing-species descriptions are converted separately: their baked wood is replaced
transactionally by current Apple geometry. This keeps position, ID, size, seed and global age;
legacy wood edits are replaced along with that morphology. The source file is unchanged until an
explicit Save. Other species do not migrate.

## Recipe and limits

`src/garden_layout.rs` owns version/seed, broad unequal analytic rises, off-center arrival,
a connected curved route and second planting pocket, four tree anchors, five elongated
flower lobes and sparse outliers. `garden_opening.rs` realizes these via normal atlas soil,
Grow occupancy, tree placement, authored flower placement and trim/removal transactions.
Dirt has a 30-voxel upper layer over rock; there are no protected scene bits.

- Four deterministic **Apple Trees** use the current shared Apple architecture and fruit lifecycle
  at the original four anchors, with the original individual seed mapping. Version 3 uses the same
  human-scale Apple defaults as new planting: size `5.8`, level-3 leaf sprays, density `0.12`, and
  fruit hanging offset `6 ± 1.5` voxels. The default opening measures about **1.54–1.74 player-body
  heights**, instead of version 2's 2.60–2.83. Bounded footprint fitting still limits generated wood
  and leaves to a 46-voxel radius (the current default crowns measure about 26–27 voxels).
  They do not inherit the user's tuning-tree descriptor or age. Route/border tests and real capsule
  traversal verify clearance. There is no fruit-disable override or opening-only branch recipe.
  Version 3 changes new tree authoring only; the independent flower hash salt remains unchanged.
  Existing saved Apple trees are not resized. See [scale evidence](evidence/apple-tree-player-scale.md).
- At most 180 authored flowers, 2,304 candidate attempts, nine-voxel anchor separation,
  valid realized soil columns, stable species keys (`wild-geranium`, `forget-me-not`,
  `oxeye-daisy`) and explicit clustering/outlier hashes. Rejection terminates even when
  every candidate is invalid. Initial growth is 210–255/255, not seed-only growth.
- Ordinary short grass is established once, with three taller mixed patches. It uses
  the existing occupancy density and per-chunk/per-species allocator limits (40,000),
  not a new per-frame scattering system. The original version-1 accepted snapshot contained
  **130,010 ordinary grass instances plus 180 flowers**, and four trees. Grass totals can change
  with the trees' republished footprint; the version-3 flower fingerprint remains `c285dfeb`. This is not a
  promise of 180 total vegetation instances; grass has its own existing storage budget.
- Arrival/route/pocket mowing reserves real plantable room; short trimmed margins soften
  the opening. All later player edits use the same data. No new live soil-water or wet
  footprint authority is introduced.

These are artistic initial conditions, not botanical simulation, universal species
friendship/repulsion rules or measured moisture habitats. See the evidence and qualifications
in [the spatial ecology note](research/garden-spatial-ecology.md).

## Hidden review and acceptance

Four built-in presets are listed by `--list-camera-snapshots` without writing the snapshot
library: `garden-arrival`, `garden-corridor`, `garden-reverse`, `garden-overview`.
They also work with a loaded realized snapshot. Scheduled captures with these presets hide
HUD/perf panels but do not change saved visibility preferences.

All builds/tests/native runs must be serialized with other delivery workers:

```bash
flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -ec '
  cargo build --release
  if [ -n "$DISPLAY" ]; then unset WAYLAND_DISPLAY WAYLAND_SOCKET; fi
  RUST_LOG=info cargo run --release -- --hidden --mute \
    --screenshot garden-arrival target/garden-delivery/world/arrival.png \
    --screenshot-delay 3 --auto-exit 5 \
    --terrain-save target/garden-delivery/world/realized.rflterrain
'
```

Do not nest the lock, force a present mode, launch visibly, or use a user save as audit output.
The temporary native audit is explicitly unsaved as a setting:

```bash
flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -ec '
  if [ -n "$DISPLAY" ]; then unset WAYLAND_DISPLAY WAYLAND_SOCKET; fi
  RUST_LOG=info RE_FLORA_GARDEN_AUDIT=target/garden-delivery/world/audit \
    cargo run --release -- --hidden --mute --auto-exit 0.5
'
```

It requires fresh ordinary generation and an output below `target/garden-delivery/world`
(no `..`). It walks all route segments with the real player capsule, digs, plants into the
dug soil, mows the plant, and writes temporary planted/mown/minimal-empty saves. It is not a
saved Debug option and does not alter preferences. Pure tests cover seed determinism,
variation, rejection bounds, spacing, route slope/clearance, mature tree envelopes and the
CLI/launch-owner policy matrix. The existing snapshot smoke verifies exact bytes, flora,
identity/growth and trees at startup and across two runtime replacements.

**全空世界运行时验收（scope v3）：** stage 2 的“添加树后重载全空存档”水发布超时，
现已在水地形权威 owner 修复。完整观察的空域无需伪造 collider 即可 Ready。
原 smoke 对全空、普通、挖掘种植及挖掘割草存档均通过两次精确替换，并断言水 Running、
地形 Ready 与世界更新解冻。全空 startup/resave 仍为零植物零树、无花园生成。
[中文故障报告](reports/empty-world-reload.md) 保留 red/green 证据、已实际复现的旧基线，
以及仍未知的准确引入提交。

See [world delivery evidence](reports/garden-world.md) for actual captures, measurements,
source changes and remaining acceptance boundaries.
