# Human-scale Apple Tree defaults

Implemented after baseline `00683cc7`, following the approved
[height research](../research/apple-tree-player-scale.md). This is an intentional change to
**new tree authoring**, not a behavior-preserving generation refactor or a saved-world resize.

## Delivered behavior and ownership

- New Apple Trees use mature, fruit-bearing architecture at a compact garden scale. The
  player capsule, camera height/FOV, growth algorithms, fruit size and lifecycle are unchanged.
- `src/tree_gen/species.rs` owns native planting size and the matched foliage/fruit defaults.
  Apple: size **5.8**, leaf level **3**, density **0.12**, downward fruit offset **6 ± 1.5 voxels**.
  Fruit probability remains **0.45**. These size parameters are not metres.
- `TreeDesc::default()` uses the species' `default_desc()`, and checked-in `[tree.desc]` matches
  the scale-related values. Native/CPU measurements verify both paths; a regression test checks
  fresh configuration uses current defaults while explicitly saved authoring survives round trips.
  The mutable user config is deliberately not frozen to profile values by a unit test.
- Switching varieties uses the destination species' native size, retaining the individual seed.
  Pine, Willow and legacy Conifer retain the former configured size **20**, rather than inheriting
  the compact Apple's 5.8. Matching-species live authoring retains its saved dimensions/settings.
- Fresh garden recipe **version 3** uses the same Apple defaults. Only the original individual
  seed mapping and footprint fitting belong to the opening recipe. All four root positions are
  unchanged. Shape varies by seed; no second Apple architecture or per-opening leaf preset exists.
- Existing saved Apple/Pine/Willow/Conifer descriptions remain authoritative. No new migration,
  trunk restamping, automatic resize or file rewrite is applied to them. Retired Test Trees still
  follow the previously delivered migration, preserving their saved sizes while adopting current
  Apple defaults; they are not forced down to the new planting size.

This separation matters: changing a growth rule globally could disagree with already-baked saved
wood. This change instead authors smaller *new* descriptions and leaves the generator and saved
current descriptions unchanged.

## Measured geometry

`Tree::height_voxels()` measures root-to-crown rest geometry from generated cone AABBs and actual
leaf positions plus a half-voxel leaf envelope. It does not confuse leader height or `size` with
final height. Wind, GPU quantization, fruit and terrain variation are outside this CPU measure.
The current collision body is 24 voxels tall.

| Fresh opening tree | Seed | Previous height, voxels | New height, voxels | New body-height ratio | New crown radius, voxels |
|---|---:|---:|---:|---:|---:|
| 0 | 2539892922 | 62.669 | 40.305 | 1.68 | 25.898 |
| 1 | 2847798192 | 62.329 | 36.912 | 1.54 | 26.187 |
| 2 | 305502193 | 64.065 | 41.757 | 1.74 | 26.592 |
| 3 | 580287596 | 67.991 | 41.001 | 1.71 | 27.245 |

The opening is about **35–41% shorter**, at the same anchors and walking-camera pose. If the
24-voxel player is interpreted as 1.70 m, the new opening corresponds to about **2.61–2.96 m**;
this is an illustrative conversion, not an established world-unit/metre calibration.

Configured planting at seeds **7 / 122 / 907** measures **39.707 / 39.805 / 39.099 voxels**, or
**1.65 / 1.66 / 1.63 body heights**. The previous configured trees were 135.79–139.78 voxels tall.
Across seeds `0..64`, `122`, `907`, and `u64::MAX`, the new default ranges from **35.54 to 44.11
voxels** (about **1.48–1.84 body heights**). The 35–45 voxel regression envelope allows natural
seed and leaf-edge variation; this is not a runtime clamp or a proof over every possible seed.

## Native visual iteration

All captures are actual hidden/muted Release renders, not generated illustrations. Reviewed the
final opening, ground-eye Apple fixtures at seeds 7/122/907, and a mixed Apple/Pine/Willow fixture.

The first compact candidate retained density 0.035 with smaller leaf sprays; its crown looked too
sparse and the apples dominated it. Increasing density to 0.12 restores readable leaf masses
without restoring the oversized level-5 canopy. The final result keeps a visible trunk, mature
branching and fruit; appearance remains subject to the user's visual judgment.

