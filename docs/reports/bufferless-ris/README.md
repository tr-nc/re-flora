# Buffer-less RIS experiment evidence bundle

Captured/measured locally on 2026-11-04. [View the HTML report](index.html).
This folder is self-contained presentation evidence, not runtime game assets.

## Files and roles

- `index.html`: visual report, original HTML/CSS layout and charts; no external font/script/image dependencies.
- `original.png`: unmodified original-path native game screenshot (runtime capture original, not an imported source artwork).
- `ris.png`: unmodified experimental RIS-path native game screenshot (runtime capture original).
- `captures.json`: capture hashes/renderer binary and camera hashes. Copied from `target/ris-capture-delivery/summary.json`; only the two `file` fields were renamed from `off.png`/`on.png` to this bundle's `original.png`/`ris.png`.
- `perf-8.json`, `perf-19.json`: all six alternating native runs and median summaries for each actually uploaded light count; originals were `target/ris-perf-delivery/summary.json` and `target/ris-perf-many/summary.json`.
- `audit-8.json`, `audit-19.json`: actual-receiver aggregate readback summaries; originals were `target/ris-audit-delivery/summary.json` and `target/ris-audit-many/summary.json`.
- `lifecycle-64.json`: accepted-count and removal fixture evidence, copied from `target/ris-scaling-complete/summary.json`. Its near-empty grass timings are **not** sixty-four-light grass performance evidence.
- `native-evidence.txt`: verbatim last four receiver-readback rows per audit run and all scaling stage/completion lines, with their original ignored local log paths. Full native stdout logs remain in those `target/` folders; no uncommitted local file is required to display the report.
- `README.md`: this provenance/processing record. [Research/source note](../../research/bufferless-ris.md) preserves the estimator rationale and limitations separately.

## Screenshot provenance and repeatable capture

Creator/source: the local Re: Flora native Release renderer and the repository's existing garden/assets. No external image service or generative image model was used for these new PNGs; no AI prompt/seed/model settings apply. The garden's procedural seed is an application setting, not an AI generation seed. There is no external source asset ID or source URL for these local captures.

Renderer source corresponds to the implementation through `01d8ead9` (later changes in that chain only affect validation scripts/fixtures). All JSON evidence in this bundle uses the same binary SHA-256:

`2cdd6e7f31b0b39a3e914afde96bdb9647f372e2264101f3838a49cdfae2f0e4`

Capture command from the repository root:

```bash
node scripts/validate-light-ris.mjs --case capture --output target/ris-capture-delivery
```

The output directory must be new when reproducing. The helper temporarily sets saved `time_of_day=0.95`, `grass_local_light_field=false`, `local_light_budget=8`, `grass_light_ris_rays=4`, `grass_light_ris_candidates=8`, and `grass_light_ris=false` then `true`, restoring the exact original config bytes afterward. Otherwise the current committed saved configuration is retained. Underlying per-mode native arguments:

```text
target/release/re-flora --hidden --windowed --mute --garden-seed 20260519
  --environment-probe-spacing-voxels 32
  --screenshot garden-arrival <output>/off.png|on.png --screenshot-delay 6
  --auto-exit 10
```

- Camera: committed `garden-arrival` preset; camera-file SHA-256 is in `captures.json`.
- Scene: ordinary fresh garden, no terrain load/save or fixed test scene. Actual final selected local-light count eight; the GPU upload allocation capacity is sixty-four.
- Physical output: 1920×1080; scene shading: 384×216, 5× scene pixel stride, UI native. PNG: 8-bit/channel RGBA, non-interlaced.
- No screenshot-specific freeze was applied: wind, plant simulation and DDGI update progress can differ between runs. Consequently, non-grass differences in the screenshots cannot be attributed entirely to RIS.
- Processing: byte-for-byte copy from `<output>/off.png` to `original.png` and `<output>/on.png` to `ris.png`. No crop, resize, tone adjustment, exposure adjustment, denoise, re-encoding or composite. HTML scales these originals responsively using CSS; image links expose the originals.

SHA-256:

```text
21535f44a7d1d7e68b3759bdadaaf9385138783e677898814722a4ce22a34758  original.png
a28ce8ce2bf9c389494523feeb4134ebf0c4de1fe78d2786d2c1fa2770c7ca6b  ris.png
```

## Measurement/research provenance and rights

The helper's `perf`, `audit`, `scaling`, and `capture` cases are reproducible through `node scripts/validate-light-ris.mjs --help`. The nineteen-light runs use `--light-budget 64` but report the actual selected count, not sixty-four fictitious emitters. Timing is uninstrumented Release production output; audit runs are separate and excluded. JSON numeric values were not fitted or rescaled for the HTML charts. The HTML rounds them for display.

Diagrams/charts are original CSS and local numerical measurements. Linked paper/SDK source pages remain external: no third-party paper illustration, restricted SDK implementation or denoiser was copied into this bundle. Screenshots incorporate existing project artwork; the repository's [asset policy](../../../LICENSE-ASSETS) and the applicable existing asset-bundle licenses/provenance continue to apply. This report does not grant new redistribution rights to depicted third-party artwork or imply that the game's code license licenses all artwork. No additional external artwork was imported for this experiment.

## Rendered delivery check

The report was opened in a dedicated headless agent-browser session and inspected at 1440×1000 and 390×844. Both PNGs loaded, page-level overflow was absent, both native disclosure controls opened, and a focused narrow comparison table scrolled with ArrowRight. Local file links, fragment IDs and image paths were checked. Axe-core 4.12.1: desktop 0 violations / 0 incomplete; narrow 0 violations / 1 color-contrast review incomplete from the deliberately horizontally clipped table/short zero cells. Table text and backgrounds were visually inspected; no claim of an incomplete-free narrow automated audit is made. Verification screenshots and raw axe JSON remain under ignored `target/ris-report-*`; the browser session was closed.
