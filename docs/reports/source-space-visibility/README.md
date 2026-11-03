# Source-space visibility — experimental evidence and provenance

2026-11-03 · **[View the visual report](index.html)** · implementation `2e548ae4`, independent quadrature guard `1d0e3122`.
This supersedes the runtime experiment, not the historical findings, in the
[rejected receiver-factorization report](../local-light-field-32/index.html).

**Nonzero sharing is now demonstrated; an optimization win is not.** The saved
`grass_local_light_field` checkbox remains off by default. Unchecked is original
per-receiver lighting, checked is the source-space visibility candidate. Both
sides include the independently committed traversal-scratch fix `df8ad327` and
finite-source endpoint ownership fix `349bc53c`. This is not comparison against
the older, incorrectly self-shadowing evaluator.

## What the evidence establishes

- Spatial DDGI probe spacing remains **32**, with no global/local spatial probe
  densification, exposure/light boosts, brightness floors or changed indirect
  transport. A 32×32 angular grid on each cube face is a different visibility
  domain, not new DDGI probes. Its 24 source origins are the existing possible
  finite-emitter quadrature points; each receiver still uses its original 12 taps.
- Certificates prove either a whole cone clear up to a conservative depth, or
  intersection with an actual published opaque voxel. They do not interpolate
  radiance or choose shadow-map depths as an approximation. Uncertainty uses
  original transport; certified-clear connections still query live trees.
- The actual grass-consumer audit samples roughly 1/1024 receiver entries and
  checks each active connection against the original evaluator in the **same GPU
  scene state**, not just at producer positions. Final stable default sample:
  **952 active taps; 253 clear, 381 opaque, 318 fallback; 341 reference-nonzero,
  253 shared-nonzero; zero sampled transmission mismatches**. Shared direct
  luminance is **0.022779964 / 0.025130939 = 90.6451%** of sampled reference direct
  luminance. The audit covers active finite-emitter taps, not unsupported analytic
  point-light fallbacks. An earlier stable sample was 90.9320%; seed 7 was **83.0690%**, with
  288/423 nonzero connections shared and zero mismatches. These are samples, not
  all grass, all pixels or a percentage of total direct-plus-indirect illumination.
- Energy uses unquantized float CAS accumulation of actual RGB times linear
  luminance weights (0.2126, 0.7152, 0.0722); no clipped counter or zero-valued
  cache acceptance is counted as useful energy. Summation has float roundoff.
  Raw startup/steady-state observations are in [receiver-audit.txt](receiver-audit.txt).
- Source/geometry changes invalidate the whole frame-owned cache; partial builds
  have an unavailable header and retain original lighting. Only the last batch
  publishes readiness. Native startup observations verify fallback before
  publication. Pure contract tests reject stale identities, incomplete/truncated
  buffers and overflow. Complete real-grass add/remove/blocker lifecycle coverage
  is **not established** by these startup runs.

## Measurements and visual limits

Same Release binary, default seed, 8 selected lights, original opaque guardian,
2560×1440 output / 512×288 internal scene, nearest upscale, camera
`garden-arrival`, spacing 32, time 0.95, unchanged exposure.
Two sequential **unaudited** 25-second M4 Pro Vulkan/MoltenVK runs, final ~6 seconds:

| Mode | Grass cache GPU median | Frame-render GPU median | Actual FPS | GPU samples |
| --- | ---: | ---: | ---: | ---: |
| Original | 28.365 ms | 50.9225 ms | 17.3420 | 4 |
| Source-space candidate | 37.437 ms | 59.453 ms | 14.8024 | 3 |

FPS is frame advancement / elapsed wall time: 104/5997 ms and 88/5945 ms.
This candidate is slower in this short sample; **no performance acceptance** or
RTX result is claimed. Sparse sequential runs have no confidence intervals and
retain thermal/temporal variation. Cache producer has no dispatch in the final
window, not zero consumer overhead. Eight lights use **18,874,400 bytes (18 MiB)
per in-use frame slot**, plus unchanged DDGI allocation. Stable-field build took
10.407 seconds across 144 batches of at most 8192 cells; five sampled producer
scopes had median 2.544 ms and maximum 5.315 ms, not an exhaustive maximum.
Continuous light/geometry changes can prevent completion and retain fallback.

The fixed old warmth-loss ROI gives candidate/reference **1.10585**: a one-sided
loss check passes, **not equivalence** (about 10.6% above reference). Independent
runs retain wind, simulation and DDGI history/convergence differences. Images
are not a frozen or pixel-identical direct-only comparison. Same-frame consumer
checks provide the stronger direct-visibility evidence; neither establishes
formal visual approval.

## Validation and blocked checks

- `cargo fmt --check`, `cargo check`, `cargo test`: **1833 app + 4 library tests
  pass**, 15 app tests ignored. Includes actual egui checkbox click, save/reload
  and switch-back, bounded-build and device-range contracts.
