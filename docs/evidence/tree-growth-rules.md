# Shared tree growth refactor: compatibility and native evidence

> Historical evidence for this delivery stage. The subsequent requested opening-tree replacement
> and Test/Garden Tree retirement are documented in
> [Apple opening and Test Tree retirement](apple-opening-and-test-tree-retirement.md).

## Scope

Instrumented baseline: `24bd9d2f`, with generation unchanged from `2bc5b7dd` / report `4b242d1f`.
The refactor introduces one internal growth interface and typed species rules, preserving the
existing public `Tree` interface, saved descriptions, shapes and gameplay. See
[`../tree-growth.md`](../tree-growth.md) for ownership, extension points and limits.

No runtime GUI configuration, shader, generated Rust file, game image/audio asset, opening layout,
fruit lifecycle or renderer behavior was intentionally changed. The four opening trees remain
legacy Garden Trees. The previous documentation's claim that these became Apple Trees was corrected;
only the tuning description and new planting selection default to Apple Tree.

## Before/after generation comparison

Added an opt-in ignored diagnostic **before** changing generation:

```sh
cargo test --release capture_tree_generation_compatibility -- --ignored --nocapture \
  > /tmp/re-flora-growth-before.log 2>&1
rg '^TREE_GENERATION_FINGERPRINT ' /tmp/re-flora-growth-before.log \
  > target/tree-growth-refactor/before-fingerprints.jsonl
# Repeat after the refactor, replacing before with after, then:
diff -u target/tree-growth-refactor/before-fingerprints.jsonl \
  target/tree-growth-refactor/after-fingerprints.jsonl
```

**All 148 records matched exactly.** Each capture crosses the actual serialized `TreeDesc` →
`at_age` → `Tree::new` interface and fingerprints description bytes, branch topology/identity,
positions, axis metadata, wood cones, translated rest collision cones, ownership indices, and
leaf-instance positions/attachment anchors. Coverage:

- Six generation modes: legacy Garden, both legacy Conifer modes, Apple, Pine, Willow.
- Seeds **7 / 122 / 907**; ages **0 / 0.25 / 0.45 / 1**.
- Unmodified profiles and edited size/thickness/density/subdivision descriptions: 144 combinations.
- All four default opening-tree descriptions: four more combinations.

Both JSONL artifacts have SHA-256:

```text
88141a32e8211ebb51c3c6ae8e87112b5754a908fff1bb2d9238296aca6bd17e
```

This is same-machine, same-toolchain bitwise generation evidence, not a portable cross-platform
float golden, exhaustive proof over all possible inputs, screenshot equality, or a performance
benchmark. The diagnostic remains ignored during ordinary tests. Existing deterministic invariant
checks and new rule-level behavioral checks run normally.

## Automated checks

- `cargo fmt --check`, `cargo check`, `git diff --check`, `cargo build --release`: passed.
- `cargo test tree_gen`: **45 passed**, one intentionally ignored diagnostic. Includes connectedness,
  deterministic seed/age behavior, bounded axes, finite taper, physical root-axis ancestry and pose
  bindings. New public-interface tests enforce Apple tip-only versus Pine/Willow/Conifer along-shoot
  foliage, independence from recursive-only controls, and preservation of live authored settings.
- `cargo test vegetation::`: **43 passed**, including mixed snapshot restore and fruit policy.
- `cargo test plant_catalog`: **2 passed**; `cargo test tree_gui`: **3 passed**.
- `cargo test every_tree_architecture` and `cargo test lateral_wood_stays_excluded`: **1 + 1 passed**.
- `cargo test garden_layout`: **3 passed**, preserving opening footprint and layout contracts.
- `cargo test every_serialized_custom_leaf`: **1 passed**, unified saved-object round trips.

Counts are per filter, not a deduplicated full-suite total. Full `cargo test` was not rerun; earlier
unrelated suite failures/timeouts were not repaired. No measured speedup or performance acceptance
is claimed.

## Native Release checks

Repeated the existing unsaved fixture for Apple/Pine/Willow at seed 122 and the mixed scene at
122/123/124. Each used the real preview, publication, age 1 → 0.45 → 1, JSON snapshot restoration,
CPU/GPU pose, surface/BVH and lighting checks:

```sh
RE_FLORA_TREE_SPECIES_REVIEW=apple RE_FLORA_TREE_SPECIES_SEED=122 RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/tree-growth-refactor/after/apple-122.png \
  --screenshot-delay 2 --auto-exit 4
# Repeat with pine / willow, and with REVIEW=mixed for the mixed scene.
RUST_LOG=info cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 120
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 180
```

All four species fixtures passed and shut down with `failures=0`. Single-tree counts exactly match
the original delivery's seed-122 runs:

| Species | Branch segments | Published quantized leaves | Apple sites |
|---|---:|---:|---:|
| Apple | 250 | 31,329 | 27 |
| Pine | 460 | 20,098 | 0 |
| Willow | 448 | 12,605 | 0 |

The original raster smoke passed kick/wind overlay, publication rollback, legacy Conifer A/B,
snapshot restore, age/stiffness sweeps, deletion and replacement. It again logged the known
`Attempted to remove non-existent tree 1` warning. No ERROR/panic/VUID appeared in these runs. The
ordinary hidden/muted startup exited normally; its same-worktree latest log was inspected.

### Visual evidence and processing

Viewed `target/tree-growth-refactor/comparison.png` and the mixed native frame. The three species
retain their distinct silhouettes, attachment layout and crown organization; no unintended
structural change was observed. This is regression review, not a newly proposed appearance.

- `before/{apple,pine,willow}-122.png` are byte-exact copies of the original delivery's native
  `target/tree-species-review/` captures, **1600 × 1000**. They were not newly rendered this turn.
- `after/{apple,pine,willow}-122.png` and `after/mixed.png` are fresh native Release captures,
  **2880 × 1620**. The current native monitor/Wayland scaling differs from the earlier run.
- Contact sheet: before crops **760×740+380+50**; after crops **1231×1199+760+81**, derived by the
  1.62 height scale around frame center to preserve the same approximate view. Both are resized
  to **390 × 380**, with 32-pixel labels and 6-pixel montage margins. ImageMagick with the system
  Noto Sans font was used. Originals are retained; no recoloring, foliage edits or AI generation.
- These are transient validation artifacts under ignored `target/`, not imported game assets.
  Different capture extent, temporal lighting and wind phase mean this is **not a pixel-exact or
  controlled performance A/B**. The 148 generation fingerprints provide the exact structural check.

Temporary command logs: `/tmp/re-flora-growth-*.log`; durable app logs: this worktree's
`target/re-flora-logs/`. Final ordinary log:
`re-flora-20261025-135847.254-105082.log`. No visible game was automatically launched.
