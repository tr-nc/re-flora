# Apple opening and Test Tree retirement

This is the requested **visible recipe change and retirement**, after the behavior-preserving
refactor at `85c4258b`. The older [148-case report](tree-growth-rules.md) describes its own historical
baseline; it did not replace opening trees or retire their morphology.

## Behavior and ownership

- The new garden has four current Apple Trees at the unchanged normalized anchors
  `(0.23, 0.32)`, `(0.36, 0.22)`, `(0.78, 0.57)`, `(0.80, 0.30)`. Individual seed mapping is unchanged.
- The opening has no private branch recipe, five-iteration override, fruit-disable override or baked
  legacy age profile. It uses the shared Apple architecture and real apple lifecycle, fitted to the
  existing 46-voxel radial envelope. The bound now measures actual leaf placements, not anchors
  plus an assumed radius.
- An initial native candidate kept full-sized level-5 leaf clouds while fitting only wood. It looked
  too bush-like at this compact footprint. The final opening uses level-4 sprays and larger fitted
  wood, retaining visible Apple scaffolds/trunks. This is opening authoring, not a second generator
  or a change to planted Apple defaults. Arrival and overview frames were viewed.
- Garden recipe version is now 2. The flower hash stream is independent of that version: all 180
  flowers retain fingerprint `c285dfeb`. Ordinary grass totals may vary with republished tree bounds.
- `TreeDesc::default()` is Apple. The Garden Tree species definition, `LegacyGarden` authoring arm
  and old default branching entry point are removed. Other consumers use current Apple or explicit
  legacy Conifer. Shared recursive branching remains because Conifer and other vegetation use it.
- `src/tree_gen/compatibility.rs` contains only historical wire defaults, normalization and saved
  geometry reconstruction. `garden_tree` / missing species decode to a retirement marker, not a
  catalog species. Ordinary construction normalizes it to Apple **before** age scaling.
- GUI deserialization normalizes retired tuning descriptions. Garden restoration must do more:
  its atlas already contains baked wood. It first reconstructs exact saved wood through the
  existing recursive adapter, then uses ordinary batch tree replacement to clear old wood and
  publish Apple wood, foliage, fruit, collision and observers. Both reconstruction budgets are
  checked before mutation. No independent Garden Tree architecture is registered.
- Conversion retains ID, position, size, individual seed, global age and fruit phase, but adopts
  current Apple authoring. Retired trees' manual wood edits are replaced with the morphology.
  Conifer, Pine, Willow and existing Apple descriptions are not migrated; overlapping geometry
  follows the existing replacement transaction's neighbor handling.
- Loading never overwrites the source snapshot. An explicit Save persists Apple identities.
  Loading an empty save still does not generate an opening garden.

Default opening observations (world units for root positions):

| Tree | Seed | Root x/z | Fitted size | Crown radius, voxels |
| --- | --- | --- | --- | --- |
| 0 | 2539892922 | 0.46 / 0.64 | 8.874 | 40.675 |
| 1 | 2847798192 | 0.72 / 0.44 | 9.605 | 44.341 |
| 2 | 305502193 | 1.56 / 1.14 | 8.785 | 42.085 |
| 3 | 580287596 | 1.60 / 0.60 | 9.407 | 44.435 |

All use Apple fruit probability `0.45`; this is not a visible-fruit count. Bounds describe generated
rest geometry, not an exhaustive envelope over all possible wind settings.

## Automated and native evidence

Filtered tests (overlapping scopes; not a summed full-suite result):

| Filter | Passed |
| --- | ---: |
| `tree_gen` | 45; 1 explicit diagnostic ignored |
| `garden_layout` | 3 |
| `vegetation::` | 46 |
| `gui_config_model` | 4 |
| `gui_config_loader` | 56 |
| `plant_catalog` / `grow_panel` / `player_tools` | 2 / 3 / 19 |
| `tree_gui` / `every_serialized_custom_leaf` | 3 / 1 |
| `tree_collision` / `trunk_only` / `lateral_wood_stays` | 2 / 1 / 1 |

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- Compatibility capture: **144 fixed-description fingerprints match the pre-retirement baseline**.
  Of these, 120 cover retained generation modes and 24 cover the read-only reconstruction used to
  remove retired wood. The four opening rows intentionally change. Same-machine/toolchain sampled
  evidence only; not cross-platform float goldens, screenshot equality or a benchmark.
- Native fresh-garden audit: capsule walked the route in **212 steps**; Dig removed **129** dirt
  voxels; Grow accepted and mow removed the planted fixture. No protected terrain or route hacks.
- Captured a real version-1 garden at `85c4258b` before editing, then loaded it through production
  startup and two runtime replacements, with intervening tree placement each time. Each conversion
  retained four identities, exact flora layout/growth and canonical Apple descriptions/leaf data.
  Water returned to `Running / Ready`, world updates resumed, and there were no duplicate trees.
- Native old-atlas readback: **15,422 changed voxels**, confined to wood addition/removal near the
  old/current tree geometry; **1,151 old-only interior probes** had **zero wood ghosts**, on all
  three loads. The audit allows a two-voxel boundary tolerance and probes old interiors at least
  one voxel inside: it is not a CPU replica of GPU voxelization or proof over every boundary voxel.
- Saved the converted garden to a separate file, restarted from it and repeated two runtime loads:
  terrain bytes, flora and tree data matched exactly. The newly generated version-2 save passed
  the same startup/two-runtime-load check. Converted files no longer trigger retirement.
