# Bee wing buzz — first-two-seconds loop

User-supplied download, imported on 2026-10-29 for local sound review. The user
identified it as the bee sound and requested cropping the first two seconds and
making it loopable. This bundle is not connected to in-game bee playback.

## Files

All WAV files are stereo, 48,000 Hz, signed 16-bit PCM.

| File | Role | Frames | Duration |
| --- | --- | ---: | ---: |
| [bee-original.wav](bee-original.wav) | Original download, byte-for-byte preserved | 384,000 | 8.000 s |
| [bee-first-2s.wav](bee-first-2s.wav) | Exact source crop, editing/reference intermediate | 96,000 | 2.000 s |
| [bee-loop.wav](bee-loop.wav) | Selected loop derivative with a circular 50 ms crossfade | 93,600 | 1.950 s |

Original filename: `Seamless_loop_of_a_s_#2-1793254420925.wav`.
The download filename is not a verified service asset ID. The source download was
left in place; it is not needed to reproduce this bundle.

## Generation provenance

The conversation proposed the project's existing **ElevenLabs / SFX v3** workflow
for this bee sound, but no generation screenshot or service metadata was supplied
with this download. Provider/model are therefore contextual, not independently
confirmed for this take. Creator/account identity, source URL, generation ID, seed,
reference inputs, negative-prompt field and generation-time license are unknown.

Recommended settings were Loop on, duration 8 s, Prompt Influence 30%, and prompt
improvement on. Actual submitted settings were not confirmed; the file's measured
8 s duration is not proof of the other settings.

The exact submitted prompt and any effective service-rewritten prompt are
**unavailable**. The following is the exact assistant-proposed prompt immediately
before the download, recorded as a candidate rather than asserted submission:

```text
Seamless loop of a single honeybee hovering steadily in place. Close-up isolated wing buzz, soft warm mid-low hum with fine fluttering harmonics and subtle natural pitch variation. Calm, delicate and realistic, with consistent overall intensity. No flybys, Doppler shifts, swarm, mosquito whine, aggressive wasp buzz, background ambience, voices, music or reverb. No takeoff, landing or fades.
```

## Reproduce the edits

Tool: FFmpeg 9.0.2. Run from this directory with the supplied original. The commands
create the derivatives; FFmpeg asks before overwriting existing outputs.

```sh
ffmpeg -hide_banner -loglevel error -i bee-original.wav \
  -af 'atrim=start_sample=0:end_sample=96000,asetpts=PTS-STARTPTS' \
  -c:a pcm_s16le -map_metadata -1 bee-first-2s.wav

ffmpeg -hide_banner -loglevel error -i bee-first-2s.wav \
  -filter_complex '[0:a]asplit=3[h][m][t];[h]atrim=start_sample=0:end_sample=2400,asetpts=PTS-STARTPTS[head];[m]atrim=start_sample=2400:end_sample=93600,asetpts=PTS-STARTPTS[body];[t]atrim=start_sample=93600:end_sample=96000,asetpts=PTS-STARTPTS[tail];[tail][head]acrossfade=ns=2400:c1=hsin:c2=hsin[seam];[seam][body]concat=n=2:v=0:a=1[out]' \
  -map '[out]' -c:a pcm_s16le -map_metadata -1 bee-loop.wav
```

Processing, with half-open frame ranges:

1. Retain original frames **[0, 96,000)**, exactly 0–2 s; crop PCM is unchanged.
2. Blend the final 2,400 frames **[93,600, 96,000)** into the first 2,400 frames
   **[0, 2,400)** using FFmpeg's complementary half-sine (`hsin`) fade curves,
   independently for each channel. This avoids the correlated-signal gain boost
   possible with an equal-power blend.
3. Place that 2,400-frame blend first, then append unchanged crop frames
   **[2,400, 93,600)**. At playback wraparound the body returns to its adjacent
   source tail sample at the start of the blend, rather than jumping to source 0.
4. Overlap shortens the period to **1.95 s**. No source after 2 s is used. No silence
   padding, resampling, pitch shifting, time stretching, channel conversion,
   normalization, gain adjustment, DC removal or other filtering was performed.
   Exported WAV headers are regenerated; source metadata is not copied.

## Validation and listening status

- Original matches the download by SHA-256. Crop PCM exactly matches the original's
  first 96,000 frames. Loop body outside the crossfade is byte-for-byte unchanged.
- All three files decode with the frame counts and format listed above.
- Loop channels have peaks 1,433 / 823 (PCM16 units), RMS −41.40 / −45.23 dBFS,
  and no clipping. Original quiet level and channel balance are retained.
- Wraparound sample steps are 57 / 68; crossfade-to-body steps are 15 / 67.
  Both are below ordinary adjacent-sample step 99th percentiles (168 / 105).
  These checks reject obvious boundary discontinuities, not perceptual modulation.
- No human listening approval is claimed. Audition repeated playback for a possible
  recurring crossfade dip, tonal change, and general suitability before integration.

SHA-256:

```text
9566786183c9e7dccd9d29aa4c2e3e7053a3edc61b961876427fb1550669554c  bee-original.wav
cd5a80d7bf0675f0fff3d14bbf8860ee39b923a3ee4a766afa4241d574bede6d  bee-first-2s.wav
631f979fe4523e4cd72bbdb44720c2292411207919736ac17fe157d941b54843  bee-loop.wav
```

## Rights

The user authorized local import/editing. Subscription tier, commercial-use
permission, attribution requirements and standalone-file redistribution rights
for this specific generation are **unverified**. Do not infer them from older
project assets or from the provider name. This bundle does not establish public
repository or commercial release clearance. Derivatives inherit the source's
restrictions; resolve rights before publication or distribution.
