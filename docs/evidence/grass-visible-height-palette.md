# Grass palette follows current visible height

## Impact

Grass shorter than its local mature height (young/trimmed/regrowing) did not reach the configured tip palette. Competition-limited grass also had a separate yellow-green stress tint applied after palette sampling, so its base/tip colors differed even when normalized gradients reached their endpoints.

## Cause

`prepareFloraVertex` trimmed geometry with `min(growthProgress / 255, growthPotential)` but passed `growthPotential` alone to the color helper. The denominator therefore described future local maturity, not currently visible layers. Both cached and direct grass consumers subsequently applied `applyGrassGrowthStressTint`, overriding configured palette endpoints according to competition/environment limitation.

The current maturity-only helper was introduced in `dc8274c3` (2026-10-06), which deliberately retained immature tip colors below the final palette. The tint was present by the Slang port `6d7ad9ff` (2026-07-18); its earlier origin is not established here. This change adopts the user's clarified rule rather than retaining that older maturity convention.

## Fix

The geometry owner computes the current discrete visible layer count once and supplies it directly to `grassColorGradient`. For N >= 2, layer i uses i/(N-1): bottom=0, top=1 at every height. N=1 is both root and tip and retains the existing tip-color convention; N=0 emits no geometry. The helper no longer accepts an independently selectable growth potential.

Remove the additional growth/competition tint in both render paths. Existing palette variation, soil-wetness material response, lighting and shadows remain, so this is a configured-albedo endpoint guarantee, not a guarantee that differently lit tips have identical final screen pixels. Moisture-driven wilting is unaffected.

## Validation

- Before fix: `cargo test grass_palette_uses_visible_height` failed on the real caller wiring. This is a source-wiring guard, not a rendered-color test.
- Production Slang executable `shader/tests/grass_growth_color.slang` passes over heights 2..8, every 8-bit growth progress and all 16 competition levels. Checks exact base/tip colors, full linear interpolation, one-layer behavior and equal gradients for differently limited stems with equal visible height.
- `cargo test flora::species`: 13 pass. `cargo fmt --check`, `cargo check`, `cargo build --release` pass.
- Hidden muted ordinary Release smoke passes. Existing native grass lifecycle review passes with pose reuse both on and off, growth 0..1, pixelization toggles and window resizes. Logs have no ERROR/panic/VUID. `--perf` is required by this review harness; this is not performance acceptance.
- Viewed screenshot: `target/grass-color-review/near-both-b.png` at growth about 0.75; visible short stems span bottom through tip colors.
- Full Rust/Slang suites were not repeated for this focused change; preceding validation had nine unrelated Rust failures and a sprinkler-contact Slang failure, documented in `docs/research/plant-water-status.md`.

```sh
RE_FLORA_GRASS_STEM_REVIEW=near-both-b \
RE_FLORA_STEM_PIXEL_LIFECYCLE=1 RE_FLORA_STEM_SAMPLE_FRAMES=60 \
RE_FLORA_GRASS_STEM_CAPTURE=target/grass-color-review \
cargo run --release -- --authored-flora-bench --hidden --mute --perf
# Repeat with RE_FLORA_GRASS_BAND_POSE_REUSE=0 for the direct path.
```

No configuration or generated-file changes. No visible game automatically launched.
