# Footsteps Sound FX — Undergrowth / Leaves

## Source and purchase evidence

- Creator/publisher: **High Impact Media**; the filenames and older storefront
  link identify **Tom Winandy SFX**.
- Product: https://high-impact-media.itch.io/footsteps-sound-fx-undergrowth-leaves
- Original download archive name shown on the purchase page:
  `Footsteps SFX - Undergrowth & Leaves.zip`.
- The project owner supplied a screenshot of the itch.io purchase/download page
  on 2026-10-23. It confirms purchase on **2025-07-08**. This records purchase
  evidence, not an independently verified transaction or permission to publish
  raw files. The private screenshot, email, account details and download links
  are deliberately not committed.

## Files and processing history

The product advertises 120 WAV files: walk 25, run 25, sprint 25, sneak/crouched
25, jump 10 and land 10, at 44.1 kHz / 16 bit. This folder contains the **70-file
runtime subset**:

| Filename pattern | Index range | Role |
| --- | --- | --- |
| `TomWinandySFX - FS_UndergrowthLeaves_walk_NN.wav` | 01–25 | Walking |
| `TomWinandySFX - FS_UndergrowthLeaves_run_NN.wav` | 01–25 | Running |
| `TomWinandySFX - FS_UndergrowthLeaves_jump_NN.wav` | 01–10 | Jumping |
| `TomWinandySFX - FS_UndergrowthLeaves_land_NN.wav` | 01–10 | Landing |

All installed files decode as **48 kHz, mono, PCM16**. The installed sample rate
therefore differs from the advertised source. The original archive has not
been compared byte-for-byte; the exact resampling tool, settings, and any other
historical edits are unknown. Do not describe these as verified untouched
originals. No WAV was changed while adding this documentation.

[SHA256SUMS](SHA256SUMS) identifies every installed WAV. Validate from this folder:

```sh
shasum -a 256 -c SHA256SUMS
```

Runtime declarations live in `src/audio/runtime_assets.rs`. The bank is currently
used for all footstep surfaces; adding a file to this folder alone does not add
it to playback or startup preloading.

## License — commercial project use, not public raw redistribution

The product page links to this publisher agreement:
https://docs.google.com/document/d/1LPXeZS7uFktK4ZbK96Tqgs-u7SlJfR_kE8AFIHi2HtM/edit

[LICENSE.txt](LICENSE.txt) is the verbatim text export retrieved on 2026-10-23:
https://docs.google.com/document/d/1LPXeZS7uFktK4ZbK96Tqgs-u7SlJfR_kE8AFIHi2HtM/export?format=txt

License snapshot SHA-256:
`a28cfe8233177905ee85a26722ad3f36e5ae89c094e02c280183b833fe011267`.

The currently linked agreement grants non-exclusive, non-transferable,
royalty-free use in unlimited commercial and non-commercial projects, including
video games, and allows modification. Attribution is appreciated, not required.
It explicitly prohibits standalone redistribution and says:

> You CANNOT include these sounds in an open-source project or game engine template where the end-user can easily extract the raw audio files.

**Do not publish these raw WAVs in the public source repository under this
agreement.** Purchase does not grant a sublicense to other repository users.
The repository's general asset license does not apply to these recordings.
The agreement in effect at the original purchase date has not been recovered;
this snapshot is not proof of the historical terms or a special exception.

The current checkout still contains the audio; adding provenance does not resolve
public distribution or remove earlier Git history. Asset separation/history
cleanup and any publisher clarification are separate work, required before
claiming that public redistribution is cleared. Merely excluding files from
startup loading does not exclude them from the current release packaging script.
