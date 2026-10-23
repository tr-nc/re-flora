# Sprinkler sound assets

This folder is a self-contained bundle: the original ElevenLabs download, the selected runtime loop, and their provenance/editing record. Generation settings below were supplied by the user; no regeneration was performed to verify them.

## Files

| File | Role | Duration | Frames |
| --- | --- | --- | --- |
| [sprinkler-original.wav](sprinkler-original.wav) | Unmodified original download, renamed for clarity; retain as the editing source | 8.000 s | 384,000 |
| [sprinkler-first-6-loop.wav](sprinkler-first-6-loop.wav) | Selected loop containing the first six water bursts | 1.560 s | 74,880 |

Both files are stereo, 48,000 Hz, signed 16-bit PCM WAV. The selected loop is now used in game; the original remains editing/reference material and is not preloaded.

## Generation provenance

- **Source/provider:** ElevenLabs.
- **Model:** SFX v3.
- **Loop:** on.
- **Requested duration:** 8 seconds.
- **Prompt Influence:** 30%.
- **Prompt improve:** on.
- **Original download filename:** `Seamless_loop_of_a_s_#1-1792739546356.wav`.
- **Generation URL/asset ID and seed:** not supplied. The download filename is not asserted to be a generation ID.
- **Effective improved prompt:** not supplied; the exact user-submitted prompt is recorded below, not a reconstruction of the service's rewrite.

### Submitted prompt (verbatim)

```text
Seamless loop of a small garden sprinkler. Soft steady water spray with delicate droplets pattering on leaves. Gentle, soothing, consistent volume. No harsh hiss, mechanical clicks, background ambience or music. No startup, shutdown or fades.
```

Despite requesting steady water, this particular generated take has a pronounced repeating pulse. The user preferred that natural rhythmic result. Other prompts proposed during exploration were not reported as the prompt used for this file.

## Selected loop construction

The user reported unwanted clicks later in the take and selected the **first-six-bursts loop**, rather than the earlier longer crop or de-clicked/locally repaired experiments. Those rejected variants and preview files are not part of this bundle.

The selected loop was made **directly from the unmodified original**, using Python `wave` and NumPy:

1. Read the original stereo PCM without resampling or changing gain.
2. Take frames **[0, 75,600)**, corresponding to **0–1.575 seconds**. This includes the first six bursts and 15 ms of trailing water before the next burst.
3. Make a **720-frame / 15 ms circular equal-power overlap** between frames **[74,880, 75,600)** (tail) and **[0, 720)** (head). For each channel and `i = 0..719`:

   ```text
   theta = (pi / 2) * i / 719
   output[i] = round(original[74880 + i] * cos(theta)
                   + original[i] * sin(theta))
   ```

4. Append original frames **[720, 74,880)** unchanged. Write the resulting **74,880 frames / 1.560 seconds** as stereo 48 kHz PCM16 WAV. Rounding used NumPy `rint`; no sample clipped.

Only the first 15 ms of the output differs from the corresponding original prefix; this is the circular seam, not a fade to silence. No de-clicking, local patching, normalization, gain adjustment, filtering, time stretching, or pitch shifting was applied. The original file remains byte-for-byte identical to the download. Keep these source/selected assets unnormalized; any later runtime trim or derivative must be documented separately.

## Measured rhythm

Analysis of this selected file used 5 ms stereo mean-square energy windows with a three-window moving average, followed by local-maximum selection with at least 180 ms separation. These are approximate **energy peak** times, not sample-accurate valve triggers:

```text
0.075, 0.335, 0.595, 0.860, 1.115, 1.380 seconds
```

