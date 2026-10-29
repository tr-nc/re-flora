# Wind through grass — first-two-seconds loop

User-supplied grass sound, imported on 2026-10-29. The user requested an exact
first-two-seconds crop and a loop-ready derivative, plus an integration proposal
before changing game playback. The user subsequently approved implementation.
**Runtime:** only `grass-wind-loop.wav` is declared in `src/audio/runtime_assets.rs`.
`GrassWindAudio` plays bounded world-anchored patches from committed grass coverage
and local wind. Originals and the exact crop are not preloaded.

[Original HTML proposal with audio previews](../../../docs/reports/grass-wind-audio.html) ·
[Implementation, saved controls and validation](../../../docs/reports/grass-wind-audio.md).
The file bytes remain unchanged. Initial playback makeup is an explicit **+12 dB**
Debug control, not normalization of the resource; listening approval is still pending.

## Files

All audio is stereo, 48,000 Hz, signed 16-bit PCM WAV.

| File | Role | Frames | Duration |
| --- | --- | ---: | ---: |
| [grass-wind-original.wav](grass-wind-original.wav) | Unmodified original download | 384,000 | 8.000 s |
| [grass-wind-first-2s.wav](grass-wind-first-2s.wav) | Exact source crop; editing/reference intermediate | 96,000 | 2.000 s |
| [grass-wind-loop.wav](grass-wind-loop.wav) | Runtime loop, circular 50 ms crossfade | 93,600 | 1.950 s |

Original download filename: `Seamless_loop_of_a_s_#3-1793254692653.wav`. Moved from
the user's Downloads folder and renamed without modifying bytes; SHA-256 was
checked before and after the move. The filename is not a verified generation ID.
The original is fully contained here; no external Downloads file is needed.

## Generation data

The user supplied the generation settings screenshot; values were transcribed as
text. The screenshot itself is deliberately not stored, following their preference.

| Setting | Value |
| --- | --- |
| Provider | ElevenLabs (conversation context) |
| Model | SFX v3 |
| Loop | **On** |
| Requested duration | **8.0 seconds** |
| Prompt Influence | **30%** |
| Prompt improvement / polish | **On** |

Source URL, verified asset/generation ID, seed, reference inputs, separate negative
prompt and account identity were not supplied. The service's effective improved
prompt is **unavailable**, not reconstructed from the result.

### Prompt supplied in the conversation

The following is the exact prompt proposed for this grass generation. The supplied
screenshot shows settings only; exact submission of this text was not separately
confirmed. Preserve that distinction if the user later supplies generation history.

```text
Seamless loop of a small patch of fine meadow grass gently rustling in a steady light breeze. Close-up isolated grass texture, soft silky swishing and delicate friction as slender green blades brush against each other. Fine, gentle and natural, with consistent overall intensity and subtle irregular detail. No gusts or swells, wind whoosh, microphone wind noise, tree-leaf flapping, dry-leaf crackling, footsteps, insects, birds, background ambience, music or reverb. No fades.
```

## Reproduce processing

Tool: FFmpeg 9.0.2. Run in this directory; commands ask before overwriting outputs.

```sh
ffmpeg -hide_banner -loglevel error -i grass-wind-original.wav \
  -af 'atrim=start_sample=0:end_sample=96000,asetpts=PTS-STARTPTS' \
  -c:a pcm_s16le -map_metadata -1 grass-wind-first-2s.wav

ffmpeg -hide_banner -loglevel error -i grass-wind-first-2s.wav \
  -filter_complex '[0:a]asplit=3[h][m][t];[h]atrim=start_sample=0:end_sample=2400,asetpts=PTS-STARTPTS[head];[m]atrim=start_sample=2400:end_sample=93600,asetpts=PTS-STARTPTS[body];[t]atrim=start_sample=93600:end_sample=96000,asetpts=PTS-STARTPTS[tail];[tail][head]acrossfade=ns=2400:c1=hsin:c2=hsin[seam];[seam][body]concat=n=2:v=0:a=1[out]' \
  -map '[out]' -c:a pcm_s16le -map_metadata -1 grass-wind-loop.wav
```

1. Retain source frames **[0, 96,000)**, exactly 0–2 seconds.
2. Overlap tail **[93,600, 96,000)** with head **[0, 2,400)** using complementary
   half-sine (`hsin`) fade-out/fade-in, independently for each channel.
3. Place the 2,400-frame blend first, followed by unchanged crop frames
   **[2,400, 93,600)**. Playback wraps from the body's last sample into its adjacent
   source tail at the start of the blend. Period: **93,600 frames / 1.95 s**.
4. No material after source 2 s, silence padding, normalization, gain adjustment,
   resampling, channel conversion, DC removal, pitch shift or time stretching.
   WAV headers are regenerated; original metadata is not copied to derivatives.

## Validation

- Original file hash matches the download before moving.
- Crop PCM exactly equals original frames [0, 96,000); loop PCM outside the blend
  exactly equals crop frames [2,400, 93,600).
- All files decode with the formats/durations above; no clipping.
- Loop L/R peaks: **363 / 442** PCM16 units; RMS: **−53.24 / −53.58 dBFS**.
  This take is very quiet. No automatic loudness boost was applied to the files.
- Wrap sample steps: **40 / 12**; crossfade-to-body steps: **142 / 37**.
  Both are below ordinary adjacent-step 99th percentiles **177 / 173**.
- Numerical seam checks are not listening approval. Listen for repeated texture,
  a 1.95 s rhythm and crossfade dips before accepting it in the game. One loop with
  different phase offsets still shares the same 1.95 s repetition period.

SHA-256:

```text
232cb0fb9a846da94f8b4f479cfb2c0cf34c9b358fde4d76a84da5646636be47  grass-wind-original.wav
d184a8043bf2a0129db73aece275577e0a4b11f510a31a380b59e370ecf26c61  grass-wind-first-2s.wav
04a7554eda25b53120bde24050f4f13a887ef40c1dbcd5251f503831e2f2d4a1  grass-wind-loop.wav
```

## Rights

User authorized local import, processing and investigation. Generation-time
subscription tier, commercial clearance, attribution obligations and permission
for standalone/public-repository redistribution are **unverified**. Provider name
or import permission is not a license. All derivatives inherit the original's
restrictions; do not publish/distribute this bundle without resolving those rights.
