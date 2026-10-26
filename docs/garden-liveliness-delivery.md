# Garden liveliness delivery checklist

Requested scope (sequential, local commits only):

- [x] Butterfly: faster flight/wingbeats/turning; occasional curved return approach;
  active landing beats; feeding mouth anchored to the current flower pose;
  relaxed paired-wing opening of roughly 10–60 degrees, not a frozen pose.
- [ ] Replace the third plantable tree (weeping willow) with London plane
  (法国梧桐; Platanus × hispanica / × acerifolia), informed by botanical sources;
  preserve old save readability; inspect real game renders.
- [ ] Add a central-aim vacuum tool for nearby harvestable fruit and special
  ground plants; never collect ordinary grass, trees or terrain; reuse inventory
  transactions and persistence, with visible collection feedback.
- [ ] Add carrots with a stylized taproot/feathery rosette, selection, planting,
  growth, inventory and harvesting; inspect in-game appearance.
- [ ] Aggregate validation and current Release binary for later manual acceptance.

## Acceptance boundaries

Checks use real production interfaces, deterministic tests and native Release
runs. In-game screenshots count as renderer/appearance inspection, not the user's
subjective approval of animated motion. No push, official packaging, external
publication or unattended persistent garden/config writes are requested.

## Evidence

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
