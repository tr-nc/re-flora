# Garden liveliness delivery checklist

Requested scope (sequential, local commits only):

- [x] Butterfly: faster flight/wingbeats/turning; occasional curved return approach;
  active landing beats; feeding mouth anchored to the current flower pose;
  relaxed paired-wing opening of roughly 10–60 degrees, not a frozen pose.
- [x] Replace the third plantable tree (weeping willow) with London plane
  (法国梧桐; Platanus × hispanica / × acerifolia), informed by botanical sources;
  preserve old save readability; inspect real game renders.
- [x] Add a central-aim vacuum tool for nearby harvestable fruit and special
  ground plants; never collect ordinary grass, trees or terrain; reuse inventory
  transactions and persistence, with visible collection feedback.
- [x] Add carrots with a stylized taproot/feathery rosette, selection, planting,
  growth, inventory and harvesting; inspect in-game appearance.
- [x] Aggregate validation and current Release binary for later manual acceptance.

## Acceptance boundaries

Checks use real production interfaces, deterministic tests and native Release
runs. In-game screenshots count as renderer/appearance inspection, not the user's
subjective approval of animated motion. No push, official packaging, external
publication or unattended persistent garden/config writes are requested.

## Final aggregate validation

All four features were rechecked together at `70a302b6`, in the main worktree.
The four independent implementation commits are `0d636289` (butterflies),
`d2d73f5a` (plane), `ec18b7b8` (vacuum), and `70a302b6` (carrots).
No push or package/release operation was performed.

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
  Current executable: `target/release/re-flora`.
- Full Rust suite: 1707 passed / 13 ignored / 8 existing baseline failures;
  details and starting-commit reproduction are recorded below. **The full suite
  is not green.** `target/liveliness-carrot-all-tests.log` contains the results.
- All 64 model-preview Node tests passed, including exact published asset
  regeneration and crop geometry/studio separation:
  `target/liveliness-final-node-tests.log`.
- Final native Release runs used hidden/muted windows and the real Vulkan path:
  - `target/liveliness-final-butterflies.log`: complete visit cycle, distinct hosts.
  - `target/liveliness-final-plane.log`: preview/placement, growth, serialized
    restore, GPU pose/surface/lighting (seed 907).
  - `target/liveliness-final-vacuum.log`: batch plants + ripe apple, occlusion,
    grass/tree preservation, duplicate prevention, save/reload.
  - `target/liveliness-final-carrot.log`: finite planting, canonical and scheduled
    growth, mature-only vacuum/manual harvest, produce/sale, save/reload.
  All four reported their acceptance marker and shutdown `failures=0`; no
  ERROR, panic, VUID or validation-error lines. The ordinary hidden/muted 0.5s
  smoke also passed (`target/liveliness-carrot-smoke.log`).
- Working tree remained clean after the final runs; diagnostic settings were
  not saved. No visible game was left running. For later manual acceptance,
  launch `cargo run --release` from `/home/terence/code/re-flora` (without
  `--hidden` or `--perf`). Animated feel and subjective art approval remain
  the user's acceptance, not something screenshots/tests can certify.

## Evidence

### Carrot

