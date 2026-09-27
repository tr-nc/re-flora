# Model frame, vine tick and DDGI capture acceptance

Controller acceptance: **2026-09-27**, local `opti`.
Validated code: `12d296f11676ee0a6280c92c94fcc9c2883d2e89`.
Implementation base: `618203afa0199dfdbed09715fb5b345b101d3a0f`.
No push, release, version bump, visible launch or changes to `main`/`pretty-leaves`.

## Delivered scope

- **P1 — Model Pixel Frame:** `ModelPixelFrame` owns preparation/publication and
  paired compute/draw resources for particle models, attached apples and fallen
  apples. The interface returns prepared draws instead of exposing allocation and
  pairing protocol to the renderer. Sorted complete metadata, frame-slot readiness,
  descriptor/extent retirement and the existing shared model sampling are retained.
  See [module decision](../model_pixel_frame_architecture.md).
- **P2 — Playable Vine Tick:** production and regression tests use the same
  `VineTick` time, action, terrain-readiness and revalidation protocol. Plant/rod
  mechanics and App scene/camera ownership stay separate. An active leaf review
  now excludes the automatic vine demo before world/camera side effects.
  See [module decision](../climbing_plant_tick_architecture.md).
- **DDGI correctness:** terminal visibility results publish valid sample-policy
  owner evidence without fabricating sample counts. An explicit stale-Active `e2`
  fixture waits for the eligible old field before issuing overlapping edits;
  ordinary publication scheduling and strict lineage checks are unchanged.
  See [diagnosis and fix](../research/ddgi_terminal_visibility_evidence.md).
- **Discovered diagnostic defects:** fixed the independent projection mismatch at
  64px, then the separate compiler-context mismatch at 16px/0.25 size. Native
  evidence now observes vertices inside the executed coverage loop, not another
  projection call or the GPU coverage mask. CPU classification/depth checks and
  original coverage epsilon remain strict. The intermediate supporting-material
  regression was reproduced and corrected before acceptance.
  See [64px evidence](../research/model_coverage_boundary.md) and
  [small-leaf evidence](../research/model_small_leaf_boundary.md).

Deleting the new ownership modules would restore protocol knowledge to their
callers; these are not file-only moves or forwarding facades. No atlas cache,
solver retuning, universal render transaction or effects registry was introduced.
**P3 God Ray/Lens Flare ownership remains deferred.**

## Controller-owned final checks

These checks ran on the integrated code after machine resume, using
`CARGO_BUILD_JOBS=4`, its own `target`, shared sccache, and the normal automatic
present mode. Every GPU invocation held the non-nested shared
`/tmp/re-flora-summer-gpu.lock`, with Release `--hidden --mute` and X11.

| Check | Result |
| --- | --- |
| `cargo fmt --check`, `cargo check`, Release build | Pass; existing warnings remain |
| `cargo test` | 1210 main + 4 build-policy tests passed; 4 existing ignored |
| `python3 scripts/run_slang_tests.py` | All 25 native Slang CPU tests passed |
| Targeted DDGI Python tests / Ruff | 90 tests passed; Ruff passed |
| Release default smoke / same-worktree logs | Pass; shutdown failures=0 |
| Fixed 64px and 16px regressions | Pass; original center and coverage checks retained |
| `validate-model-coverage-poses.mjs` | 906 deterministic, unscreened cases / 15 batches; every instance has checked samples |
| Original live leaf runner, `--seconds 20` | Three consecutive independent passes, PIDs 872471/872564/872657; 21/22/22 numerical checks; 8/16/64px and 0.25/4 size stages all nonzero |
| Original butterfly runner, `--seconds 20` | Pass, including all shadow/transmission stages |
| Apple/model stage-one runner | Orthographic A/B, five view counts, mixed resolutions, attached/fallen apples and actual fruit drops pass |
| Supplemental model-active resize | Three publications after fruit drop: generations 5/6/7 at 960×600, 1408×792, 1280×720; 684 coherent frames |
| Raster tree/resize smoke | A/B round trips, thin geometry, lighting, age/stiffness, removal/replacement all pass |
| Native vine flat / prune-recovery / overhang | Explicit verified completion, finite motion and collision checks pass |
| Original strict stale-Active `e2` capture | Two real captures and unchanged content/owner/lineage analyzer pass |
| Sustained terrain/light edits and indirect response | Both production fixtures pass, nonzero response and stable references |

The post-resume leaf sequence ran **14:09:03–14:10:10 +08:00**. The worker's
sleep-spanning run is retained but is not substituted for this sequence or treated
as active runtime/performance evidence. All final native logs were inspected for
ERROR/VUID/panic and shutdown failures; GUI/camera hashes are unchanged. No generated
or configuration files changed in the implementation diff.

Both `e2` artifacts contain geometry 2 / token 1 / field 3 / epoch 2, sourced from
field 2 / epoch 1. Recording occurs while geometry 3 is rebuilding and terrain 4
is queued. Both complete files have SHA256
`436f6a7ad3b10798f5a0c4d4b4152093ae96986e3f60e995ca313755f18b89f5`.
The controller's complete fixed64 final and center RGBA/depth tiles also match the
original captured bytes, not merely coverage masks or selected pixels.

Release fixture measurements: 40 edits, 10 light toggles, 22 useful publications,
206 ms median publication gap, 281 ms final catch-up. Indirect response on
10/50/90%: 304/304/304 ms; off: 314/314/521 ms. These are fixture measurements,
**not a general FPS improvement or cross-driver performance claim**.
Validated binary SHA256:
`aaab02a7bc041293476558c1e6258b0cd4dc2f66ab67e9670bc987d4057ab14b`.

## Retention and cleanup

Local evidence is under `target/improve-acceptance/` (not tracked build output):

- `awake-final/`: commands, original logs, captures, screenshots, strict analyses,
  hashes, and machine-checked `acceptance-facts.json`.
- Earlier controller failures remain, including both leaf counterexamples and the
  asynchronous auxiliary resize-driver failure. The latter had already produced
  a correct app resize; the driver was corrected to await WM acknowledgement.
- `evidence/{model-frame,vine-tick,ddgi-capture}.tar.gz`: worker red/green evidence,
  raw captures, discarded probes/negative controls, commands and handoffs. Archives
  were integrity-checked and compared with source files before removing worktrees.
  `SHA256SUMS`, workflow results, briefs and the original HTML architecture report
  accompany them. Only evidence was retained, not whole worktrees/build caches.
- `verify-accepted.mjs` rechecks final facts against retained original control tiles
  without depending on removed worker paths. `run-awake-native.sh` records the final
  native matrix and calls the unchanged strict runners.

All three source tips were clean, inactive and ancestors of accepted `opti`:
model-frame `6de821f5`, vine-tick `184e41c9`, DDGI `3fb8fc09`.
The three task-owned worktrees and corresponding local branches were removed
normally after acceptance. Unrelated worktrees and remote references were untouched.

Scope limits: one native GPU/driver was validated; ordinary apple/orthographic
consumer checks are not a new per-pixel oracle. Direct observer-seam clipping tests
would strengthen future coverage. No unresolved correctness gate is waived by
these limits, and the earlier failed runs remain available for audit.
