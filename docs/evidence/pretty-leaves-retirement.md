# Merge main and retire the pretty-leaves rendering experiments

## Scope and recoverability

- Merge source: `main` / `origin/main` at `6522e8d1330def9c795b51ad2504ecf36210f1fc`.
- Previous feature tip: `ae8cdf2169287823b833304957209de250fdfe51`, preserved by
  `archive/pretty-leaves-experiments-ae8cdf21` before resolving the merge.
- This is an explicit retirement of the rejected appearance experiments, not
  acceptance of their camera-motion stability. No attempt to conceal their
  remaining silhouette flicker is retained.

The resolution uses main's voxel-derived trunk geometry/publication, including
its existing raster/hybrid controls, and attached voxel leaves. It removes the
later continuous `WoodMesh`, current-camera pixelized wood, attached shared-model
leaves/rotation switch, model surface cache and generic GPU paged storage. Main's
existing decorative falling-leaf model/sprite choice remains; actual detached
leaves retain the source voxel's geometry and size.

Real canopy detachment, weakening, regrowth, stable provenance, full release-batch
reservation, GPU pose handoff, synchronized occupancy/ecology/canopy audio and
reversible lifecycle A/B remain. Their independent history through `b3329b30`
served as the behavioral reference, not as a wholesale replacement for main's
newer architecture. Lifecycle remains default-off and session-only.

## Conflict decisions

- Preserve main's `ModelPixelFrame` ownership and strict executed-projection
  diagnostics. Adapt demand-sized model inputs to that owner: complete instance,
  triangle and draw-index streams publish once in the ready fence slot, and the
  prepared draw retains its matching indices. No cache-era allocation protocol
  leaks back into `Tracer`.
- Keep same-frame tile batches under 64 MiB and 65,535 Z-dispatch instances,
  including low-resolution/invisible objects. The old 16,384 simulation reservation
  is not a real-leaf admission ceiling. GPU storage/address/memory limits still
  exist; this is not an unlimited-population or performance claim.
- Preserve main's vine tick and DDGI fixes. Extend the existing leaf-review scene
  exclusion to the lifecycle fixture before the automatic vine can edit terrain
  or focus the camera. The fixture uses the same wind input for detachment and
  attached GPU response.
- Migrate saved experimental controls back to main's tree controls without losing
  authored wind, lifecycle or falling-leaf values. Main saves gain the five
  lifecycle controls and decorative-only enablement. Loading does not write the
  file; unified Save round-trips the migrated schema.
- Regenerate tracked GUI/shader-derived Rust with `cargo check`, rather than
  resolving generated definitions by hand.

The removed cache/paging work has no dependency in this restored path. It remains
recoverable in the archive if a separately scoped, measured optimization later
justifies it; retaining an unused second resource-ownership scheme is not needed
for the lifecycle feature.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo build --release` passed.
- Full `cargo test`: **1224 binary tests passed, 4 ignored; 4 collision tests passed**.
  GUI migration tests cover old main and experimental saves, wind precedence,
  preserved authored values, no implicit saves and Save/reload.
- Frame tests cover once-only input publication, allocation failure/retry,
  ready-slot reuse/retirement, sorted compute/draw pairing across tile batches,
  200,000 low-resolution/invisible models and byte-range/overflow guards.
- Hidden, muted Release startup/shutdown passed.
- `scripts/check_real_leaf_lifecycle.sh`: **14,572 attached → stripped → regrown →
  stripped**, **29,144** actual detached leaves, capacity **32,768**, unrelated
  particle retained, live occupancy/ecology and reversible mode checks passed.
  App log: `target/re-flora-logs/re-flora-20260927-161623.508-910012.log`.
- `node scripts/validate-leaf-model.mjs --seconds 20`: main's unchanged strict
  original RGBA, coverage and depth oracle passed the complete A/B/size sweep.
- `node scripts/validate-apple-model.mjs --seconds 12 --stage-one`: attached/fallen
  apples, actual fruit drops, independent model resolutions, orthographic A/B and
  8/16/37/128/512 views passed.
- `python3 scripts/validate_butterfly_mesh.py --seconds 12`: live 8/22/64px,
  independent hit-depth oracle and self-shadow switches passed.
- Hidden `--raster-tree-smoke --resize-lifecycle-test --auto-exit 30` completed
  `[TREE][RASTER_SMOKE] passed` with all geometry/lighting/wind/stiffness/age/remove/
  replace checks and **110 color draws**; resize reached generation 4.

Native runs used `/tmp/re-flora-summer-gpu.lock`. Logs were checked for errors,
panics, VUIDs and clean shutdown. GUI/camera files remained byte-identical across
these runs. Full command logs are `/tmp/re-flora-merge-{final-fmt,final-check,
final-test,build,smoke,lifecycle,leaf-native,apple-native,butterfly-native,
tree-native}.log`; native model artifacts remain under their usual `target/`
review directories.

An initial 150-second full test attempt timed out before this approximately
193-second suite completed and showed a failure in the unchanged sparse-worker
readiness test. That test passed in isolation; two subsequent full default-thread
runs passed without changing it. The incomplete attempt is not counted as a pass.

No visible game was automatically launched. These are correctness/resource-use
checks, not new manual visual/audio approval, natural-wind/dense-scene acceptance
or performance measurements. The former pixelized-wood motion fixture was retired
with its renderer, not relabeled as a successful visual fix.