- [Research and modeling decisions](research/london-plane-and-carrot.md#carrot)
  use NC State Extension and RHS. Original tapered root + divided basal rosette
  recipe, published through the shared pixel-model bank. First-year carrot,
  not a flower/nectar host. Seven leaves, two greens, restrained orange root
  shoulder; most of the root remains underground.
- Grow → Flora → Carrot, finite seed purchases, canonical growth, Harvest or
  Vacuum, saved produce stock and Shop sale. Immature crops are protected;
  full inventory checks happen before world removal; repeated collection cannot
  duplicate a harvested root. Root crops cannot be bought/recycled as seedlings.
- Fixed the growth owner needed by seed-grown authored plants: the ordinary
  chunk queue now advances their CPU-owned age and republishes it, including
  grass-free chunks. Reload/other publications retain age rather than resetting
  it to the planting value. Existing model flowers also benefit.
- Inspected two actual Release images, thickened initially disappearing stalks
  and joined the crown for the current pixel budget. Final image:
  `target/liveliness-carrots-full.png`. This is stylized; growth is uniform
  scaling, not physiological emergence or a real agricultural calendar.
- Checks: fmt/check, three carrot policy/migration tests, thirteen model tests,
  fifteen Node publication/geometry/studio tests, Release build. Native fixture
  verifies finite stock debit, 0→128→255 age, real scheduled growth, partial-age
  reload, immature protection, vacuum + manual Harvest, exact-once produce,
  inventory/layout reload and sale. `target/liveliness-carrots-scheduled-console.log`
  contains `CARROT_REVIEW passed`, `scheduled_growth=true` and shutdown
  `failures=0`, with no ERROR/panic/VUID.
- Full Rust run: 1707 passed, 13 ignored, the same eight baseline failures
  documented below; no new failing test. Log: `target/liveliness-carrot-all-tests.log`.
- Reproduce: `RE_FLORA_CARROT_REVIEW=1 RUST_LOG=re_flora=info cargo run
  --release -- --hidden --mute --auto-exit 6`. Generated `flowers.json` and
  shader-derived `gpu_structs.rs` are included; neither was hand-edited.
  Asset provenance is in `assets/models/README.md`.

### Vacuum

- Toolbar **Vacuum**: hold left mouse, aim at screen center in either camera
  mode. Bounded forward cone, 1.25-world-unit reach, radius capped at 0.16;
  up to 16 items every 0.12 seconds, no catch-up burst after a stall.
- Takes ripe attached/fallen apples and authored special plants only. Grass,
  terrain, trees, unripe apples and other scenery cannot enter the collection
  transaction. Flowers become reusable seedling stock in Shop; no invisible
  second backpack or unpersisted pickups.
- Canonical identity is removed and inventory credited in one synchronous
  ownership transfer, before fallible GPU refresh or cosmetic flight. Full
  stock/missing owners cannot destroy a plant; repeated pulses and save/reload
  cannot credit it twice. Scene visibility is one bounded native ray batch per
  pulse, including posed wood and fixed-model surfaces, not a through-wall radius.
- Cosmetic colored voxel streams curl toward the camera center. UI/focus loss,
  tool switching and releasing the primary button stop suction, not committed
  inventory. Ordinary pointer/radius brushes are not reused accidentally.
- Checks: fmt/check, vacuum/tool-routing and economy transaction tests, toolbar
  rendering tests, Release build. Native fixture passed three-plant batch,
  retained grass/trees, solid-ground occlusion, exact-once removal, inventory +
  flora snapshot round-trip, and real ripe-apple collection.
  `target/liveliness-vacuum.log` has `VACUUM_REVIEW passed` and shutdown
  `failures=0`, no ERROR/panic/VUID. Inspected `target/liveliness-vacuum.png`.
- Reproduce with `RE_FLORA_VACUUM_REVIEW=1 RUST_LOG=re_flora=info cargo run
  --release -- --hidden --mute --auto-exit 6`. No generated files/new assets.

### London plane

- Research: [botanical references and translation](research/london-plane-and-carrot.md).
  Third Grow/Tree and shop entry is now **London Plane**. Existing willow trees
  are load-only legacy geometry; unplanted willow goods migrate to plane goods.
- Unequal ascending branching, clear lower trunk, wider palmate-inspired foliage
  masses, and cream/olive/brown rest-anchored exfoliating bark. Shared growth,
  preview, wind, physical ownership and snapshots remain authoritative.
- Inspected two native Release renders and enlarged the initially too-thin
  foliage sprays. Final capture: `target/liveliness-plane-full.png`, seed 907,
  206 axes / 35,699 leaf voxels / zero apples / 167.56-voxel height.
  It is intentionally a stylized unpollarded plane, not a photo-real botanical model.
- `cargo fmt --check`, `cargo check`, species tests, uncached bark-publication
  test and `cargo build --release` passed. Actual game fixture verifies preview
  equality, age rebuild, serialized restore, GPU pose/surface/lighting and
  clean shutdown (`target/liveliness-plane{,-full}.log`).
- Full test run: 1701 passed, 13 ignored, 8 failed. **All eight failures reproduced
  on untouched starting commit `fe4d71ac`** in a detached temporary worktree:
  seven old raster-tree fixtures assume former Apple dimensions; one saved
  apple-size test expects 1.0. Baseline logs:
  `target/liveliness-baseline-{raster,apple}.log`. Not hidden or “fixed” by
  changing unrelated defaults. Temporary worktree was removed.
- No new external assets or generated tracked files. Shader checks compile the
  actual native rendering path; no claim of a new performance budget pass.


### Butterfly

- Flight now runs at 7.5 wingbeats/second (formerly one authored cycle/second).
  Saved self-speed is 0.65 instead of 0.25, turning sharpness 2.2 instead of 1.
  Navigation remains fixed-step with terrain/world collision guards. Seeded
  occasional one-turn approaches converge back to the selected moving flower;
  capture retains full flight beats for 0.30 seconds before feeding.
- Feeding uses the original wing mesh with independent 10–60 degree opening,
  and a small bent proboscis. Its model-local tip, rather than body center, is
  anchored to the same current GPU flower pose used by native/cached rendering.
  Pose phase is held by the presentation clock; live host sway remains current.
- `cargo fmt --check`, `cargo check`, 55 butterfly-focused tests and
  `cargo build --release` passed. New tests cover changing wings with exact
  host following, both direct/circular routes and mouth/wing contact bounds.
- Hidden muted 0.5 s smoke passed. Production visiting fixture completed a
  full cycle across distinct hosts; close-up captures show the butterfly
  feeding at the flower center. Logs have no ERROR/panic/VUID, shutdown
  `failures=0`. Capture: `target/liveliness-close-top.000001.png`;
  logs: `target/liveliness-{visits,close-top}.log`.
- Reproduce: `RE_FLORA_BUTTERFLY_FORAGING_REVIEW=close RUST_LOG=re_flora=info
  cargo run --release -- --hidden --mute --screenshot garden-arrival
  target/feeding --screenshot-delay 4 --auto-exit 6`. The fixture changes only
  in-memory controls/camera; use `visits` for the complete-cycle assertion.
  No new settings UI, shader ABI, generated files or binary assets.
- Still subject to the user's animated/subjective acceptance. At normal game
  size the proboscis is deliberately subtle, not an exaggerated black line.
