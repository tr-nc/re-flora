# Garden expansion — specification, checklist and delivery log

Status: implementation and automated review delivery complete, with the acceptance limits below. Local-only; no new push or official packaging. Offline report: `docs/reports/garden-expansion/index.html`.

## Problem Statement

The Restorative Garden needs a more convincing night-time appearance, more plants
and small wildlife, useful physical gardening/building tools, and a lightweight
harvest-and-reinvestment loop. Current grass-covered soil causes severe editing
hitches and extends green too far outside the visible grass. Existing procedural
tree wind audio should use the newly supplied recorded assets instead.

The user requests autonomous, end-to-end delivery without intermediate questions.
This document is the durable checklist: incomplete work must remain visibly
incomplete, not be described as shipped merely because a design or stub exists.

## Solution

Preserve Free-form Planting and Gentle Consequence. Deliver each coherent change
through the real game, with tests, Release validation, appropriate screenshots and
local commits. Start by fixing the reported grass regression. Then add the night
experience, physical building and gardening tools, additional plants/wildlife,
and Seed Circulation with optional unlimited resources for debugging.

## User Stories

1. As a gardener, I want grass edits to remain responsive and green soil to stay
   within the grass patch rather than forming a broad green halo.
2. As a gardener, I want blinking fireflies at night; they may appear invisibly
   between flashes rather than requiring a visible entrance animation.
3. As a gardener, I want a moon sticker matching the existing sun's visual style.
4. As a gardener, I want the existing small blue flower to glow at night, with
   environmental illumination if it can be made affordable.
5. As a gardener, I want placeable Japanese-inspired carved stone lanterns made
   from real editable terrain voxels, with an emissive light inside.
6. As a gardener, I want to place varied upright poles, choose thin/thick poles,
   and freely connect two or more poles with rope to form fences.
7. As a gardener, I want to plant sunflowers.
8. As a gardener, I want colored butterflies, including perched butterflies, to
   remain legible instead of becoming excessively dark.
9. As a gardener, I want to drag a plow to cut a furrow and push the same soil
   toward the sides/front without creating or destroying any soil voxels.
10. As a gardener, I want to press to scoop soil with a shovel, carry it while
    holding, and release elsewhere to deposit it through the existing soil
    deposition behavior, with well-defined support/collapse and cancellation.
11. As a gardener, I want to aim at apples and collect them into persistent storage.
12. As a gardener, I want money and a shop for seeds, seedlings and tree saplings,
    while retaining an unlimited-resource debug mode.
13. As a gardener, I want planted trees to begin as saplings and grow over time,
    with convincing intermediate shapes rather than appearing fully grown.
14. As a gardener, I want bees and attachable hives using the existing cached-angle
    pixel-model post-processing path, not terrain voxel rendering. Hives attach
    to trees or poles, with a plausible garden-beekeeping arrangement.
15. As a gardener, I want recorded gentle/strong-wind tree sound assets to replace
    the current procedural tree sounds using the existing asset handoff guidance.

## Implementation Decisions

### Agreed requirements and invariants

- Soil remains Dirt; grass is an appearance derived from ordinary grass only.
  Flowers/trees do not turn soil green. Preserve moisture, collision and material.
- Fireflies and bees/hives share the existing cached-angle post-processed model
  path. Fireflies are night-only and blinking. Moon art should match the sun vibe;
  initial artistic quality is an iteration item, not a blocker for implementation.
- Reuse the existing small blue flower for nocturnal glow if appropriate. Shared
  illumination from flowers/fireflies is desirable, but may be omitted if it
  cannot meet reasonable performance/safety requirements; record the decision.
- Lanterns and poles are actual editable terrain geometry. Prefer the existing
  emissive voxel for lantern light and an existing material for poles. Do not
  replace an in-use material ID merely to make room.
- Poles vary per placement, with explicit thick/thin modes. Rope connections may
  form arbitrary multi-pole chains, not only one fixed fence prefab.
- Plowing conserves the exact quantity of soil. No disappearing/excess voxels
  when movement is blocked, canceled, out of bounds, or partially applied.
- Shoveling reuses soil deposition. Soil in transit is owned by the tool/storage;
  failed drops and cancellation must neither lose nor duplicate it.
