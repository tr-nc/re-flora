# Gentle breeze through leaves — four loop candidates

Four original ElevenLabs downloads supplied by the user. All are imported **byte-for-byte unchanged**, renamed for clarity. This folder contains the complete source/provenance record and all four takes; no external Downloads path or chat history is needed to identify them.

## Generation provenance

Settings transcribed from the user's generation-settings screenshot, which the user confirmed applies to all four files:

| Setting | Value |
| --- | --- |
| Source/provider | ElevenLabs |
| Model | SFX v3 |
| Loop | **on** |
| Duration | **8.0 seconds** |
| Prompt Influence | **30%** |
| Prompt improvement / polish | **on** |

### Submitted prompt (verbatim, with UI line wrapping removed)

```text
Seamless loop of a small cluster of leaves gently rustling in a steady light breeze. Close-up isolated point source, soft leafy friction and delicate flutter. Consistent intensity, no gusts or swells. No wind whoosh, forest ambience, birds, branches or reverb.
```

The screenshot shows the submitted prompt and improvement enabled; the effective rewritten prompt was not supplied. Seeds, generation URLs and verified generation IDs were not supplied. The download filenames below are identifiers for the source files, not asserted service generation IDs.

## Files

Each file is **8.000 seconds / 384,000 frames**, stereo **48,000 Hz signed 16-bit PCM WAV**.

| Imported original | Original download filename |
| --- | --- |
| [leaves-breeze-01.wav](leaves-breeze-01.wav) | `Seamless_loop_of_a_s_#1-1792741669938.wav` |
| [leaves-breeze-02.wav](leaves-breeze-02.wav) | `Seamless_loop_of_a_s_#2-1792741674217.wav` |
| [leaves-breeze-03.wav](leaves-breeze-03.wav) | `Seamless_loop_of_a_s_#3-1792741686667.wav` |
| [leaves-breeze-04.wav](leaves-breeze-04.wav) | `Seamless_loop_of_a_s_#4-1792741686668.wav` |

**Processing: none.** No trimming, crossfading, normalization, gain change, filtering, channel conversion, resampling, pitch shifting or time stretching. Each imported file is the original; no redundant derivative copy is needed. Downloads copies were removed only after byte-for-byte verification of the project copies.

SHA-256:

```text
3594315e37839f77db3bfb201925063ce5a81f7975893fa22f1d0487b36cd7dd  leaves-breeze-01.wav
cc5772164930744415b20b3b4cbcd9a2dc215e9cdf13651c61e46ed3a1f5d67d  leaves-breeze-02.wav
74deeab38af52d3be2346fa9ffa8225ae7b7a85f919161613fb85e8e8a21507c  leaves-breeze-03.wav
2fd6df5a7cc2727dc6bde32047bdc3b88237a9418a5e75890224a7384ebab1c7  leaves-breeze-04.wav
```

## Intended use and review status

- Intended sound: a small localized cluster of leaves under steady light airflow, **not** a forest-wide ambient wind bed.
- The planned game use is positional audio with gentle/strong leaf textures mixed according to live wind strength. These four files supply the **gentle-breeze group** of the saved recorded-leaf A/B experiment. The unchecked mode retains procedural foliage audio.
- Preserve the original stereo downloads. Any later mono/positional preparation, level matching, filtering or seam repair must be recorded as a derivative in this bundle.
- Verified: all four decode successfully, match their downloads exactly, and have the frame count/sample format above.
- Not verified: audible seam quality, steadiness, lack of ambience, suitability of each take, or in-game spatial/loudness balance. Loop mode and prompt wording describe the generation request, not guaranteed audio quality. A listening pass is still required.

## Runtime derivatives (2026-10-24)

The originals above are still unchanged. `runtime/` contains four separate, reproducible
mono PCM16 / 48 kHz derivatives used by the runtime manifest, plus source/output SHA-256
records in [runtime/manifest.json](runtime/manifest.json). Only these derivatives are preloaded.

Preparation: stereo channel mean, DC removal, 100 ms equal-power tail/head overlap,
then RMS matching to −30 dBFS. The overlap shortens each loop to **379,200 frames / 7.9 s**;
playback remains at original speed, without pitch shifting. Rebuild or verify with:

```sh
python3 scripts/prepare_tree_leaf_loops.py
python3 scripts/prepare_tree_leaf_loops.py --check
```

Each active canopy generation chooses one take here and one strong-wind take independently,
with independent random phases held stable until that source is replaced. Live local wind
crossfades the two inside one spatial Voice. Re-entering the A/B mode preserves the selection
and advances its offset rather than rerolling. Existing Tree leaves mixer and Tree Wind Volume
control both modes. See [implementation/validation](../../../docs/reports/recorded-tree-leaf-audio.md).

Signal preparation and automated PCM/lifecycle checks do **not** establish human listening
approval, perceptual loudness matching against procedural audio, or a new license.
All derivative rights inherit the restrictions below.

## Usage rights

**Commercial release blocked — Free-plan generation (confirmed by the user; checked 2026-10-23).** ElevenLabs' [official publication policy](https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform) says Free-plan outputs cannot be used commercially. Attribution does not grant commercial permission, and later upgrading does not automatically license previously generated files. This applies to the originals and any derivatives in this folder.

The user authorized importing these files, but that is not a commercial or redistribution license. Before commercial use, replace them with qualifying paid-plan generations or obtain explicit written authorization from ElevenLabs for these outputs. No such authorization has been supplied. Billing details, generation-time terms/notice history and any separate agreement have not been verified.

The [ElevenCreative Terms, Exhibit 4 section 2](https://elevenlabs.io/creative-terms) also restrict standalone/isolated SFX distribution. Do not assume even paid-plan outputs may be published as raw WAVs in a public repository or reusable asset library. Nothing was pushed during this import/review work. This note records the published policy, not a separate license grant.
