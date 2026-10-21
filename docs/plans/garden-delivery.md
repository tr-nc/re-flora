# Garden delivery — scope v1

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
  `flock /tmp/re-flora-garden-delivery-heavy.lock <command>` (hold one lock around compound
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
