# Apple landing on dirt — one-shot variations

Three user-selected ElevenLabs takes, each containing **one apple impact**, despite the submitted prompt requesting five. Each original and its onset-trimmed derivative live together in this folder. These are prepared assets, not yet connected to in-game playback.

## Generation provenance

Settings supplied by the user:

| Setting | Value |
| --- | --- |
| Source/provider | ElevenLabs |
| Model | SFX v3 |
| Duration | 2 seconds |
| Loop | **off** |
| Prompt polish | **off** |
| Prompt Influence | **30%** |

Generation URLs/IDs and seeds were not supplied. Original download filenames are recorded below and in [provenance.json](provenance.json); they are not asserted to be generation IDs. No generation settings were inferred from the earlier sprinkler assets.

### Submitted prompt (verbatim)

```text
Five separate apples landing one at a time on soft garden dirt, evenly spaced with clear silence between impacts. Each has a rounded thud, tiny soil crunch and brief settling, with subtle natural variations. Same apple size, surface and close-up perspective. No bouncing, rolling,
 ambience or reverb.
```

The user confirmed the actual outputs each contain one sound rather than five. Do not split their secondary settling sounds into additional apple variants.

## Files and trim boundaries

All files are **48 kHz stereo, signed 16-bit PCM WAV**. Originals are 96,000 frames / 2.000 seconds each and preserve the downloaded bytes, with clearer filenames.

| Selected one-shot | Original | Download filename | Leading time removed | Output duration |
| --- | --- | --- | --- | --- |
| [apple-dirt-01.wav](apple-dirt-01.wav) | [apple-dirt-01-original.wav](apple-dirt-01-original.wav) | `Five_separate_apples_#1-1792741332552.wav` | 427 ms | 1.573 s |
| [apple-dirt-02.wav](apple-dirt-02.wav) | [apple-dirt-02-original.wav](apple-dirt-02-original.wav) | `Five_separate_apples_#2-1792741332552.wav` | 275 ms | 1.725 s |
| [apple-dirt-03.wav](apple-dirt-03.wav) | [apple-dirt-03-original.wav](apple-dirt-03-original.wav) | `Five_separate_apples_#1-1792741343376.wav` | 196 ms | 1.804 s |

Only **leading delay** was removed. Every retained PCM sample is identical to the corresponding original sample, including the attack, quieter secondary sounds, and the entire original tail. No fade-in, normalization, gain adjustment, noise removal, filtering, channel conversion, resampling, pitch/time adjustment, or tail trimming was applied.

### Onset selection and reproduction

Analysis used Python `wave` and NumPy:

1. Calculate RMS across both channels in 1 ms / 48-frame windows, relative to full-scale 32768.
2. Locate the first three consecutive windows above −50 dBFS RMS. This avoids treating isolated, very quiet pre-impact noise as the intended attack.
3. Backtrack at most 80 ms to the nearest three-window run below −60 dBFS; take the following window as the detected onset.
4. Retain **2 ms before that onset** to protect the beginning of the sound. Keep all subsequent samples through original frame 96,000 (exclusive).

Exact retained frame intervals:

```text
apple-dirt-01.wav = original frames [20496, 96000)
apple-dirt-02.wav = original frames [13200, 96000)
apple-dirt-03.wav = original frames [ 9408, 96000)
```

For example, from this folder, the following reproduces the retained PCM of take 01 (container metadata may differ):

```sh
ffmpeg -i apple-dirt-01-original.wav -af 'atrim=start_sample=20496,asetpts=PTS-STARTPTS' -c:a pcm_s16le apple-dirt-01-reproduced.wav
```

Use the other frame offsets for takes 02/03. [provenance.json](provenance.json) includes every source/derivative SHA-256, frame count, trim boundary, duration and measured peak level.

## Validation and intended use

- Detected sustained attacks begin approximately **5 ms, 7 ms, and 4 ms** into the trimmed files, respectively. The previous 196–427 ms leading delays are gone. These are threshold-based audio measurements, not claims about end-to-end game latency.
- Retained samples were compared directly to the originals and match exactly. Formats, frame counts, hashes, decode validity and absence of clipping were checked.
- Select among the three **non-looping** derivatives for variation when integrating apple-on-dirt collisions. The `-original.wav` files are editing/reference sources, not intended runtime variants.
- Do not silently normalize these files under the generic SFX guide. Preserve the approved input while evaluating the runtime mix; document any later processing as a new derivative.
- Actual in-game latency, spatialization, level balancing and collision-trigger behavior have not been tested. A human listening pass remains necessary for final sound quality acceptance.

## Usage rights

**Commercial release blocked — Free-plan generation (confirmed by the user; checked 2026-10-23).** ElevenLabs' [official publication policy](https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform) says Free-plan outputs cannot be used commercially. Attribution does not grant commercial permission, and later upgrading does not automatically license previously generated files. This applies to the originals and any derivatives in this folder.

The user authorized importing these files, but that is not a commercial or redistribution license. Before commercial use, replace them with qualifying paid-plan generations or obtain explicit written authorization from ElevenLabs for these outputs. No such authorization has been supplied. Billing details, generation-time terms/notice history and any separate agreement have not been verified.

The [ElevenCreative Terms, Exhibit 4 section 2](https://elevenlabs.io/creative-terms) also restrict standalone/isolated SFX distribution. Do not assume even paid-plan outputs may be published as raw WAVs in a public repository or reusable asset library. Nothing was pushed during this import/review work. This note records the published policy, not a separate license grant.
