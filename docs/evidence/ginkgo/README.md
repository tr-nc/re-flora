# Ginkgo delivery and native captures

**Visual report: [银杏 · 一树秋色](index.html).** Open `index.html` in a browser;
keep it beside both PNGs when copying this bundle. This Markdown file retains
supporting provenance and historical validation evidence, not the visual deliverable.

Implementation: `d753b383` (`add plantable autumn ginkgo trees`).
Research and explicit botanical/art limits: [ginkgo research](../../research/ginkgo.md).

## Player behavior

- **Grow → Tree → Ginkgo**; finite-mode shop offers **Ginkgo sapling**, 35 coins.
- Upright branching, narrower upper scaffold envelope, shallow-notched fan-shaped
  voxel sprays, golden autumn foliage and gray-brown ridged bark. No apples.
- Uses the existing per-tree size/maturity settings, deterministic individual
  variation, growth, wind, collision and snapshot publication. Golden base colors
  belong only to ginkgo; saved individual color offsets apply after the base.
- This is a fixed autumn art direction, not four-season phenology. Sprays are
  stylized leaf masses, not new individual-leaf meshes. Existing falling-leaf
  particles are unchanged. No seed/white-nut harvest or sex selection is added.
- No existing species identity, saved description format or default is replaced.

## Validation

Executed in the implementation worktree on macOS / Apple M4 Pro / Vulkan via
MoltenVK. These are correctness runs, not controlled performance benchmarks.

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- Full `cargo test`: **1772 passed, 0 failed, 15 ignored**, plus **4 passing**
  collision-library tests. Integration targets contained no applicable tests on
  this platform. Initial expansion exposed an existing three-species test fixture
  placing the fourth tree outside the garden; its spacing now derives from the
  catalog length. Final full-suite rerun is green.
- Species tests span seeds 0, 7, 122, 907 and `u64::MAX`, multiple growth ages,
  connected axes, deterministic geometry, monotonic visibility, bounded leaves,
  no fruit and JSON restore. Ginkgo-specific checks cover the fan's base/lobes/
  notch, actual generated spray coordinates, upright taper and gray-bark style.
- Palette tests verify gold remains local, retains a height gradient, and applies
  user variation afterward. Bark tests include uncached fragment publication.
  Shop test buys the registered sapling and round-trips its stock and wallet.
- Standard `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed;
  inspected worktree log with `--tail-latest-log 35`.
- Native ginkgo review seeds **7, 122, 907**: passed real Grow selection/preview,
  placement, age 0.45 rebuild, serialized observer restore, return to the exact
  mature description and GPU pose/surface/lighting checks.
- Native mixed review: Apple, Pine, London Plane and Ginkgo coexist and all pass
  restore/GPU checks. Screenshots were opened and inspected: golden ginkgo,
  unchanged green neighbors, complete trunks/crowns and cast shadows. Artistic
  user approval and dense-forest performance acceptance are not implied.
- Final logs contain no ERROR, WARN, panic or VUID diagnostics; shutdown reports
  `phase=complete failures=0`. DDGI's ordinary relocation `failed=1081` counter
  denotes invalid buried probes, not a run/shutdown failure.
- No generated source or saved `config/gui.toml` changes. No visible game is left
  running. Release executable: `target/release/re-flora`.

Observed native publications (leaf counts are rendered voxel instances):

| Ginkgo seed | Axes | Leaves | Apples | Height, voxels |
|---|---:|---:|---:|---:|
| 7 | 250 | 12272 | 0 | 123.963 |
| 122 | 250 | 12249 | 0 | 126.495 |
| 907 | 250 | 12146 | 0 | 111.667 |
| 125, mixed scene | 250 | 12291 | 0 | 118.641 |

Transient test/build/native logs are under `target/ginkgo-*.log` and
`target/ginkgo-review/`; normal run logs are in `target/re-flora-logs/`.

## HTML presentation validation

The HTML presents the implementation evidence above, not new game measurements.
No game code, image pixels or generated files changed in the presentation step.

- Native Chromium review at 1440 × 1000 and 390 × 844; desktop hero/validation and
  mobile hero/gallery/validation screenshots were opened and inspected.
- Both disclosures work with pointer and keyboard. All expanded content also
  fits a 320 × 740 viewport without document-level horizontal overflow; wide
  commands/tables have keyboard-accessible internal scrolling.
- Both originals decode at 5120 × 2880 with unchanged SHA-256 values. Original
  image navigation works; local links and in-page destinations resolve.
- No page errors or external resource requests. No JavaScript, remote fonts or
  report build step; CSS is inline and images are relative to this folder.
- axe-core 4.12.1 WCAG 2A/2AA: zero violations and zero incomplete checks at the
  desktop and mobile review sizes, with both disclosures open. This automated
  check is not a blanket accessibility certification.
- `git diff --check` and `cargo build --release` pass. Game tests were not rerun
  for this HTML-only change. The isolated browser session was closed afterward.

## Self-contained image provenance

Both files below are **original native game captures**, PNG, **5120 × 2880**.
They are documentation/reference images, not runtime game resources. Generated
locally by Re: Flora's Release screenshot facility from this repository's
procedural world and tree code on 2026-10-29. No external photograph, stock image,
image-generation model, prompt or remote asset service was used. There is no
source URL/asset ID; the source is implementation `d753b383`.

| File | Original capture filename | Role / scene |
|---|---|---|
| [ginkgo.png](ginkgo.png) | `ginkgo-final.png` | Original, one mature ginkgo, seed 122 |
| [mixed.png](mixed.png) | `mixed-final.png` | Original, Apple/Pine/Plane/Ginkgo, seeds 122–125 from left to right |

Processing: byte-for-byte copy and rename only. **No crop, resize, color editing,
compositing or generated retouch.** The opt-in native species fixture hides UI
only for scheduled captures, freezes time of day at 0.43 and retains live wind.
Camera is the fixture's orbit pose; single-tree distance 0.8, mixed distance 1.8.
No `--perf`, forced present mode or edited user config. Different run-time wind
phases can make newly captured pixels differ despite the same tree seed.

Reproduce from repository root:

```sh
RE_FLORA_TREE_SPECIES_REVIEW=ginkgo RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/ginkgo-review/ginkgo-final.png \
  --screenshot-delay 4 --auto-exit 6

RE_FLORA_TREE_SPECIES_REVIEW=mixed RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/ginkgo-review/mixed-final.png \
  --screenshot-delay 4 --auto-exit 6
```

Use `RE_FLORA_TREE_SPECIES_SEED=7` or `907` for other shapes. Capture dimensions
follow the selected display; `--windowed` produced 2560 × 1440 on this machine.
The fixture is unsaved and does not overwrite the player's garden or GUI file.
For ordinary play omit both the review environment variable and screenshot flags.

SHA-256:

```text
9b977aff69c5756ce2cf706f578fbb171febc6d43f3995eb027bd127c08bd11f  ginkgo.png
1ad504c50a6efbb9db1ccbb0027d5a97c727664d97f49dfa8326dfb6e76a498b  mixed.png
```

Attribution: Re: Flora contributors. Repository art policy is **CC BY-NC-SA 4.0**
([license text](https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode));
preserve attribution, noncommercial and share-alike requirements when reusing
these rendered references. No additional third-party image was imported for this
bundle; this note does not grant separate rights to unrelated game assets.
