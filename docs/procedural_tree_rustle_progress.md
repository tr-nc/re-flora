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
- `tools/analyze_tree_rustle.py` - dependency-free WAV metrics for rustle band/envelope checks
- `docs/audio/wind_ref.wav` - current high-quality reference capture
- `target/audio-captures/analysis/wind_ref_metrics.json` - generated reference metrics
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

## Synthesis Targets

Use the reference as a qualitative target, with ratios more important than absolute loudness:

- Base bed: pinkish low/mid wind body, centered near 1 kHz, with 95% rolloff around 3-4 kHz while leaves are quiet.
- Leaf activation: a slow derived envelope with attack/release inertia; gusts should brighten leaf bands over seconds, not jump the whole mix louder.
- Low/mid stability: the low/mid bed changes only modestly during gusts; most perceived motion comes from high-frequency leaf bands.
- 3-6 kHz contact: rises roughly 2-4 dB during leaf-rich motion and remains the dominant high-frequency leaf band.
- 8-12 kHz air: present and rising during leaf-rich motion, but below 3-6 kHz so it reads as air instead of hiss.
- Crackle: controls discrete contact grain rate, duration, brightness, clustering, and peakiness; continuous leaf sheen remains available at low crackle.

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
- Status: done for this tuning pass.

### Phase 3 - Convert observations into synthesis targets

- Objective: Translate reference traits into model parameters and layer behavior.
- Expected output: target curves for base wind bed, leaf activity envelope, 3-6 kHz sheen, and 8-12 kHz air.
- Dependencies/blockers: Need confirmation that the current reference is the desired target.
- Status: done for this tuning pass.

### Phase 4 - Implement model improvements

- Objective: Update the procedural synth without reintroducing plastic crinkle.
- Expected output: synchronized changes in `prototype_tree_rustle.py` and `prototype_tree_rustle_live.py`, with preset/config updates as needed.
- Dependencies/blockers: Phase 3 targets should be clear first.
- Status: done for this tuning pass.

### Phase 5 - Validate by metrics and listening

- Objective: Prove the synth moved toward the reference and still feels controllable.
- Expected output: generated comparison WAVs, live tuner validation, and a short metrics comparison against reference sections.
- Dependencies/blockers: Phase 4 implementation.
- Status: objective checks done for this tuning pass; final subjective approval still depends on listening in the opened live tuner.

## Verification Method

Reference-capture checks:

```bash
ffprobe -v error \
  -show_entries stream=codec_name,channels,sample_rate,bits_per_sample,duration:format=duration,size \
  -of default=nw=1 \
  docs/audio/wind_ref.wav

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
  record_system_audio.py \
  analyze_tree_rustle.py

uvx ruff check \
  prototype_tree_rustle.py \
  prototype_tree_rustle_live.py \
  prototype_tree_rustle_gui.py \
  record_system_audio.py \
  analyze_tree_rustle.py
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

uv run python analyze_tree_rustle.py ../docs/audio/wind_ref.wav \
  --section clean_wind 2.4 8.8 \
  --section leaf_rise 8.8 16.6 \
  --write-json ../target/audio-captures/analysis/wind_ref_metrics.json

uv run python analyze_tree_rustle.py ../target/audio-prototypes/tree_rustle_reference_compare_dense.wav \
  --section early 2.4 8.8 \
  --section late 8.8 16.6 \
  --write-json ../target/audio-prototypes/tree_rustle_reference_compare_dense_metrics.json

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

## Latest Validation Snapshot

Reference metrics from `tools/analyze_tree_rustle.py`:

- clean wind `2.4-8.8s`: centroid `1165 Hz`, rolloff95 `3504 Hz`, `3-6 kHz` minus low/mid `-17.12 dB`, `8-12 kHz` minus `3-6 kHz` `-13.79 dB`.
- leaf rise `8.8-16.6s`: centroid `1544 Hz`, rolloff95 `5484 Hz`, `3-6 kHz` minus low/mid `-15.17 dB`, `8-12 kHz` minus `3-6 kHz` `-12.18 dB`.

Current dense render, `seed=42`, `duration=16.7`:

- early `2.4-8.8s`: centroid `988 Hz`, rolloff95 `3199 Hz`, `3-6 kHz` minus low/mid `-17.80 dB`, `8-12 kHz` minus `3-6 kHz` `-12.29 dB`.
- late `8.8-16.6s`: centroid `1438 Hz`, rolloff95 `5918 Hz`, `3-6 kHz` minus low/mid `-15.72 dB`, `8-12 kHz` minus `3-6 kHz` `-10.87 dB`.
- overall RMS rises about `0.7 dB`; low/mid rises modestly while `3-6 kHz` rises about `2.6 dB` and `8-12 kHz` rises about `4.0 dB`.
- crackle check: `--crackle 0` has lower centroid/rolloff and smooth envelope; `--crackle 1` produces higher peakiness and brighter contact bands, so the control remains meaningful.

## Progress Log

- Softened the original procedural rustle timbre by lowering harsh crackle/dryness/brightness defaults and increasing leaf body.
- Reworked grains to be longer, slower, and more band-limited so they act like leaf-friction puffs instead of tiny plastic ticks.
- Added a band-limited leaf-contact sheen layer around the natural 3-6 kHz region, modulated by leaf flutter.
- Raised/remodeled high-frequency treatment to restore natural air while avoiding brittle synthetic crinkle.
- Synchronized the Python CLI renderer, live Web Audio version, and live config presets.
- Fixed crackle so the slider controls discrete leaf-contact event rate, brightness, duration, and clustering.
- Added `tools/record_system_audio.py` to capture system playback through the default monitor source, not microphone input.
- Captured `target/audio-captures/wind_ref.wav` as a preferred reference, then moved it to `docs/audio/wind_ref.wav` so it is kept with the project documentation.
- Verified the capture format and identified silence, clean wind, and leaf-rise regions.
- Generated a spectrogram and trimmed analysis clips for the reference.
- Decided that the reference's key behavior is not a large loudness rise; it is a stable low/mid wind bed with gradually increasing high-frequency leaf activity.
- Added `tools/analyze_tree_rustle.py` so future tuning can reproduce reference and synth metrics without external Python audio dependencies.
- Added a derived slow leaf-activity envelope so gusts increase contact brightness over time instead of multiplying the whole mix.
- Cascaded the air/body/leaf/contact filters to make the base bed pinker and reduce raw white-noise tails.
- Added a separate restrained 8-12 kHz air layer below the 3-6 kHz contact band.
- Re-tuned the dense render against `docs/audio/wind_ref.wav`; current metrics now closely match the reference's centroid/rolloff and band-ratio movement.
- Mirrored the model changes into the live Web Audio worklet and verified the local web host serves the updated page/worklet/config.

## Open Questions / Risks

- One reference clip may be too narrow; collect more good examples if possible.
- The latest dense render is close by metrics, but final acceptance still depends on user listening feedback.
- Need confirm whether the desired in-game sound should match this recording exactly or only use it as qualitative guidance.