- Apples, money, purchased items and tree age require coherent persistence.
  Unlimited resources is a debug option, not a replacement for the economy model.
- Newly planted trees start as saplings. Existing saves should retain existing
  mature trees/ages where practical, not reset all old trees to saplings.
- New settings use the shared saved declarative/settings controls. New assets
  include source, exact processing/generation details and rights/provenance notes.
- Read the recorded-tree-audio report/handoff and original asset metadata before
  changing the sound model. Preserve spatialization, wind response and safe gain.

### Proposed approaches, to confirm through implementation and evidence

- Grass: incremental root/terrain invalidation and sparse GPU coverage updates;
  use rooted cells and small enclosed gaps, not outward radius-four dilation.
- Night visibility follows the existing sun/day-night authority; avoid a second
  inconsistent time-of-day source. Bound dynamic lights and cached model counts.
- Moon: inspect the sun asset, derive a moon palette/sticker locally where rights
  permit, and retain the original input plus repeatable processing information.
- Lantern/poles use existing placement and terrain publication owners. Rope and
  hive anchors require stable identities and explicit deletion/load behavior.
- Plow/shovel share an authoritative soil-movement transaction/deposition seam,
  rather than separate ad-hoc remove/add operations with independent accounting.
- Hive husbandry and shovel collapse behavior need research/inspection. Choose a
  safe, bounded behavior and record limitations instead of blocking on questions.
- Shop economics start simple; avoid unrelated pressure, complex market simulation
  or loss of existing free-play/debug usability.

## Testing Decisions

- Test observable actions and invariants through real interfaces, not copied
  implementations. Fast deterministic logic tests for resource accounting,
  placement constraints, invalidation, inventories and transaction rollback.
- Release native runs are authoritative for performance. Baseline and compare
  identical scenarios; debug/unit timings are not performance acceptance.
- After Rust/shader changes: formatting, cargo check (regenerates shader ABI),
  relevant tests, Release build and hidden muted app run with inspected logs.
- Real-path fixtures for planting/removing/burying/restoring grass; plow mass
  conservation; shovel press/drag/release/cancel; harvest/shop inventory; and
  attachment removal/restore. GPU/window/audio checks are opt-in, not normal tests.
- Inspect screenshots for grass boundaries, night/moon/glow, lanterns, poles/rope,
  sunflower, perched butterflies, bees/hives, and several growing-tree ages.
- Listen/check levels and logs for recorded tree sounds under gentle/strong wind;
  record any unavailable listening or spatial/performance evidence.
- Distinguish pre-existing failures from regressions using baseline evidence.

## Checklist and Progress

Legend: `[ ]` not complete, `[-]` in progress, `[x]` implemented and verified.
Each completed item needs a delivery note with commit, validation, evidence and
known limitations; partially implemented items remain open.

| State | Item | Acceptance |
|---|---|---|
| [x] | 01 Grass hitch and boundary regression | Local edit cost independent of distant roots; tight green contour; deletion/burial/load/moisture preserved; comparative Release measurements and screenshot |
| [x] | 02 Recorded tree wind audio | Inspect history/handoff/assets; replace procedural source for gentle/strong wind; validate transitions, gain and spatialization |
| [x] | 03 Moon and night appearance | Matching sticker style, correct night visibility, provenance, inspected night screenshot |
| [x] | 04 Fireflies | Night-only blinking, bounded population, cached-angle post-process rendering; no disruptive popping while lit |
| [x] | 05 Glowing blue flowers | Night emission, ordinary planting/growth intact; decide/measure environmental lighting support |
| [x] | 06 Voxel stone lantern | Player-placeable real terrain, emissive core, lighting/collision/edit/save validation |
| [x] | 07 Poles and rope fences | Varied real-voxel poles, thick/thin option, arbitrary rope chains, valid anchors and persistence |
| [x] | 08 Sunflower | Registered, selectable, plantable, growth/rendering/persistence; asset provenance if added |
| [x] | 09 Butterfly brightness | Diagnose albedo vs light; improve colored and perched butterflies without washing out white ones; screenshots |
| [x] | 10 Plow | Drag furrows and displaced piles; exact soil conservation through boundaries/blockage/rollback; native validation |
| [x] | 11 Shovel | Press-scoop/hold-carry/release-deposit; reused deposition; soil ownership/cancel/load safety; explicit support/collapse behavior |
| [x] | 12 Apples, storage, currency and shop | Harvest/E, persistent wallet/stock/fruit receipts, real purchases and finite planting, preserved debug supply; native reload and fallen pickup. See `docs/garden-economy.md`. |
| [x] | 13 Sapling growth | Per-tree staged clocks, compatible old ages, edit/obstruction safety; native save/reload and four age captures. See `docs/garden-saplings.md`. |
| [x] | 14 Bees and attached hives | Original cached bee/house models; bounded flower visits and night return; real tree/post attachments, edit/burial/removal/reload validation. Solitary-bee house choice documented in `docs/garden-bees.md`. |
| [x] | 15 Aggregate validation | Release/check/format/native regression matrix completed; main tests 1759 passed, same 9 baseline failures, 14 ignored; integration 1 passed/4 ignored. Not an all-green certification. See `docs/reports/garden-expansion-validation.md`; no push. |
| [x] | 16 Final HTML report | `docs/reports/garden-expansion/index.html`: 25 bundled original screenshots (~29 MiB), provenance/hashes, native excerpts, reproducible metrics, commands and explicit incomplete acceptance. Local links/copy integrity and desktop/mobile browser checks pass. |

