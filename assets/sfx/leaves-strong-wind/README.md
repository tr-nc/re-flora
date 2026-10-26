# Strong wind through leaves — four loop candidates

Four original ElevenLabs downloads supplied by the user. All are imported **byte-for-byte unchanged**, renamed for clarity. This folder contains the complete source/provenance record and all four takes; no external Downloads path or chat history is needed to identify them.

## Generation provenance

Settings transcribed from the user's generation-settings screenshot, supplied for these four files:

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
Seamless loop of a small cluster of leaves rustling vigorously under steady strong airflow. Close-up isolated point source, dense leafy friction and rapid crisp flutter. Consistent intensity, no gusts or swells. No wind roar, forest ambience, birds, branch creaks or reverb.
```

The screenshot shows the submitted prompt and improvement enabled; the effective rewritten prompt was not supplied. Seeds, generation URLs and verified generation IDs were not supplied. The download filenames below identify the source files; they are not asserted service generation IDs.

## Files

Each file is **8.000 seconds / 384,000 frames**, stereo **48,000 Hz signed 16-bit PCM WAV**.

| Imported original | Original download filename |
| --- | --- |
| [leaves-strong-wind-01.wav](leaves-strong-wind-01.wav) | `Seamless_loop_of_a_s_#1-1792741782176.wav` |
| [leaves-strong-wind-02.wav](leaves-strong-wind-02.wav) | `Seamless_loop_of_a_s_#2-1792741793931.wav` |
| [leaves-strong-wind-03.wav](leaves-strong-wind-03.wav) | `Seamless_loop_of_a_s_#3-1792741803001.wav` |
| [leaves-strong-wind-04.wav](leaves-strong-wind-04.wav) | `Seamless_loop_of_a_s_#4-1792741803002.wav` |

**Processing: none.** No trimming, crossfading, normalization, gain change, filtering, channel conversion, resampling, pitch shifting or time stretching. Each imported file is the original; no redundant derivative copy is needed. Downloads copies were removed only after byte-for-byte verification of the project copies.

SHA-256:

```text
fad0f2f1a919507d064e2960db74142167c359a33e92be6f489eacb019204580  leaves-strong-wind-01.wav
38ddffa16da4a349e87cc018a896d96a8defaf2fdac992e04bddeaa99f3edf48  leaves-strong-wind-02.wav
503213bf2a32e81ab6443b5f8352810da7a109e2575a0859f4b647a7dd8ca54b  leaves-strong-wind-03.wav
92b751e0eeb76f10cc841d89803bdff895fcd4cca99174e60ea390b859de1e0f  leaves-strong-wind-04.wav
```

## Intended use and review status

- Intended sound: a small localized cluster of leaves rustling vigorously under steady strong airflow, **not** a whole-tree/forest ambient wind bed or a wind-roar track.
- These are the **strong-wind candidates** for future positional leaf audio. The intended blend is between gentle-breeze and strong-wind textures based on live game wind strength; resources should maintain steady overall intensity rather than contain their own large gust cycles.
- All four takes supply ambient tree-leaf audio through separate runtime derivatives; each canopy independently pairs one with a gentle-breeze take. Since 2026-10-27 this is the only ambient tree-leaf implementation; the old A/B switch and procedural ambient path are removed.
- Preserve the original stereo downloads. Any later mono/positional preparation, level matching, filtering or seam repair must be recorded as a derivative in this bundle.
- Verified: all four decode successfully, match their downloads exactly, and have the frame count/sample format above.
- Not verified: audible seam quality, steadiness, lack of ambience, suitability of each take, or in-game spatial/loudness balance. Loop mode and prompt wording describe the generation request, not guaranteed audio quality. A listening pass is still required.

## Runtime derivatives (2026-10-24)

The originals above are still unchanged. `runtime/` contains four reproducible mono
PCM16 / 48 kHz derivatives plus [source/output hashes](runtime/manifest.json).
Only derivatives are in the preload manifest, never the originals.

The common preparation recipe is stereo channel mean, DC removal, 100 ms equal-power
tail/head overlap and RMS matching to −30 dBFS. Each runtime loop is **379,200 frames /
7.9 s** at original speed. Run `python3 scripts/prepare_tree_leaf_loops.py --check`
to verify all eight derivatives, or omit `--check` to rebuild them.

Strong-wind takes use an independent random choice and offset from the gentle layer.
The smoothed local wind response drives an equal-power blend inside the existing canopy
Voice; zero-weight layers still advance, and changing wind does not restart playback.
Existing Tree leaves mixer and Tree Wind Volume remain the saved loudness controls.

See [implementation/validation](../../../docs/reports/recorded-tree-leaf-audio.md).
Automated checks do not establish listening approval or commercial permission.
All derivatives inherit the restrictions below.

## Usage rights

**Commercial release blocked — Free-plan generation (confirmed by the user; checked 2026-10-23).** ElevenLabs' [official publication policy](https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform) says Free-plan outputs cannot be used commercially. Attribution does not grant commercial permission, and later upgrading does not automatically license previously generated files. This applies to the originals and any derivatives in this folder.

The user authorized importing these files, but that is not a commercial or redistribution license. Before commercial use, replace them with qualifying paid-plan generations or obtain explicit written authorization from ElevenLabs for these outputs. No such authorization has been supplied. Billing details, generation-time terms/notice history and any separate agreement have not been verified.

The [ElevenCreative Terms, Exhibit 4 section 2](https://elevenlabs.io/creative-terms) also restrict standalone/isolated SFX distribution. Do not assume even paid-plan outputs may be published as raw WAVs in a public repository or reusable asset library. Nothing was pushed during this import/review work. This note records the published policy, not a separate license grant.
