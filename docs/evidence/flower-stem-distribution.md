# Stem voxel size and per-plant height distribution

## Saved controls

Debug → Flora → Ground Plants → Model Flowers now exposes:

- **Stem Voxel Edge Scale**: 0.2–4, default 1 (baseline half a grass voxel).
- **Stem Height Multiplier Mean**: 0.25–4, reuses the existing saved height value/ID.
- **Stem Height Multiplier Variance**: 0–1, default 0.01, not standard deviation.

All use the declarative config and standard save/load traversal. Global plant size
and growth still scale the whole plant. Browser previews and authored head assets
are unchanged.

Species own base integer counts (37–43), not target stem heights. A stable hash of
plant position and species supplies a Box–Muller standard-normal sample `z`:

```text
cap    = max(1, round(baseCount × (mean + 3 × sqrt(variance))))
count  = clamp(round(baseCount × (mean + sqrt(variance) × z)), 1, cap)
edge   = baselineEdge × voxelScale
height = count × edge
```

The sample does not depend on frame, time, draw slot or slider values. Variance 0
makes same-species counts equal. Size 0.9 preserves counts and makes the stem 10%
shorter; the mean slider changes counts instead of stretching cells. Integer
rounding, the one-layer minimum and the upper three-sigma safety cap mean actual
heights are a bounded discrete approximation, not an unbounded exact Gaussian.

## Rendering ownership and safety

- CPU source generations build a bounded bank of closed cells; GPU instances
  choose an integer prefix, hide unused layers and reposition rigid cell centers.
  No stretched partial top cubes and no per-instance head baking.
- A single source allocation carries its mean, standard deviation, edge, base
  count and draw count together. The 64-byte CPU/Slang part record and cache format
  version 5 prevent old/new layout mixing.
- The entire head, including calyx, follows the actual tip without changing head
  size. The stem gradient uses actual height. Root placement remains unchanged.
- Rest bend is limited for short/thin stems; combined rest/wind displacement keeps
  adjacent cell footprints overlapping, including one-layer stems.
- Index capacity covers all supported settings (maximum 301 layers). Culling
  includes shortened heads, display-quad corners and bounded wind transport.
- Higher variance increases bounded vertex work. These are correctness checks,
  not large-population performance acceptance.

## Validation

- `cargo fmt --check`, `cargo check`: passed.
- `PATH=/opt/homebrew/bin:$PATH cargo test`: 1,249 main + 4 auxiliary passed,
  4 ignored. Includes declarative save/reload and all renderer-input conversions.
- Focused model/cache tests: 7 + 6 passed. Cover voxel/count independence, variance
  vs standard deviation, zero variance, non-finite input normalization, count
  bounds, complete cubes, short-stem wind contact, source layout and sampled-head
  culling bounds.
- Hidden muted Release control sweep (`RE_FLORA_FLOWER_MODEL_REVIEW=controls`,
  `--auto-exit 15`) reached phases 0–26. New phases cover 0.9× vs 1× edge at fixed
  count, nonzero variance, thin/short/high-variance stems, maximum parameters and
  restoration. Clean shutdown with `failures=0`, no ERROR/panic/VUID diagnostics.
- `node scripts/validate-flower-models.mjs --seconds 10`: all eight species and nine
  view/resolution/lifetime/resize phases passed; config hash unchanged by runs.
- Final `cargo run --release -- --hidden --mute --auto-exit 0.5` and built-in
  `--tail-latest-log 100`: passed, successful shutdown and no validation errors.

Local screenshots/logs: `target/flower-native-review/`, `target/re-flora-logs/`.
Only GUI declarations were regenerated; no authored asset or browser changes.
No visible game session was launched automatically.
