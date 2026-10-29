# SFX Guide

## Runtime selection

`src/audio/runtime_assets.rs` is the authoritative manifest for file-backed
runtime sound effects. Playback and startup preloading share these declarations:
70 footstep variations, two cicada calls, eight tree-leaf loops, three apple impacts,
one sprinkler loop, one bee-flight loop, one grass-wind loop, one terrain-edit loop,
and one UI click (88 clips total). The [bee bundle](bee/README.md) and
[grass-wind bundle](grass-wind/README.md) record generation settings, crop and loop
preparation; their originals and intermediate crops are not runtime clips.
The cache does not scan this directory. Importing a WAV, original recording, or
candidate loop does not enable loading or playback; declare it and connect its
playback consumer when integrating it. Procedural audio needs no WAV declaration.

Declared files are still required and decoding errors remain startup errors.
Missing-asset silence/fallback and release-package filtering are separate work;
the current packaging script still copies the entire assets directory.

## Third-party provenance

- [Sound Armoury SFX Mega Bundle](SOUND-ARMOURY.md): terrain loop, UI button and
  the two [summer cicada calls](summer_cicadas/README.md). Purchase confirmed;
  commercial game use permitted under the EULA, public raw-file redistribution
  requires separate permission.
- [Footsteps](Footsteps%20SFX%20-%20Undergrowth%20%26%20Leaves/README.md): a separate
  High Impact Media purchase, not covered by the Sound Armoury bundle.

## Editing guidance

Use audacity to edit the audio files.

Normalize the audio sources.

[Seamless loop](https://youtu.be/OYecGSTqfa4?si=n23QG5S8XqMArtuo)

Export to WAV with 48000Hz sample rate.