The opt-in, unsaved `RE_FLORA_TREE_SPECIES_REVIEW=apple-walk` fixture uses the ordinary standing
eye height above terrain at a fixed close viewpoint. It reuses real catalog selection, preview,
publication, age rebuild, JSON restoration and GPU pose/surface/lighting validation. It does not
change gameplay cameras. Existing `apple|pine|willow|mixed` modes retain their original view.

Final single-Apple fixtures have **1,615 / 1,568 / 1,627** published leaf instances and
**30 / 37 / 33** fruit sites at seeds 7/122/907. Counts are not visible fruit totals. Mixed Pine and
Willow remain at size 20, with 18,000 / 12,596 leaves and no apples; neither inherited Apple scale.

## Validation

Commands ran from the main worktree, with heavy jobs serialized through
`/tmp/re-flora-garden-delivery-heavy.lock`.

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- Targeted filters (overlap; do not sum as a full suite):
  - `tree_gen`: **47 passed**, one explicit compatibility diagnostic ignored.
  - `garden_layout`: **3**; `vegetation::`: **47**.
  - `gui_config_model`: **5**; `gui_config_loader`: **56**.
  - `plant_catalog`: **2**; `grow_panel`: **3**; `player_tools`: **19**.
  - `tree_gui`: **3**; `every_serialized_custom_leaf`: **1**; `tree_preview_mesh`: **5**.
  - `tree_collision`: **2**; `trunk_only`: **1**; `lateral_wood_stays`: **1**.
- New tests cover mature height across 67 seeds (not a reduced-age substitute), opening scale and
  seed/anchor preservation, species switching, configured/fallback consistency, saved authoring,
  and real compiled fruit sites with whole mature apples clear of flat ground.
- Actual player-capsule route: **212 steps**, all segments reached. Ordinary Dig removed **129**
  dirt voxels; Grow/mow paths passed. **180 flowers**, unchanged fingerprint **`c285dfeb`**.
- New version-3 save and the existing version-2 Apple save each passed startup plus **two runtime
  loads** with intervening tree placement: exact terrain bytes, flora and tree data, no duplicates,
  water `Running`, terrain persistence `Ready`, and world updates resumed.
- The actual retired Test Tree fixture also passed startup plus two runtime loads. Each migration
  review found **15,422** tree-wood-only changed voxels and **1,151** old-only interior probes with
  **zero old wood ghosts**. This is bounded migration evidence, not exhaustive voxelizer equivalence.
- Native raster smoke passed thin wood, held/partially dropped fruit and kick/wind overlays,
  surface/lighting, age/stiffness, remove/replace, mixed Conifer A/B and snapshot checks.
- Final ordinary `cargo run --release -- --hidden --mute --auto-exit 0.5` passed. Inspected the
  full startup output and same-worktree `--tail-latest-log 200`: successful shutdown, `failures=0`.

### Test assumptions exposed by smaller defaults

Two architecture tests had implicitly borrowed the old default size 30 while asserting absolute
Conifer height / Willow tip clearance. Those reference fixtures now state size 30 explicitly;
separate new tests exercise the compact defaults.

A physics regression test also assumed the full visual cube mesh and compact root-axis collision
mesh shared triangle IDs. At the smaller trunk radius, part of its synthetic cube is intentionally
non-colliding, exposing the wrong oracle. Its padded-ray checks now use the actual collision
positions/indices, while still checking collision stays fixed through wind and changes only on
geometry publication. **No production collision filtering, BVH or movement algorithm changed.**
The original failure was the `tree_ray_candidates(...).contains(id)` assertion; the corrected test,
root-only/lateral-wood tests and native raster smoke pass. The historical introduction of the test's
assumption was not investigated; the scale change exposed it.

An intermediate arrival run logged a desktop XDG settings portal timeout during window setup;
subsequent final captures completed normally. Raster smoke retains its known repeated-delete
warning, `Attempted to remove non-existent tree 1`. Do not describe all runs as warning-free.
This was not a full test-suite, cross-platform or performance acceptance run. Screenshot FPS HUDs
are not benchmarks. No visible game session was automatically launched.

## Reproduction and local artifacts

Local diagnostic directory: `target/garden-delivery/world/apple-scale/` (not runtime assets).
Representative commands, run serially:

```sh
RUST_LOG=info cargo run --release -- --hidden --mute \
  --terrain-save target/garden-delivery/world/apple-scale/current.rflterrain \
  --screenshot garden-arrival target/garden-delivery/world/apple-scale/after-arrival.png \
  --screenshot-delay 2 --auto-exit 4

RE_FLORA_TREE_SPECIES_REVIEW=apple-walk RE_FLORA_TREE_SPECIES_SEED=122 RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/garden-delivery/world/apple-scale/after-walking-122.png \
  --screenshot-delay 2 --auto-exit 4

RE_FLORA_GARDEN_AUDIT=target/garden-delivery/world/apple-scale/audit RUST_LOG=info \
  cargo run --release -- --hidden --mute --auto-exit 0.5

RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify RUST_LOG=info cargo run --release -- --hidden --mute \
  --terrain-load target/garden-delivery/world/apple-scale/current.rflterrain --auto-exit 0.5
```

Repeat the walking fixture for seeds 7/907 and use `mixed` for the original mixed-scene view.
The pre-change capture requires baseline `00683cc7`; first-pass `sparse-*` captures use the
uncommitted density-0.035 candidate, not the final descriptor. No side-by-side runtime A/B mode
was requested or added. The existing offline HTML's original 11-image archive is unchanged;
these newer captures are separate evidence.

All PNGs below are original **2880 × 1620**, without cropping, recoloring or image generation.
`before-arrival` and `after-arrival` use the same preset/anchors; temporal wind/light can differ.
`before-planting` uses the original elevated species-review camera, not the new walking view.
Rendered assets retain repository licensing/provenance, including third-party notices; these
screenshots do not grant new blanket rights to their contents.

| PNG | Role | SHA-256 |
|---|---|---|
| `before-arrival.png` | Baseline opening | `4946d28821724f5c5a9b92ce457f98671dfb81e963bd71e433022104b96ecd39` |
| `before-planting.png` | Baseline configured Apple, seed 122 | `801265b4377a3844eb4f2e6a94f2658058d679de98b0d0f5f3792905d34386be` |
| `sparse-arrival.png` | Rejected sparse opening | `e3f138e66c95aeb3dd9606f99a4a7045bfad0c8834c9e6b011b53f65fa5c59b3` |
| `sparse-walking-122.png` | Rejected sparse close view | `86fde695942cc4a15197f3850a553a5929bd36fc2ed6918cdb6742b323135a73` |
| `sparse-mixed.png` | Sparse intermediate mixed fixture | `3322356ddf52cc8a57b2fe1125da8adadcd731c23e275abfd1cd66418c30110b` |
| `after-arrival.png` | Final opening | `f745278d628d2f0578a7ada7f6962a71f4a2e99c39f560b9d00fcadc4630ccdb` |
| `after-walking-7.png` | Final Apple close view, seed 7 | `5926f5cfedc32d5eefe010cee9a3e13ae5ff6121b151420029cff841d4fc7fe6` |
| `after-walking-122.png` | Final Apple close view, seed 122 | `d9ef3978472fa070eb5f0cbbb5b372b106e8b86ce011e5406687a748a1fe74bc` |
| `after-walking-907.png` | Final Apple close view, seed 907 | `d81a8b5d198a3d538a9ed30bd770ac53cf3364f59bfb7f5e7fa6f9073e4b3461` |
| `after-mixed.png` | Final mixed fixture | `97fadee7d3017d95fecb00023ff7fd29d7f46bf7e4c4cd493262cd5be1770e10` |

Snapshots and measurements:

- New `current.rflterrain`: `c615ff3c537d4441c1ec7bc824dbe9cd63c0b0dd5e0be827ec29c223f0aee875`.
- Read-only version-2 fixture `../apple-opening/current.rflterrain` remains
  `cf4867b46e327ff5833d33a746f901422da10f8d6aedb93c0e5a611b009e5299`.
- Read-only legacy fixture `../apple-opening/legacy-before.rflterrain` remains
  `3009be55a063f2072e44ea385581e24059d6e3bb3cefff5d43fce1789f497ded`.
- `measurements.jsonl`: `c4163798855375a3f88b1954030156601be69e91239b58e26746502a09b479ef`,
  produced with the local production-source probe described in the research note.
- Logs: `/tmp/re-flora-apple-scale-*.log`; durable native logs remain under the worktree's
  `target/re-flora-logs/`. No shaders, generated bindings or imported assets changed.
