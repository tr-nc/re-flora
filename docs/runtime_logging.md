# Runtime logging contracts

Normal runs do not report each DDGI history batch, local footstep play/render/completion,
wet-path success, cicada call/retirement, routine acoustic response, or ecology supply/birth/timing. These call sites
are removed, not moved to a different logger level or hidden behind a new setting.

Audio still consumes lifecycle, voice and acoustic telemetry in the same order. Correlation,
first-render checks, bounded conclusion deadlines, deferred emitter cleanup and telemetry
ownership release are unchanged. Unowned/stale telemetry is counted by the router without
printing each event. Wrong-emitter and first-render contract errors remain visible.

The old `[AUDIO][LOCAL_FOOTSTEP_WET]` report is replaced by a failure-only
`[AUDIO][LOCAL_FOOTSTEP] ... wet_path_failure=...` warning for dropped/missing telemetry or
an environment send excluded by budget. Its observation includes the response, acoustic
conclusion and energy summary. Successful or explicitly disabled acoustics do not log.
Playback/publication failures, missed completion deadlines, voice-cap pressure and cleanup
failures remain warnings/errors. Runtime/device transitions, initial mix/assets, cicada
clear/shutdown totals and the final acoustics summary remain lifecycle evidence.

## Startup progress

Ordinary launches include `[STARTUP]` records on the existing `re_flora::run_summary`
INFO target; no `RUST_LOG` or `--perf` is needed. The App owns one startup reporter
from window/Vulkan initialization until the first successfully presented game frame
has completed on the GPU. It reports real stage boundaries (`begin`/`done`), stage
and total wall-clock milliseconds, and `waiting` every two seconds during a long
stage, including synchronous driver calls blocking the window thread. `waiting`
is a heartbeat, **not proof that the driver is making progress**, an ETA, or a
percentage. Stage names separate shader modules, GPU resources, compute/graphics
pipelines, geometry, terrain/model caches, world population/lighting, and the first
frame.

`ready` is emitted only after first-frame completion. Early return or unwinding
emits `aborted`, not success. The reporter is woken and joined on either path; it
never accesses rendering/world state and does not remain alive during gameplay.
Explicit `RUST_LOG` filters still control these INFO messages.

For per-pipeline **CPU time inside `vkCreate*Pipelines`** (including enabled
validation layers and the driver), opt in to:

```bash
RUST_LOG=warn,re_flora::run_summary=info,re_flora::run_log_binding=info,re_flora_vkn::pipeline=debug \
  cargo run --release -- --hidden --mute --auto-exit 0.5
```

`[PIPELINE_CREATE]` includes compute shader or graphics vertex/fragment names and
`create_ms`. This excludes resource/descriptor preparation, is not GPU frame time,
and does not prove a cache miss or a particular driver-internal compilation step.
Normal startup progress does not print every shader. No pipeline-cache policy,
shader quality, scene defaults, or persisted preferences are changed by logging.

Check a completed normal startup log with:

```bash
cargo run --release -- --latest-log
node scripts/check-startup-log.mjs <path-printed-above>
```

The checker validates stage/heartbeat ordering, first-frame readiness, no later
startup messages, and the absence of silent gaps over the two-second heartbeat
plus a one-second scheduling allowance on the test host. This is an observability
check, not a startup-speed budget. It fails on the original silent startup log.

## Existing diagnostics and tests

- No in-repository script parses the removed local-footstep/cicada/routine-acoustics messages.
  `cargo test audio::` checks the typed routing, attribution, completion/deadline and wet-path
  outcomes directly; successful log text is not a correctness assertion.
- `--canopy-audio-telemetry`, `--canopy-audio-diagnostic` and
  `--canopy-audio-budget-diagnostic` retain their explicit `[AUDIO][CANOPY]` telemetry.
  `scripts/analyze_canopy_audio_diagnostic.py` still requires its same summary/sample fields
  and assertions. Normal-run quieting does not replace or weaken that diagnostic.
- Ecology's explicit `RE_FLORA_ECOLOGY_SMOKE` fixture still asserts replacement, host removal
  and empty-garden behavior against state (including retained supply/birth counters). Its
  `[ECOLOGY][SMOKE] passed` marker is unchanged. No repository script depends on the removed
  supply/birth/timing chatter. See [ecology evidence](ecology_spawning.md); this fixture saves
  its own garden under `target` and is not a no-save normal-run smoke.
- `[DDGI][HISTORY]` is historical experiment output, not a current log contract. Its shader
  counters/readback data are unchanged. `scripts/check_ddgi_cave_edits.py` still checks its
  explicit capture/publication/history-toggle evidence; see
  [the historical measurements](ddgi_history_candidate.md).
