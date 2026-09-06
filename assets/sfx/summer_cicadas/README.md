# Summer cicadas

Selected from the user-provided `~/Downloads/SA_SF_CricketsCicadas_96k_v1.0` pack. Original files remain untouched. See `provenance.json` for exact original filenames, SHA-256 hashes, trimming and FFmpeg filters.

The filenames explicitly identify these as synthesized cicadas. Only the package contents/file-list PDFs were present; no license text was found. User permission to copy into this repository is recorded, but no additional redistribution rights are asserted.

The two mono calls are resampled to 48 kHz PCM16, reduced by 3 dB, band-limited at 600 Hz and 11 kHz, and faded in over 0.6 s and out over 1 s. Dog Day retains its complete 10.03 s call; Linne's uses seconds 3–11 for a shorter answering phrase. They play once with bounded concurrency and silence between calls; these are not seamless loops.

## Runtime and review

`src/audio/summer_cicadas.rs` owns finite calls, scheduling, density, gain and retirement; `src/app/core/summer_cicadas.rs` reads committed foliage and a bounded sample of the live grass instance buffers. There are at most six habitats per kind and three simultaneous cicada calls. Starts are separated by at least 2.7 s, and each habitat rests 12–28 s after its 8–10 s call. Default gains are −20 dB in canopies and −24 dB in grass; existing master volume/mute applies. Habitat refresh is 0.5 s. No saved insect entities or additional GUI configuration are introduced.

An opt-in real-app acceptance run uses `RE_FLORA_CICADA_SMOKE=1`: save the live garden under `target/summer-evidence`, replace it twice while calls are playing, then remove a sounding canonical tree and verify its sources retire. Combine with the existing `RE_FLORA_GARDEN_SNAPSHOT_SMOKE=seed` to plant actual grass when the startup world has none. Use a long hidden muted release run (65 s), the shared GPU lock, and inspect `[AUDIO][CICADAS]` logs. This modifies only the disposable running scene and its target snapshot; restore any GUI configuration drift after the run.

WAV playback reviews the prepared source material. Muted app logs and screenshots verify lifecycle and placement, not the audible spatial mix. A headphone listening pass in the actual game remains necessary before calling the sound balance accepted. No release performance comparison has been made for this feature.