### Delivery log

- Player follow-up (2026-10-27): removed the lantern size slider after approval
  of 0.5×. New placements now use a fixed half-size constant; existing stamped
  lanterns remain unchanged. Legacy saved scale values and the empty placement
  section are retired on load/Save. The instant-mature tree checkbox is retained.

- Player follow-up (2026-10-27): added saved **Stone lantern scale (new lanterns)**
  in Construction Placement, range 0.5–2, default 1. Only newly stamped terrain
  uses the preset; geometry, window clearance and foundation allowance scale
  together. Half-size actual terrain save/reload retains 1,381 stone/27 emissive
  cells, open windows and collision roof ray.
- Added default-on **Plant mature trees instantly (new trees)** in Tree Planting.
  New finite/free placements start at age 1 without a clock; unchecked placements
  retain sapling growth. Existing adults and growing saplings are unaffected by
  later toggles. Native paid/free planting, growth independence and restart checks
  pass, along with settings migration/save tests and focused geometry/vegetation
  tests. Release rebuilt; see `docs/garden-tree-planting.md` and
  `docs/garden-stone-lantern.md`. No automatic visible launch.

- Player follow-up (2026-10-27): recorded breeze/strong-wind canopy audio is now
  the only implementation. Removed the A/B checkbox, eight unused synthesis
  controls and the procedural ambient loop/replacement path. Old true/false
  settings are retired at load and removed on explicit Save; mixer, wind response
  and +12 dB recorded calibration remain unchanged. Finite kick leaf-contact
  synthesis remains private to the kick module. Audio/schema/save/native lifecycle
  checks and six-stage five-canopy hidden Release review pass; Release rebuilt.
  See `docs/reports/recorded-tree-leaf-audio.md`. No listening/rights approval or
  new full-suite acceptance is implied; historical offline evidence is unchanged.

- Player follow-up: saved **Tree Planting** sliders now own future tree size
  (0.5–2x species-relative scale) and apple density (0–1, default 0.25).
  Preview/placement capture the settings; existing mature trees and growing
  saplings keep their original descriptions across subsequent edits and reload.
  Real slider interaction, settings migration/persistence, growth and native
  world save/restart checks pass. See `docs/garden-tree-planting.md`.

- Player follow-up (2026-10-26): sunflower stems raised from 68 to 136 authored
  layers, keeping the head and every other flower unchanged. At current reference
  controls, mature height rises from 12.14 to 22.39 voxels versus the 24-voxel
  player. Research, native before/after review and validation are recorded in
  `docs/research/sunflower-player-height.md`; the initial offline report's
  sunflower screenshot intentionally remains historical. Release rebuilt;
  14 Node flower, 36 Rust flora and 9 snapshot tests pass. User's running game
  was not interrupted; the new embedded asset requires a restart.