- Six bursts in exactly **1.56 s**: **3.846153846 bursts/second**.
- Mean burst interval including the loop seam: **0.260 s**.
- Measured peak intervals, including wrap: **0.260, 0.260, 0.265, 0.255, 0.265, 0.255 s**.
- If opposite nozzle pairs alternate once per burst: one pair switch every **0.260 s**; a complete Z→X→Z cycle every **0.520 s** (**1.923076923 full cycles/second**).
- Loop repetition frequency is **0.641025641 Hz**; do not confuse this with the six-times-faster burst cadence or with spectral pitch.
- Selected loop peak level: approximately **−22.99 dBFS**. No runtime loudness setting has been selected yet.

The recording's duty cycle/active spray duration has **not** been measured. Runtime integration retains the existing 30% valve opening (78 ms per 260 ms) as a fixed motion shape, not as a measured audio property. Do not infer it from the 15 ms edit crossfade. The first measured peak is about 75 ms after the file begins, so playback phase and motion phase must account for this if aligning the visible impact with the sound.

## Integrity

SHA-256:

```text
8d29f222b7cd7d20b8f36659f39c59c31febd28de2b093048467b448a24e9197  sprinkler-original.wav
05984b1b8009b8a0355e91d05d29968f9bb2c9829606f4f8aaa83534a708d146  sprinkler-first-6-loop.wav
```

Validated: durations/frame counts, WAV decoding, six energy peaks, no clipping, and unchanged selected samples outside the circular seam. User selected this version after previewing alternatives; in-game loudness, spatialization, and audiovisual alignment still need a listening pass.

## Runtime integration

Restored unchanged from Git revision `1067833b` at the user's request for local
development. The hashes above still apply; no normalization, editing, time
stretching or pitch change was performed. Only the selected WAV appears in
`src/audio/runtime_assets.rs`; the original is never preloaded.

`src/sprinkler.rs` owns frame count, sample rate, six-burst count, fixed cadence,
30% valve opening, and the approximately 75 ms peak offset. The existing rotary
head remains: the old opposing-pair interpretation above is historical, not the
current hardware. The existing world-derived spray clock carries the full loop
position; native GPU valve phase is derived from it. There is no frequency or
duty UI, and obsolete saved overrides are discarded at load.

Each admitted sprinkler starts the original-speed loop once at its corresponding
world/instance phase plus 75 ms. It is not restarted on each burst or sought every
frame. Moving it updates the existing spatial emitter. Pause, removal, mix-disable,
world-load gating and stale/clock-discontinuous frames retire sources; resume
starts at the current phase. The established 16-slot bound includes retirement.

The existing Terrain bus, positional mono rendering/downmix, attenuation and
master controls remain in use. Source gain remains **−12 dB**; the continuous
stale-safety envelope has steady gain **0.72** (about −2.85 dB), with approximately
1 ms start/stop edges. It does not reshape the recording's bursts. These are
runtime mix gains, not edits to either WAV. Human listening may motivate a later
mix adjustment; no listening approval is claimed.

Audio blocks and device buffering add real latency. The world-clock arithmetic
is tested over ten simulated minutes; this is not a measurement of physical
device-clock drift or sample-accurate audiovisual output. No per-frame seek or
playback-rate correction hides that boundary.

## Usage rights

**Commercial release blocked — Free-plan generation (confirmed by the user; checked 2026-10-23).** ElevenLabs' [official publication policy](https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform) says Free-plan outputs cannot be used commercially. Attribution does not grant commercial permission, and later upgrading does not automatically license previously generated files. This applies to the originals and any derivatives in this folder.

The user authorized importing these files, but that is not a commercial or redistribution license. Before commercial use, replace them with qualifying paid-plan generations or obtain explicit written authorization from ElevenLabs for these outputs. No such authorization has been supplied. Billing details, generation-time terms/notice history and any separate agreement have not been verified.

The [ElevenCreative Terms, Exhibit 4 section 2](https://elevenlabs.io/creative-terms) also restrict standalone/isolated SFX distribution. Do not assume even paid-plan outputs may be published as raw WAVs in a public repository or reusable asset library. Nothing was pushed during this import/review work. This note records the published policy, not a separate license grant.
