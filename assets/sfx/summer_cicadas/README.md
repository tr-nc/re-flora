# Summer cicadas

Selected from the user-provided `~/Downloads/SA_SF_CricketsCicadas_96k_v1.0` pack. Original files remain untouched. See `provenance.json` for exact original filenames, SHA-256 hashes, trimming and FFmpeg filters.

The filenames explicitly identify these as synthesized cicadas. Only the package contents/file-list PDFs were present; no license text was found. User permission to copy into this repository is recorded, but no additional redistribution rights are asserted.

The two mono calls are resampled to 48 kHz PCM16, reduced by 3 dB, band-limited at 600 Hz and 11 kHz, and faded in over 0.6 s and out over 1 s. Dog Day retains its complete 10.03 s call; Linne's uses seconds 3–11 for a shorter answering phrase. They play once with bounded concurrency and silence between calls; these are not seamless loops.

## Runtime and review

`src/audio/summer_cicadas.rs` owns finite calls, gain and retirement. The shared ecology scheduler
in `src/ecology.rs` supplies both butterflies and cicadas from real grass, authored plants and
committed tree leaves through `src/app/core/ambient_ecology.rs`. There are at most three concurrent
calls, separated by at least 2.7 seconds. A region rests 24–40 seconds after an accepted call starts.
Default gains remain −20 dB in canopies and −24 dB in grass/plants; existing master volume/mute applies.
Hosts are validated every 0.5 seconds, and world replacement clears old calls and ecology state.

See [shared ecology](../../../docs/ecology_spawning.md) for sampling budgets, initial density tuning,
release measurement boundaries and opt-in real-app fixtures. `RE_FLORA_CICADA_SMOKE=1` remains an
alias for the shared lifecycle acceptance, which now also checks removed grass/plant hosts and an
empty garden. Original audio provenance and processing are unchanged.

Muted app logs and screenshots verify lifecycle and placement, not the audible spatial mix.
A headphone listening pass in the actual game remains necessary before calling sound balance accepted.