- Initial checklist recorded from the full user request; no items silently dropped.
- Grass baseline: previous implementation `ebd7c0f5` synchronously recomputes all
  roots. Native Release fixture, one root toggled 12 times: 1,024 roots median
  3.290 ms; 65,536 roots median 202.454 ms, max 206.667 ms, even with zero changed
  upload bytes in the dense scene. This establishes the reported hitch before
  changing the implementation. Incremental GPU replacement is in progress.
- Grass fix verified: sparse GPU updates touch nine cells per root delta with no
  terrain readback; 65,536-root one-root-edit median 0.854 ms / max 1.138 ms versus
  202.454 / 206.667 ms previously. Green no longer dilates the outline; only rooted
  cells and enclosed one-cell gaps are filled. Native acceptance and inspected
  tight-boundary screenshot pass. Coverage timings exclude the pre-existing
  planting/meshing work and are not a universal frame-rate guarantee.
- Recorded tree wind: existing eight-take wind-blended implementation from
  `f6f08d19` and handoff inspected; recorded mode enabled by default. Asset hashes,
  native lifecycle and five-canopy calm/weak/strong transition review pass; normal
  Release startup confirms recorded mode. **Commercial use remains blocked by
  supplied Free-plan asset rights; muted checks are not listening approval.**
- Moon: runtime cool-palette derivative of the original 16×16 sun, with unchanged
  original texture and a bundled provenance/rights note. Night-only full moon
  opposite the live sun; shared sky/glass composition. Inspected native screenshot
  `target/garden-moon.png`; check/build/hidden run pass. Initial small sticker,
  no lunar phases or dedicated moonlight transport claimed.
- Sunflower: seventh authored model appended without renumbering old plants;
  native Grow selection, cached model rendering, growth and snapshot schema use
  the shared registry. Gold pointed petals/brown disk, taller stem; inspected
  `target/garden-sunflower.png`. Flora and nine snapshot tests, check/build and
  hidden Release planting pass. Original procedural preset provenance recorded.
- Blue flower: the existing Forget-me-not emits from petal UVs only at night,
  in both cached and uncached model rendering. Two bounded nearby flower-cluster
  lights enter the existing direct/GI registry (global eight-light cap); emission
  remains visible on every blue flower. Stable sources do not restart GI; twilight
  light intensity is quantized and provider checks run twice per second. Inspected
  `target/garden-blue-night.png`, with a visible pool on the ground, and captured
  `target/garden-blue-day.png`. Three focused tests, check, Release build, hidden
  smoke and logs pass. Two 8-second Release runs, after frame 120: lights off
  median/p95 17.07/17.84 ms (448 logged frames), on 17.03/17.88 ms (452 frames).
  These historical records use threshold-triggered frame logging, not every frame.
  Final all-frame repetition (`RE_FLORA_GARDEN_PERF=1`, frame >120) measured off
  17.10/17.66 ms median/p95 (471 samples), on 17.10/17.88 ms (463). Early-frame
  maxima were 23.81/56.50 ms; the enabled outlier's cause was not isolated.
  Neither comparison establishes isolated GPU costs or dense-garden performance. Cluster lights approximate heads;
  not every petal is an individual physical light.
- Butterfly brightness: shared daylight-only paper-wing fill and a bounded
  chroma-dependent gain (up to 25%) keep colored/back-facing resting wings readable.
  No night emission, geometry, palette identity or behavior changes. Inspected
  fixed-pose before/after captures `target/garden-butterflies-{before,after}.png`;
  normal sunlit differences are intentionally modest. Fifty butterfly tests,
  check/Release build and hidden smoke pass. Native 18-second foraging review
  completed multiple distinct-host land/leave cycles (10 landings, 10 departures,
  8 host switches, zero stalls/lost hosts). Subjective brightness still needs
  player review; no claim that every lighting situation is approved.
