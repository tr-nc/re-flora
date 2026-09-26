# Playable vine tick ownership

`src/app/core/climbing_plants/tick.rs` owns advancing one session vine. Production
play, accelerated native reviews, and cadence regressions call `VineTick::advance`.
`Plant` and its rod retain ownership of growth, pruning and transactional mechanics.
No solver, tuning, terrain abstraction or app scheduling changed.

## Interface decision

Two shapes were considered:

- A function receiving a Plant, two clocks, dirty/dependency records and action
  flags. This leaves reset and ordering knowledge with callers: a shallow extraction.
- An owner constructed with one seeded Plant, receiving snapshot facts and a
  play/review cadence. This hides the protocol and prevents replacement plants from
  inheriting another vine's clocks, actions or validation history. **Chosen.**

The module's interface is `new`, `advance`, `request`, `observe_edit`, and read-only
Plant/growth-blocked observations. `Snapshot` bundles an existing `Terrain` adapter
with its export dependencies; it is not a second terrain trait. `None` means the
export is unavailable. `Terrain::current()` is checked even when dependencies match.
Tuning contains only the live simulation controls, not an App-field mirror.

| Owner | Responsibility |
| --- | --- |
| App / `ClimbingPlants` | GUI, fixture/seed selection, grounding and seeding, world edits, collision export acquisition, camera, rendering, action text and logs |
| `CollisionPatchCache` / `Patch` | Bounded immutable voxel export, dependency readiness against the builder, implementation of the existing Terrain seam |
| `VineTick` | Two clocks, action coalescing/order, edit overlap, dependency comparison and revalidation retry, birth/motion order, held/publishable result facts |
| `Plant` / rod | Stable history, support/pruning, growth candidates, motion and transactional solver state |
| `Review` | Scenario progression, world-edit requests and assertions; not simulation timing |

Deleting `VineTick` would put the time/readiness/action protocol back into App and
its tests. The leverage is shared production and regression execution, not a moved
`impl App` or a forwarding facade. Locality keeps protocol changes in one module.

## Preserved semantics

1. Missing/stale exports hold the update without spending time or consuming actions.
   Time spent waiting is not accumulated as simulation debt.
2. On a current export, live search tuning and coalesced actions run first, in order:
   prune highest, prune to root, disconnect root. Revalidation follows when an edit
   overlaps backing/clearance or export dependencies change. Only successful
   revalidation acknowledges those facts, including on zero-time updates.
3. Play runs motion at 20 Hz. Each 50 ms quantum first emits the selected growth
   rate's births, then advances motion. Both clocks cap work at eight quanta and
   discard excess debt. Invalid/nonpositive time holds clocks; an invalid/nonpositive
   growth rate holds births, not motion. Fractional time and birth credit survive holds.
4. Review deliberately emits one birth (only while growing), then two 50 ms pose
   steps per sample. Completed reviews still settle twice with exploration disabled.
   Review samples neither consume nor add playable clock time.
5. Plant operations remain individually transactional, not one new frame transaction.
   Failed revalidation holds publication and retries later. Unavailable motion stops
   the loop and reports waiting; earlier committed actions/births/steps remain
   publishable, and already-issued clock quanta are not replayed. This preserves the
   existing bounded-export behavior instead of inventing whole-frame rollback.
6. New seeding constructs a new tick owner; world replacement drops it and does not
   silently author or seed a demo. Terrain edits, camera and render publication remain
   at their original App seam. The explicit test-scene isolation gate is unchanged.

## Regression surface and native checks

`tick/tests.rs` replaces the copied cadence/clock loops. Tests drive elapsed frames
through the production interface and compare complete Plants (including RNG and
rod state), not clock internals. Short single-step solver oracles pin birth-before-
motion and the review's two pose steps. Other cases cover bounded catch-up, no time,
invalid input, unavailable exports, stale matching identities, late stale validation,
edit/dependency retry, prune/regrowth/root recovery, action ordering and replacement.
Host tests still cover the real export cache, clicked surfaces, GUI action wiring and
world replacement. Existing Plant/rod invariants remain separate solver tests.

```sh
CARGO_BUILD_JOBS=4 cargo fmt --check
CARGO_BUILD_JOBS=4 cargo check
CARGO_BUILD_JOBS=4 cargo test climbing_plants:: -- --test-threads=4
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 \
  cargo run --release -- --hidden --mute --auto-exit 0.5
# Hold the same lock over the entire loop; do not nest acquisition.
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY CARGO_BUILD_JOBS=4 bash -c '
  for scene in flat hole outward inward slope ground pole 1 overhang; do
    RE_FLORA_CLIMBING_REVIEW=$scene cargo run --release -- --hidden --mute --perf --auto-exit 12 || exit
  done'
```

Require the completion markers described in [climbing_plants.md](climbing_plants.md),
not merely exit zero. Compare baseline/final review facts and inspect same-worktree
run logs for errors and shutdown failures. Delivery evidence is under
`target/improve-delivery/`; these checks are behavior evidence, not a performance
improvement or controller aggregate acceptance.

Implementation validation: fmt/check, 67 focused climbing tests (23 host/tick), and
Release smoke passed. All nine native reviews completed; their full review facts
matched baseline `618203af` exactly after removing log prefixes. Same-worktree logs
had no ERROR/panic/VUID or shutdown failures. GUI/camera hashes remained unchanged;
no generated files or solver files changed. The 13 existing check/build warnings remain.
