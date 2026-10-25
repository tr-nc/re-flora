# Garden tree HTML report / 树木交付报告

Open **[index.html](index.html)** in a current browser. It is a standalone, offline HTML report:
styles, interaction code and all 11 original PNG captures are embedded. Copying just `index.html`
retains the entire report, seed filters and full-frame image viewer. No server, CDN, npm install,
`target/` directory or network font is required. External reference links need a connection only
when clicked; repository-document links require the surrounding repository.

## Files and roles

- `index.html`: generated, portable report and **byte-exact original PNG archive**. Do not edit it
  directly. Each original is identified by its image's `data-capture` attribute.
- `report.template.html`: authored Chinese report text, layout and responsive/print styles.
- `report.js`: authored seed filter and accessible native-dialog full-frame viewer, embedded by
  the builder. Without JavaScript the report still displays its body and all original images.
- `build.mjs`: dependency-free Node builder; also verifies original PNG hashes on ordinary rebuild.
- `capture-manifest.json`: original filenames, roles, dimensions, byte lengths and SHA-256 hashes.
- This `README.md`: provenance, processing, reconstruction and usage limitations.

## Image provenance

All images were produced by **Re: Flora's native Release screenshot facility** in the local project
worktree. They are actual game renders, not stock reference photos or output from an image-generation
provider. Image model/provider/prompt influence/negative prompt: **not applicable**. No new image
model, generated plant texture, reference-image input or audio resource was used for this report.

Original capture implementation baseline: `2bc5b7dd` (`add apple pine and weeping willow garden trees`,
2026-10-25). Research baseline: `7a8b89c0`. Source repository is this repository; no external asset
service URL/asset ID applies. The original working capture folder was `target/tree-species-review/`,
but originals are now embedded in `index.html`, so the bundle remains self-contained when copied.

### Every embedded original

All 11 originals are **PNG, 1600 × 1000**. SHA-256 and exact sizes are in the adjacent manifest.

| Original filename / capture ID | Role | Scene / seed |
|---|---|---|
| `apple-7.png` / `apple-7` | Current native validation frame | Apple Tree, seed 7 |
| `apple-122.png` / `apple-122` | Current native validation frame | Apple Tree, seed 122 |
| `apple-907.png` / `apple-907` | Current native validation frame | Apple Tree, seed 907 |
| `pine-7.png` / `pine-7` | Current native validation frame | Pine, seed 7 |
| `pine-122.png` / `pine-122` | Current native validation frame | Pine, seed 122 |
| `pine-907.png` / `pine-907` | Current native validation frame | Pine, seed 907 |
| `willow-7.png` / `willow-7` | Current native validation frame | Weeping Willow, seed 7 |
| `willow-122.png` / `willow-122` | Current native validation frame | Weeping Willow, seed 122 |
| `willow-907.png` / `willow-907` | Current native validation frame | Weeping Willow, seed 907 |
| `mixed.png` / `mixed` | Current native integration frame | Apple/Pine/Willow, seeds 122/123/124 |
| `apple-first-pass.png` / `apple-first-pass` | Superseded candidate reference, not current behavior | First Apple candidate, seed 122; includes placement blueprint |

Seed-122 single-tree runs used `--screenshot-delay 4 --auto-exit 8`; seeds 7/907 and the final mixed
run used `--screenshot-delay 2 --auto-exit 4`. All used `--hidden --mute`, no forced present mode,
and the `player-default` screenshot preset with the fixture's camera applied afterward. Fixture
settings include tree size 20, age 1 with an intermediate 0.45 restore, fruit phase 0.84, manual
time of day 0.43 and enabled raster tree wind. The frame is time-based; GPU scheduling/wind/lighting
convergence can vary between runs, so an identical command is not a byte-identical image guarantee.

Example (from repository root):

```sh
RE_FLORA_TREE_SPECIES_REVIEW=apple RE_FLORA_TREE_SPECIES_SEED=122 RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/tree-species-review/apple-122.png \
  --screenshot-delay 4 --auto-exit 8
```

The first-pass Apple source state was an uncommitted intermediate candidate and was not retained as
its own source commit. Its image bytes and checksum are preserved here; the final implementation
cannot be used to reproduce that old shape by merely choosing the same seed. The first-pass frame
also has a placement blueprint absent from the final evaluation frames; it is **not a controlled
lighting/performance A/B comparison**.

### Processing / derivatives

- Originals are embedded as base64 data URIs without crop, resize, re-encoding, recoloring or edits.
- Card views use CSS only: original crop **760 × 740 at x=380, y=50**, at all viewport sizes. The
  browser scales the crop to the card. Full-frame dialogs show the embedded original unchanged.
- `mixed.png` is shown uncropped in the report. No separate contact-sheet derivative is imported.
- Browser QA captures in `target/tree-report-*.png` are transient validation artifacts, not runtime
  resources or additional embedded report images.

### Rights / attribution