- Original raster-tree smoke passed, including GPU pose/surface/lighting, age/stiffness, deletion,
  replacement and legacy Conifer A/B/snapshot restoration. Its known repeated-delete warning
  (`Attempted to remove non-existent tree 1`) remains; this was not a warning-free run.
- Ordinary hidden/muted Release startup exited successfully; same-worktree log inspected.
- No full-suite or performance acceptance claimed. Previously known unrelated full-suite failures
  and long-running climbing tests were not repaired here. Final aesthetics remain user-reviewed.
- No shader, generated binding, runtime asset or `config/gui.toml` changes. No visible game was
  automatically launched and no commits were pushed.

## A validation failure diagnosed during development

**Impact:** the blocking snapshot smoke timed out during the first runtime load of a retired-tree
save. A current-species control save passed both replacements. Startup conversion and atlas checks
already passed; a normal-frame gameplay deadlock was not established.

**Root cause:** migration publishes new wood after the full snapshot collider import, queuing new
Contree CPU-cache work for three chunks. Water correctly refuses to sample an unready CPU source.
The synchronous smoke blocked normal frames and pumped only water; unlike normal `RedrawRequested`,
it never polled CPU-cache jobs. Diagnostics showed `source_pending=3`, `source_active=0` indefinitely.

**Introduction:** the new migration exposed an existing incomplete test-driver dependency loop.
The precise historical commit introducing that loop was not investigated. The red run is
`/tmp/re-flora-retirement-legacy-load.log`; the current-species control is
`/tmp/re-flora-retirement-current-load.log`.

**Fix:** both blocking snapshot-review loops now advance the same CPU-cache → terrain-collider →
water dependency order as normal frames. No production water queues, readiness rules, timeouts or
error checks were bypassed. The original legacy fixture and both current/re-saved variants passed
startup and two subsequent loads after the fix (`*-legacy-load-fixed.log`, `*-current-reload.log`,
`*-migrated-reload.log`).

## Reproduction and artifacts

Artifacts are local, ignored validation outputs under
`target/garden-delivery/world/apple-opening/`, not shipped runtime assets. The legacy fixture was
produced with the clean `85c4258b` Release binary; regenerating it requires that historical revision.
Do not overwrite a player's save for this check.

```sh
# On the pre-retirement revision, into a task-owned temporary path:
RUST_LOG=info cargo run --release -- --hidden --mute \
  --terrain-save target/garden-delivery/world/apple-opening/legacy-before.rflterrain \
  --screenshot garden-overview target/garden-delivery/world/apple-opening/legacy-before.png \
  --screenshot-delay 2 --auto-exit 4

# On the current revision: real old-save conversion, repeated loads and separate Save.
RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify RUST_LOG=info cargo run --release -- \
  --hidden --mute \
  --terrain-load target/garden-delivery/world/apple-opening/legacy-before.rflterrain \
  --terrain-save target/garden-delivery/world/apple-opening/migrated.rflterrain \
  --screenshot garden-overview target/garden-delivery/world/apple-opening/migrated-overview.png \
  --screenshot-delay 2 --auto-exit 4

RE_FLORA_GARDEN_AUDIT=target/garden-delivery/world/apple-opening/audit RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-save target/garden-delivery/world/apple-opening/current.rflterrain \
  --screenshot garden-arrival target/garden-delivery/world/apple-opening/current-arrival.png \
  --screenshot-delay 2 --auto-exit 4

# Repeat for current.rflterrain and migrated.rflterrain; all terrain bytes must now be exact.
RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify RUST_LOG=info cargo run --release -- \
  --hidden --mute --terrain-load target/garden-delivery/world/apple-opening/migrated.rflterrain \
  --auto-exit 0.5

cargo test --release capture_tree_generation_compatibility -- --ignored --nocapture
RUST_LOG=info cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 120
RUST_LOG=info cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 180
```

All four reviewed PNGs are unmodified native **2880 × 1620** screenshots. No generated illustration,
external plant image, crop or recoloring was used. Native review frames are not controlled temporal
or performance A/B captures. The standalone HTML retains its original 11 images; it does not embed
these new opening captures. Screenshot content retains the project's mixed asset licensing; see
`LICENSE-ASSETS` and individual asset notices, not a blanket redistribution license.

| Local PNG | Role | SHA-256 |
| --- | --- | --- |
| `legacy-before.png` | Historical opening reference | `33eb7910f6376e79b90b54f8e6cf1e2f7b8df608d8529f3d6335bb0fb4276959` |
| `current-overview.png` | Final new-garden overview | `599b2d0db8a777dd0ec3a44c5a0a05c123650a6805e980e52a23903ccd2276b5` |
| `current-arrival.png` | Final arrival, after opt-in edit/collision audit | `68af6028224a1f5800ce8753c216d3687dfb8b15be12d16dfbadda01dadd8bf5` |
| `migrated-overview.png` | Converted old save, retaining original larger sizes | `c568fd8a93130e384de4ad74a9b348149f2846f96e1df9f3dc2f61fd45471104` |

Source snapshot SHA-256 remained
`3009be55a063f2072e44ea385581e24059d6e3bb3cefff5d43fce1789f497ded` before/after conversion.
Converted snapshot: `99c4c22c6e796c9917d091a6e2688b30229b8eb1bfa13320523b24c5dd016243`.
Fresh final snapshot: `cf4867b46e327ff5833d33a746f901422da10f8d6aedb93c0e5a611b009e5299`.

Command logs are `/tmp/re-flora-retirement-*.log`; durable app logs are in
`target/re-flora-logs/` in this worktree. These local artifacts are not required by the shipped app.
