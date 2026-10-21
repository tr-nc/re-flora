# Garden delivery — scope v3

## Authority and target

User authorized autonomous, parallel workflow delivery of sprinkler performance, sound,
a new garden opening environment, and visible plant moisture response. No further routine
questions are required. Target is local `main` in `/home/terence/code/re-flora`, starting at
`2a133114`; existing local commits are retained. No push, release packaging, paid SFX service,
visible window, or audible playback is authorized. All app runs are Release, hidden and muted.

Preserve existing settings, camera snapshots, and user terrain/flora saves. New controls use
the declarative/saved-field UI and additive migrations. New garden generation must not replace
loaded saves. Source changes, tests, documentation and generated build output are committed
in focused validated steps. The controller owns integration and final acceptance.

## Outcomes and invariants

1. **performance** — reproduce the reported frame-rate loss with 0/1/4/8/16 sprinklers;
   report CPU/GPU breakdown and marginal/nonlinear costs using matched Release runs.
   Optimize measured bottlenecks in priority order, not speculative cleanup. Preserve the
   shared airborne/contact water source, near/far coverage, terrace wetting, valve dose,
   optical controls, and all four hardware designs/16 complete poses. Disabling effects,
   reducing the user's saved quality, or silently reducing irrigation is not a fix.
2. **audio** — actual spatial sprinkler sound, with continuous and intermittent operation,
   clean starts/stops/removal/pause/mute and bounded voices. Procedural original placeholder
   SFX is acceptable. Research free/open-source AI SFX tools, code/weight/output licenses,
   hardware needs and suitability before choosing. No unlicensed/noncommercial-only game
   assets or credentials/service registrations. One authoritative emission/valve clock;
   do not create a drifting independent audio sprinkler simulation.
3. **world** — a new, attractive, editable Restorative Garden opening, with an accessible
   starting clearing, room for Free-form Planting, gentle terrain, trees, grasses and flower
   groupings. Deterministic seed and sensible bounded population; preserve existing worlds,
   special scenes and flat diagnostic generation. Research intra-/interspecific spatial
   patterns and habitat filtering from primary papers; distinguish ecological evidence from
   deliberate gameplay/art approximations. Inspect real screenshots and refine composition.
4. **wetness** — clear, low-cost visible material response when grass is watered, compatible
   with drying and the existing moisture source. Prefer darkening/color/cheap directional
   highlights over reflection passes. No second painted wet footprint, ray-traced reflection,
   per-blade CPU water particles, or expensive per-pixel sprinkler scans. Inspect dry/wet
   comparisons and measure the incremental Release cost. Explain the actual moisture proxy.

## Schedule and resource ownership

- Research first: three independent background workers, each owns one cited Markdown note
  and a repository seam recommendation. No compilation, game, GPU or SFX inference during
  this phase.
- Performance measurement/optimization next, alone: no other heavy work is scheduled.
- Then implementation slices, after their shared-file/clock/startup contracts are resolved.
  Shared files have one owner or are edited in serialized stages. Workers never share a
  writable checkout. Read-only review can inspect an existing checkout.
- All later builds/tests/apps/inference use the same heavy-resource lock:
  `flock -o /tmp/re-flora-garden-delivery-heavy.lock <command>` (hold one lock around compound
  shell commands, avoid nested lock acquisition). No simultaneous game processes. No forced
  present mode. Validation layers for correctness; disable layers for performance measurement.
- Build outputs are separate per worktree; never symlink/hardlink mutable `target` trees.
  Btrfs copy-on-write copies of the idle integration build cache are disposable accelerators.

Task-owned disposable worktrees/branches:

| ID | Worktree | Branch | Initial scope |
| --- | --- | --- | --- |
| performance | `/home/terence/code/re-flora-agent-garden-performance` | `agent/garden-performance` | sprinkler profiling/fixes, harness, performance report |
| audio | `/home/terence/code/re-flora-agent-garden-audio` | `agent/garden-audio` | `docs/research/sprinkler-audio-tools.md`, then sound implementation |
| world | `/home/terence/code/re-flora-agent-garden-world` | `agent/garden-world` | `docs/research/garden-spatial-ecology.md`, then opening generation |
| wetness | `/home/terence/code/re-flora-agent-garden-wetness` | `agent/garden-wetness` | `docs/research/vegetation-wet-materials.md`, then wet appearance |

Each worker keeps evidence under its own `target/garden-delivery/<id>/`. Before cleanup,
needed logs, summaries, audio previews and screenshots are copied to the same relative paths
in the integration worktree. Keep only useful evidence, not an extra full build cache.
After accepted integration, remove these clean task-owned worktrees and local branches;
never remove unknown work, user saves or pre-existing artifacts.

## Acceptance and handoff

Every worker reports `id`, `scopeVersion`, `path`, `branch`, `commits`, `delivered`,
`implementationBlockers`, executed `checks` with results/evidence, `pendingAcceptance`,
`resources`, generated-file changes, and shared-file/interface changes. Null/missing output
means missing coverage, never success. Research alone does not count as implemented behavior.

Required aggregate evidence:

- `cargo fmt --check`, `cargo check`, appropriate deterministic Rust/Slang regressions,
  and current `cargo build --release` in integration worktree.
- Hidden muted native runs and actual log inspection (normal shutdown, no Vulkan errors).
- Matched before/after sprinkler population measurements, controls/resolution/GPU/warmup/
  repetitions disclosed; no performance conclusions from debug/unit-test timings.
- Existing spray/contacts/droplets/terraces/lifecycle/appearance contracts preserved.
- Sound lifecycle/signal checks and listenable preview assets; no claim of human listening
  approval when none occurred.