- Release build and hidden muted native default/seed-7 opaque runs pass; no
  errors, panics or VUID matches in these successful native logs.
- `local_visibility_certificates.slang`: **21,805** cases pass (cones/seams,
  off-centre thin blockers, source mapping, header freshness/readiness).
  `local_emitter_endpoint.slang`: **441** cases pass. Captured grazing endpoint
  error was ~12 micrometres against an old 10-micrometre trim. Ownership, not
  a larger epsilon, excludes only complete authored emissive voxels; foreign
  material edits and neighbouring emitters remain blockers.
- Standalone Slang **63/68** pass. The five existing failures reproduce against
  pre-task shader sources: `sprinkler_contact_contracts`,
  `sprinkler_radial_coverage`, `sprinkler_rotary`, `sprinkler_spray_continuity`
  (exit 1), and `terrain_material_contract` (compile exit 255, old argument-count
  mismatch). Native app compiler is SDK Slang 2025.11-12-gc5295eae2; standalone
  tests use `.tools/slang-2025.23/bin/slangc`.
- **Blocked: seed 7 + Glass guardian** crashes in existing DDGI transport with one
  non-finite record. The preserved, untouched pre-task Release binary reproduces
  the **identical** full `DdgiTraceStats`; this is not a new cache mismatch.
  [Both excerpts](glass-baseline-failure.txt) are retained. It is not fixed here,
  not a passing Glass end-to-end claim, and not waived as visual acceptance.

## Reproduction

From repository root, build `2e548ae4` (or this report-only successor) and use the
saved Debug checkbox **Grass: Source Visibility Cache (A/B)**. For capture only,
set `time_of_day=0.95`; off/on runs use false/true. Restore 0.47/false afterwards.
No other lighting, material or density setting changes. Do not run concurrent
apps or GPU browser validation during timing.

```sh
cargo build --release
# MODE=off or on, matching the saved checkbox. No audit during timing.
target/release/re-flora --hidden --windowed --mute --perf \
  --environment-probe-spacing-voxels 32 \
  --screenshot garden-arrival target/source-vis/IMAGE.png \
  --screenshot-delay 18 --auto-exit 25 > target/source-vis/final-MODE.log 2>&1

# Separate receiver audit; add --garden-seed 7 for the second layout.
RE_FLORA_LOCAL_LIGHT_FIELD_AUDIT=1 target/release/re-flora \
  --hidden --windowed --mute --perf --environment-probe-spacing-voxels 32 \
  --screenshot garden-arrival target/source-vis/audited.png \
  --screenshot-delay 18 --auto-exit 25

node docs/reports/source-space-visibility/summarize-perf.cjs
node docs/reports/source-space-visibility/check-warmth.cjs \
  docs/reports/source-space-visibility/original.png \
  docs/reports/source-space-visibility/candidate.png
```

The timing parser reads `target/source-vis/final-{off,on}.log`; committed output
is [final-perf.json](final-perf.json). Warmth script requires ImageMagick `magick`;
its output is [warmth.json](warmth.json). Scripts are bundled to keep this evidence
self-contained. The warmth checker is copied unchanged from the preceding report.
For standalone tests compile the named file with `-std 2025 -I shader/slang
-target executable`, then run with `DYLD_LIBRARY_PATH=$PWD/.tools/slang-2025.23/lib`.
Raw run/build logs stay under ignored `target/source-vis/`.

## Image provenance and rights

Every image in this folder is an **original native app screenshot**, not an AI
image or a derivative of the other screenshot. Creator/source: this repository's
local Release renderer at `2e548ae4`, on Apple M4 Pro, 2026-11-03. There is no
external image service, asset URL/ID or generation model/prompt/negative prompt;
these are not applicable, rather than unavailable generation controls. Game
procedural seed is **20260519**; both use the capture command/settings above.

- [original.png](original.png): original/reference runtime frame; source filename
  `target/source-vis/original.png`; unchecked original lighting.
- [candidate.png](candidate.png): experimental runtime frame; source filename
  `target/source-vis/candidate.png`; checked source-space visibility.

Both are **2560×1440, 8-bit RGBA PNG, non-interlaced**, copied byte-for-byte with
`cp`. No crop, trim, resize, resampling, exposure adjustment, retouch or colour
processing. HTML provides identical CSS-only clipping/zoom and links to full
originals; those are presentation views, not edited assets. No separate preview
files are supplied. Neither screenshot changes the world geometry or materials.

SHA-256:

```text
a44a6e4b4aa33a793ab3a9c1082f9597cabae0fef82c1a29bc8935dde32b4842  original.png
801426d06b1f3175feb54d644b77370bceb7efac16f569f78e36d30a77080989  candidate.png
```

The scene's existing code/asset licence and attribution obligations still apply;
no new third-party art/audio was imported. Redistribution rights for the full
rendered scene were not independently audited. This note is not a new rights grant.
