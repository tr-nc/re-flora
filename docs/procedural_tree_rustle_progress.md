# Procedural Tree Rustle Progress

## Goal

Improve the procedural tree wind/rustle prototype so it sounds like natural wind moving through leaves rather than synthetic white-noise crackle or plastic-bag crinkle.

Done means:

- the CLI renderer and live Web Audio tuner stay behaviorally synchronized;
- the crackle control has an obvious, useful effect on discrete leaf-contact sounds;
- natural high-frequency air is present, including the 3-6 kHz leaf-contact band and restrained 8-12 kHz content;
- reference recordings can be captured from system playback, analyzed, and used to tune the model;
- changes are validated by both listening and measurable spectral/envelope checks.

## Current State

Branch/worktree:

- branch: `agent/procedural-rustle-prototype`
- current work is ahead of origin and not yet integrated into main

Relevant files:

- `tools/prototype_tree_rustle.py` - offline Python WAV renderer
- `tools/prototype_tree_rustle_live.py` - live browser/Web Audio tuner
- `tools/tree_rustle_live_config.json` - live tuner defaults and presets
- `tools/record_system_audio.py` - records computer playback from monitor source, not microphone
- `target/audio-captures/wind_ref.wav` - current high-quality reference capture
- `target/audio-captures/analysis/wind_ref_spectrogram.png` - generated spectrogram for the reference
- `target/audio-captures/analysis/wind_ref_clean_wind_2p4_8p8.wav` - trimmed clean wind section
- `target/audio-captures/analysis/wind_ref_leaf_rise_8p8_16p6.wav` - trimmed leaf-rise/gust section

Known reference-capture facts:

- `wind_ref.wav` is valid 48 kHz stereo 16-bit PCM, 16.70s long.
- The first ~2.1s are silence/lead-in.
- The clean wind bed is roughly `2.4-8.8s`.
- The leaf/gust rise is roughly `8.8-16.6s`.
- Clean wind bed:
  - RMS about `-37.25 dBFS`
  - spectral centroid about `1.17 kHz`
  - 95% rolloff about `3.5 kHz`
  - `3-6 kHz` is about `12 dB` below the low/mid wind bed
  - `8-12 kHz` is about `10 dB` below `3-6 kHz`
- Leaf-rich section:
  - overall RMS rises only about `0.4 dB`
  - spectral centroid rises to about `1.6 kHz`
  - 95% rolloff rises to about `5.8 kHz`
  - `3-6 kHz` rises about `3.3 dB`
  - `8-12 kHz` rises about `5.7 dB`, but remains below `3-6 kHz`

Important assumptions to confirm:

- The captured reference is representative of the desired in-game tree/wind aesthetic.
- The first section should guide the base wind bed; the later section should guide leaf activation during gusts.
- Absolute loudness is less important than spectral ratios, ramp timing, and perceived naturalness.

## Plan / Phases

### Phase 1 - Capture reliable references

- Objective: Record high-quality system playback references without microphone contamination.
- Expected output: WAV captures under `target/audio-captures/` plus trimmed useful sections.
- Dependencies/blockers: Linux/PipeWire/PulseAudio monitor source must be available; user must provide desired reference audio.
- Status: done.

### Phase 2 - Analyze reference features

- Objective: Extract practical targets for spectrum, envelope, onset/ramp behavior, and stereo width.
- Expected output: concise metrics and analysis artifacts, especially clean-wind vs leaf-rise comparisons.
- Dependencies/blockers: Need enough reference clips to avoid overfitting to one recording.
- Status: in progress.

### Phase 3 - Convert observations into synthesis targets

- Objective: Translate reference traits into model parameters and layer behavior.
- Expected output: target curves for base wind bed, leaf activity envelope, 3-6 kHz sheen, and 8-12 kHz air.
- Dependencies/blockers: Need confirmation that the current reference is the desired target.
- Status: not started.

### Phase 4 - Implement model improvements

- Objective: Update the procedural synth without reintroducing plastic crinkle.
- Expected output: synchronized changes in `prototype_tree_rustle.py` and `prototype_tree_rustle_live.py`, with preset/config updates as needed.
- Dependencies/blockers: Phase 3 targets should be clear first.
- Status: not started.

### Phase 5 - Validate by metrics and listening

- Objective: Prove the synth moved toward the reference and still feels controllable.
- Expected output: generated comparison WAVs, live tuner validation, and a short metrics comparison against reference sections.
- Dependencies/blockers: Phase 4 implementation.
- Status: not started.

## Verification Method

Reference-capture checks:

```bash
ffprobe -v error \
  -show_entries stream=codec_name,channels,sample_rate,bits_per_sample,duration:format=duration,size \
  -of default=nw=1 \
  target/audio-captures/wind_ref.wav

cd tools
uv run python record_system_audio.py --list-devices
```

Prototype health checks:

```bash
cd tools
uv run python -m py_compile \
  prototype_tree_rustle.py \
  prototype_tree_rustle_live.py \
  prototype_tree_rustle_gui.py \
  record_system_audio.py

uvx ruff check \
  tools/prototype_tree_rustle.py \
  tools/prototype_tree_rustle_live.py \
  tools/prototype_tree_rustle_gui.py \
  tools/record_system_audio.py
```

Rendering/listening checks:

```bash
cd tools
uv run python prototype_tree_rustle.py \
  --duration 16.7 \
  --preset dense \
  --seed 42 \
  --no-play \
  --out ../target/audio-prototypes/tree_rustle_reference_compare_dense.wav

uv run python prototype_tree_rustle_live.py --host 127.0.0.1 --port 8080 --no-open
```

Acceptance criteria for the next tuning pass:

- clean wind bed is soft/pinkish, not white-noise bright;
- leaf activity ramps over seconds rather than appearing as an instant volume jump;
- overall loudness stays relatively stable while the leaf bands brighten;
- `3-6 kHz` strengthens during leaf activity;
- `8-12 kHz` is present but remains below the `3-6 kHz` band;
- crackle slider remains audibly meaningful across its range;
- user listening feedback agrees that the result is closer to the reference.

## Progress Log

- Softened the original procedural rustle timbre by lowering harsh crackle/dryness/brightness defaults and increasing leaf body.
- Reworked grains to be longer, slower, and more band-limited so they act like leaf-friction puffs instead of tiny plastic ticks.
- Added a band-limited leaf-contact sheen layer around the natural 3-6 kHz region, modulated by leaf flutter.
- Raised/remodeled high-frequency treatment to restore natural air while avoiding brittle synthetic crinkle.
- Synchronized the Python CLI renderer, live Web Audio version, and live config presets.
- Fixed crackle so the slider controls discrete leaf-contact event rate, brightness, duration, and clustering.
- Added `tools/record_system_audio.py` to capture system playback through the default monitor source, not microphone input.
- Captured `target/audio-captures/wind_ref.wav` as a preferred reference.
- Verified the capture format and identified silence, clean wind, and leaf-rise regions.
- Generated a spectrogram and trimmed analysis clips for the reference.
- Decided that the reference's key behavior is not a large loudness rise; it is a stable low/mid wind bed with gradually increasing high-frequency leaf activity.

## Open Questions / Risks

- One reference clip may be too narrow; collect more good examples if possible.
- The current synth may still be too bright above 8 kHz compared with the reference if high-frequency shaping is removed entirely.
- Need decide whether to add a saved analysis script instead of ad-hoc metric commands.
- Need confirm whether the desired in-game sound should match this recording exactly or only use it as qualitative guidance.
- Need confirm if the live tuner should expose a separate leaf-activity/ramp control or derive it automatically from wind/gust state.
