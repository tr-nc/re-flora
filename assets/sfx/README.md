# SFX Guide

## Runtime selection

`src/audio/runtime_assets.rs` is the authoritative manifest for file-backed
runtime sound effects. Playback and startup preloading share these declarations:
70 footstep variations, two cicada calls, one terrain-edit loop, and one UI click.
The cache does not scan this directory. Importing a WAV, original recording, or
candidate loop does not enable loading or playback; declare it and connect its
playback consumer when integrating it. Procedural audio needs no WAV declaration.

Declared files are still required and decoding errors remain startup errors.
Missing-asset silence/fallback and release-package filtering are separate work;
the current packaging script still copies the entire assets directory.

Use audacity to edit the audio files.

Normalize the audio sources.

[Seamless loop](https://youtu.be/OYecGSTqfa4?si=n23QG5S8XqMArtuo)

Export to WAV with 48000Hz sample rate.