- New-garden startup and save compatibility, bounded deterministic distributions, viewable
  overview/player-scale screenshots, real dry/wet grass comparisons.
- Final report `docs/reports/garden-delivery.md` in Chinese with shipped behavior, evidence,
  before/after numbers, sources/licenses, known limitations and remaining priorities.
- Local commits only, clean target and task-owned cleanup. Visual approval remains the
  player's decision; honest remaining performance limits are not concealed by the artwork.

## Stage 1 integration and stage 2 contracts

Research and the complete performance branch (through `0c625974`, including the logical
workgroup correction) are integrated. Raw evidence was copied to integration
`target/garden-delivery/`. Two exact optimizations are retained; no quality/water samples
were removed. Isolated GPU frame improvements are real but FIFO-limited FPS and the full
player-world symptom are not yet accepted. Final garden-context checks remain required.

The configured `small`/`medium` provider lacked credentials. This task uses the **same
GPT-6.1 Sol model and low/medium reasoning** through the available OpenAI provider via
per-call overrides, not a model upgrade or global configuration change. The initial null
run is failed coverage, not successful research.

Next, world and wetness implementation may run concurrently with these exclusive owners:

- **world** owns the new garden recipe, terrain/startup/placement and launch policy,
  `src/cli.rs`, `src/main.rs`, `src/app/core/{mod,loading,launch_owners,vegetation}.rs`,
  relevant terrain/placement sources and its dedicated review/harness. Use a separate
  garden recipe data file if useful. Do not edit `config/gui.toml`, GUI generators,
  `src/tracer/mod.rs`, grass material/render input files, or audio/PetalSonic.
- **wetness** owns grass support/material helpers and cached/native shader consumers,
  additive grass-only controls in `config/gui.toml`, their GUI migration/grouping and
  render-input plumbing (including narrowly the frame input structs in `src/tracer/mod.rs`).
  Native validation should extend existing `src/app/core/grass_stem_review.rs` so it does
  not edit world-owned `src/app/core/mod.rs` or startup/placement policy. Keep existing
  growth helper interfaces compatible. Generated-file overlaps are resolved by regeneration.
- Neither edits the other's owned source files. If a hook is absolutely necessary, record
  a precise small patch request and let the controller serialize it rather than guessing
  at uncommitted files. Each slice must still wire actual user-visible behavior.
- Builders, native runs and timing experiments use the common `flock -o` heavy lock;
  source authoring/research can overlap. No nested locks. Never benchmark while another
  worker compiles, runs tests, launches a game or performs inference.

**Audio is implemented after world/wetness integration**, not concurrently in their shared
files. Select original procedural DSP; restricted/unverified AI weights are not acquired.
Choose the smaller authoritative-clock seam: expose a typed read-only snapshot of the
exact `SprayClock::sample` result already uploaded by Tracer, including normalized valve
phase/rate/duty and rotor angle; consume it at the existing end-of-frame spatial transaction
with committed stable sprinkler IDs and world pause status. Never sample/advance a second
clock or derive the envelope from optical density/wetting gain. Add sample-domain anchored
voice modulation in PetalSonic only as needed for 20 Hz/low-render-FPS correctness, with
bounded stale-state silence and no realtime allocation/locks. Retain publish-before-play,
clean failure/removal ownership and a bounded total voice count. The final audio owner must
verify ordering and may refine snapshot fields, but not replace this with a free-running
unanchored loop or frame-rate-only gate.

## Stage 2 integration and stage 3 ownership

World commits through `71d7d528` and wetness commits through `2f671e8e` are integrated
at `97b99d77`. Controller inspected source and actual arrival/overview and wet-grass
comparison images; useful evidence is copied into integration. Aggregate formatting,
check, 1,663 application + 4 library + 1 integration tests, default Release build and
fresh-garden hidden muted smoke passed (`stage2-*` logs). Existing GUI values and camera
library are unchanged; only the saved grass-moisture checkbox was added. This is a
candidate visual result, not player approval. The new garden's additional rendering cost
and wetness's remaining native/performance matrix are recorded in their slice reports.

Stage 3 has two independent Medium implementation units:

- **audio** uses `agent/garden-audio`. Owns sprinkler sound/synthesis and PetalSonic envelope
  support, exact Tracer clock snapshot/accessor, committed source facts, spatial transaction,
  and necessary audio-only wiring in `src/app/core/{mod,placeables,lifecycle}.rs`. It may add
  declarative audio controls/migration and an opt-in audio review fixture. Preserve existing
  clocks, shader/water contracts, presets and new garden/grass behavior. It must not edit
  terrain persistence, water publication or world generation.
- **empty_load** reuses `agent/garden-world`. Owns diagnosis and root-cause repair of the
  observed all-empty snapshot runtime-replacement timeout, in terrain persistence/smoke,
  water coordination/colliders and their authoritative lower-level helpers. The original
  red evidence is `world/implementation/reload-minimal-empty-fixed.log`, with the disposable
  `audit/minimal-empty.rflterrain` input. Establish whether this reproduces before garden
  changes instead of assuming it is pre-existing. Do not weaken the smoke, extend timeouts,
  skip water readiness, or alter snapshot contents to hide the bug. Do not edit audio,
  GUI, `src/app/core/mod.rs`, `src/tracer/mod.rs`, or the shared clock. Request a precise
  serialized patch if an indispensable cross-owner hook is found.

Both start from the integrated scope-v3 plan commit, use independent COW build caches and
hold the shared heavy lock for builds/tests/apps/timing. Source authoring may overlap.
A source report returning is not acceptance; controller must inspect real evidence and
finish full-scene sprinkler measurements, integrated lifecycle checks and final handoff.