- Fireflies: night-gated ecology births from committed grass/flower/canopy
  habitat, cap 32 and 45-second lifetimes, fade-in/out, distinct blink phases and
  four articulated wing poses through the existing model surface pre-cache.
  No terrain voxel/sprite substitution. Shared flying-insect cache/FPS controls
  keep existing saved IDs. Six focused tests cover lifecycle/blink/publication,
  real mesh and cached material ownership; ecology and cache regression tests pass.
  Inspected `target/garden-fireflies.png`; native day-transition retired all 25
  active flies, and a separate 32-second native run recorded seven natural canopy
  births through the real scheduler. No ERROR/panic/VUID. Default cache cost adds
  6 MiB (four sources, 128 views, 16x16, 48-byte samples). Optional environmental
  firefly light is omitted: rapidly moving/blinking actors are not fed into the
  persistent authored-light/GI revision stream. No measured claim that such GI
  is impossible; the flower-cluster light implementation is separate.
- Stone lantern: normal toolbar placement produces actual editable terrain:
  9,847 Rock cells, 150 Emissive cells, open windows, pedestal and pagoda roof.
  Existing terrain publication owns meshing/collision queries/light/save behavior.
  Boundary/obstruction/ground and primary-click tests pass. Native placement,
  binary snapshot save/reload, settled physical roof ray and core removal pass.
  Inspected `target/garden-lantern-{day,night}.png`; no final-run ERROR/panic/VUID.
  Always-lit first design, current global light budget; construction pricing is
  not implemented. No manual walk/collision-feel approval is claimed.
- Fence poles/ropes: Thin pole / Thick pole / Rope toolbar tools, varied actual
  Oak Wood terrain, arbitrary click-to-click chains and sagged face-connected
  ropes. Post IDs/recipes persist in a backward-compatible snapshot field; deleted
  caps retire their IDs and reject new links. Native three-post/two-link save/load
  and cap-removal checks pass; inspected `target/garden-fence.png`. Three focused
  tests plus persistence/toolbar checks and Release validation pass. Rope height
  is currently fixed below each cap. Existing ropes remain editable terrain when
  a post is removed—no cloth/collapse simulation or automatic deletion is claimed.
- Conservative Plow: cut three shallow layers and form side banks through the
  existing deposition planner, preserving exact Dirt quantity and moisture parcels.
  A single atomic region upload contains both removal and placement; a blocked or
  partially fitting plan is discarded before mutation. Native Release moved 2,074
  parcels while retaining exactly 110,592 soil voxels, including actual save/load.
  Inspected `target/garden-plow.png`; planner/input/toolbar checks and Release smoke
  pass. Existing soil stays cohesive; moved soil settles locally. Two 95,744-byte
  edits took 17.035/20.164 ms including publication—not a 60-FPS acceptance result.
  See `docs/garden-soil-movement.md` for the support policy and research limitation.
- Shovel: distinct press/scoop, held finite soil, captured release and reused
  deposition. One owner persists carried moisture parcels; failed/canceled/partial
  drops retain their remainder, even with unlimited debug resources. Native scoop
  moved 688: terrain 109,904 + held 688 = 110,592. Actual held save/reload and full
  drop restored 110,592; partial GPU drop placed 1 and retained 687. Four-moisture
  histogram stayed [27,648; 4]. Input/planner/persistence checks and Release smoke
  pass; inspected held/drop screenshots. Carry presentation is a simple brown
  clump/status, not a finished hand/shovel animation. No subjective input approval
  or sustained-drag performance pass is claimed.
- Economy: Harvest/E collects ripe attached or fallen apples; B opens the real
  shop. Stable fruit receipts prevent duplicate collection after growth, rebuild
  or reload. Bought seeds/seedlings/saplings feed finite placement; invalid and
  duplicate placement does not spend stock. Existing unlimited debugging leaves
  real stock untouched; shop purchases still spend coins. Native save/restart
  retained 23 coins, 1 apple, 2 harvest receipts, 1 sapling, 4 sunflower seeds,
  31 grass seeds, 3 planted flora roots and 2 trees. A separate live kick/release
  run collected a fallen body. Actual egui button/input tests and native UI capture
  pass. No recurring crop seasons, all-construction pricing, generic voxel-backpack
  persistence, dense-garden perf acceptance or manual interaction approval is claimed.