Rendered content comes from Re: Flora and the assets already present in this repository. The root
[README](../../../README.md#license) distinguishes GPL-3.0 code, project-owned assets under
[CC BY-NC-SA 4.0](../../../LICENSE-ASSETS), and third-party assets under their own licenses. In
particular, the visible game's Pixelify Sans font has a [SIL OFL 1.1 note](../../../assets/font/README.md).
The HTML itself uses system fonts; it does not embed that font file. Preserve attribution to Re: Flora
and the applicable individual asset notices when sharing. No blanket new redistribution permission
for every rendered element is asserted; third-party and any unresolved underlying asset rights
remain subject to their original provenance/terms. Plant reference pages are linked, not copied as
image assets.

## Rebuild

```sh
# Rebuild from originals embedded in the committed HTML, verifying their manifest hashes:
node docs/reports/garden-tree-species/build.mjs

# Explicitly import a newly supplied capture directory instead:
node docs/reports/garden-tree-species/build.mjs --captures target/tree-species-review
```

The builder hashes the embedded interaction script into the report's Content Security Policy;
no external script/image/font requests are permitted. The first command is deterministic. An
explicit new capture import changes the manifest: update the baseline/provenance and verify that
the report's frozen metrics and claims still describe those captures before committing.

To extract any original without other project files (run in this report directory):

```sh
node --input-type=module -e '
import {readFileSync, writeFileSync} from "node:fs";
const html = readFileSync("index.html", "utf8");
for (const match of html.matchAll(/data-capture="([a-z0-9-]+)" src="data:image\/png;base64,([A-Za-z0-9+/=]+)"/g)) {
  writeFileSync(`${match[1]}.png`, Buffer.from(match[2], "base64"));
}
'
```

## Historical architecture clarification

At the shared-growth refactor stage, the report explicitly distinguished the **four legacy Garden
Trees then present in the fresh opening** from the Apple tuning description/default planting selection. The original statement that all startup
trees became Apple Trees was incorrect. A subsequent behavior-preserving shared-growth refactor is
summarized in the report, with [design](../../tree-growth.md) and
[148-case compatibility/native evidence](../../evidence/tree-growth-rules.md). All 11 embedded
original images and their manifest hashes remain unchanged. After regeneration, Chromium checks
at 1440 × 1000 and 390 × 844 confirmed the clarification text, all 11 decoded originals, working
all-nine filtering, valid fragment links and no document overflow or JavaScript/CSP errors. The
updated desktop/mobile sections were also viewed; the task-isolated browser was then closed.

## Follow-up opening and Test Tree retirement

Implementation `84883641` subsequently replaced the four opening trees with current Apples at the
same anchors and retired the independent Test/Garden Tree profile. Historical or missing-species
descriptions now convert to Apple, including transactional replacement of baked wood; source saves
are not rewritten on load. This is an intentional change after the invariant refactor, not a claim
that the refactor itself changed the opening.

The `#opening-update` section and planting/compatibility text describe the current behavior and link
[the new evidence](../../evidence/apple-opening-and-test-tree-retirement.md). The original 11 PNGs,
manifest hashes and original implementation metrics remain unchanged; the HTML does **not** embed
new opening screenshots. Regenerated size: **3,518,551 bytes**. Builder/script syntax checks passed.
Chromium at **1440 × 1000** and **390 × 844** confirmed the update text, no horizontal overflow,
valid fragment targets and all 11 originals decoding at 1600 × 1000 after explicitly requesting
lazy-image loading. No page JavaScript/CSP errors or external resource requests were observed.
Both updated-section screenshots were viewed; the isolated `retirement-report` session was closed.
The earlier filter/dialog and accessibility results below remain historical, not newly repeated.

## Validation

The original species-delivery checks belong to implementation `2bc5b7dd`; the later architectural
checks are explicitly marked as a follow-up. Neither claims a new game performance benchmark or a
full-suite run.

- `node --check` for builder and interaction script: passed.
- Rebuild using only the originals embedded in `index.html`: passed. Two successive builds produced
  identical SHA-256 checksums. All 11 originals matched their manifest hashes and decoded at
  **1600 × 1000** in Chromium.
- Native browser review via a task-isolated, headless `agent-browser` session: **1440 × 1000** desktop,
  **390 × 844** mobile, and **320 × 740** narrow-screen checks. Desktop and mobile captures were
  viewed; no horizontal document overflow, including all five expanded detail sections.
- Default seed 122 displays exactly three cards. Switching to 7, 907 and all-nine works. Full-frame
  dialog loads the matching original, accepts Escape and the close button, and restores focus to
  its opener. No browser JavaScript or CSP errors were recorded.
- All internal fragment links resolve. A standalone copy outside the repository decoded all 11
  embedded originals with **zero external resource requests**. The explanatory repository links
  intentionally still require the repository and are marked as such in the report.
- axe-core 4.12.1 WCAG 2A/2AA scan: **0 violations**, one incomplete contrast check covering seven
  text nodes overlapping clipped-image bounds in the analyzer. Viewed the affected text; explicit
  foreground/background contrast calculations were **13.43:1** for card headings, **10.19:1** for
  species subtitles and **10.16:1** for the section copy. This is not a blanket accessibility
  certification. Automated off-screen mobile clicks were rerun after explicit instant viewport
  positioning to avoid racing smooth scroll; the actual pointer/dialog interactions then passed.
- `git diff --check` and repository `cargo build --release`: passed. No Rust, shader, generated-Rust,
  runtime configuration or game assets were modified by the report change.

Transient browser captures: `target/tree-report-desktop-top.png`, `...-desktop-gallery.png`,
`...-mobile-top.png`, `...-mobile-gallery.png`, `...-mobile-dialog.png`. The isolated browser session
was closed after validation. Print styling is provided but print/PDF pagination and other browser
engines were not separately validated.
