# Runtime logging contracts

Normal runs do not report each DDGI history batch, local footstep play/render/completion,
wet-path success, cicada call/retirement, or routine acoustic response. These call sites
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

## Existing diagnostics and tests

- No in-repository script parses the removed local-footstep/cicada/routine-acoustics messages.
  `cargo test audio::` checks the typed routing, attribution, completion/deadline and wet-path
  outcomes directly; successful log text is not a correctness assertion.
- `--canopy-audio-telemetry`, `--canopy-audio-diagnostic` and
  `--canopy-audio-budget-diagnostic` retain their explicit `[AUDIO][CANOPY]` telemetry.
  `scripts/analyze_canopy_audio_diagnostic.py` still requires its same summary/sample fields
  and assertions. Normal-run quieting does not replace or weaken that diagnostic.
- `[DDGI][HISTORY]` is historical experiment output, not a current log contract. Its shader
  counters/readback data are unchanged. `scripts/check_ddgi_cave_edits.py` still checks its
  explicit capture/publication/history-toggle evidence; see
  [the historical measurements](ddgi_history_candidate.md).