- Saplings: catalog trees start at age 0.375 and mature over 900 simulated seconds,
  with one staged publication at most per half second and stable per-tree clocks.
  Old saves retain their age without starting automatic growth. Native captures
  show 9.933 / 18.521 / 33.719 / 39.099-voxel Apple trees at four ages; actual
  middle-age save/reload and a normal-clock stage transition pass. Wood editing,
  manual age override and Rock obstruction freeze growth without undoing edits.
  Vegetation/tree-generation/persistence checks and Release smoke pass. First-pass
  saplings remain sparse; thin lateral wood can appear fragmented. Single-stage
  costs including preflight were 13.116 and 18.905 ms, not dense-garden acceptance.
  No offline catch-up, seasonal simulation or subjective art approval is claimed.
- Bees/houses: Bee house toolbar mounts one roofed solitary-bee nesting model per
  tree/pole, maximum eight houses and 64 transient residents. Both use the shared
  cached-angle model path; no terrain substitution. Stable mounts, canonical wind
  poses, flower visits, return/rest, day/night and deleted hosts are owned explicitly.
  Actual save/restart retained tree/pole attachments. Native support/cap/tree removal,
  burial (new Rock preserved), removed flowers, particles-off and uncached rendering
  pass. Normal scene and a clearly labeled bare-tree attachment diagnostic inspected.
  Default cache adds 7.5 MiB. One Release comparison, all frames after frame 300:
  no houses 16.96/17.98 ms median/p95; two houses/16 residents 17.02/17.94 ms.
  Not an isolated cost or dense/max-population performance acceptance. No honey,
  breeding, collision-route solver, dedicated house rigid body or subjective approval.
  Existing FPS label migration regression fixed separately in `89d267e3` using the
  declarative schema; both previously failing migration tests and all 52 butterfly
  checks pass. See `docs/garden-bees.md` and its primary-source research note.
- Aggregate acceptance at runtime code `6c5fe738`: full main suite 1759 passed,
  9 failed, 14 ignored; all nine failure names match the prior baseline. Separate
  library tests 4 passed; explicitly invoked integration targets 1 passed/4 ignored.
  Build/check/format and final-binary old-save reloads for economy, fences, plow,
  carried shovel soil, saplings and lanterns pass, as do bee reload, firefly dawn,
  blue-flower illumination, grass native acceptance and six-stage recorded audio.
  No final native ERROR/panic/VUID; GUI defaults unchanged. This is not all-green
  release certification, dense-garden performance acceptance or subjective approval.
- Offline report delivered in `docs/reports/garden-expansion/`: HTML has no script,
  CDN, font-download or image-network dependency. Twenty-five original PNGs and
  self-contained provenance/manifest accompany durable native evidence and raw
  selected performance samples. Integrity checks pass after copying the folder.
  Browser checks at 1280x900 and 390x844 decoded all images, found no horizontal
  page overflow or external resources, and exercised anchors, details, original
  image links and table keyboard scrolling. Selected WCAG 2A/AA checks report no
  violations/incomplete checks after contrast/focus fixes. HTTP(S) routes were
  blocked; a CLI offline toggle did not change navigator state, so hardware
  disconnection is not claimed. Manual usability and simultaneous full-resolution
  decoding of all screenshots are not certified. The report preserves all nine
  test-baseline failures, cost outliers, omitted features and player-feedback needs.
- Prior full test baseline: nine unrelated failures reproduced on `9e2b7977`
  (seven raster-tree tests and two saved-default expectations). Recheck rather
  than assuming any later failure belongs to that baseline.

## Out of Scope

- Remote publication, GitHub Issues/PR creation, official packaging and releases.
- Replacing all voxel encoding/material IDs, a general physics rewrite, or a
  realistic commercial beekeeping/economic simulation.
- Guaranteeing a polished final moon illustration in the first iteration.
- Expensive/unbounded per-firefly or per-flower global illumination. If omitted,
  emission remains visible and the performance rationale is recorded.

## Further Notes

- The user explicitly asks not to be interrupted for decisions. Resolve ordinary
  design details locally; record assumptions, limitations and blocked validation.
- Order after the grass fix can follow dependencies. Recorded tree sound is an
  emphasized priority and is scheduled early; this does not remove other items.
- Final deliverable includes a local HTML report (the user's “HDML report”), not
  only a chat summary. Bundle referenced screenshots locally so the report is
  usable without this conversation or remote services.
- A checklist is not evidence of implementation. Never mark an item complete
  solely because a setting, placeholder mesh, enum variant or design exists.
