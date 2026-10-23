# Summer cicadas

Selected from the user-provided `~/Downloads/SA_SF_CricketsCicadas_96k_v1.0` pack. Original files remain untouched. See `provenance.json` for exact original filenames, SHA-256 hashes, trimming and FFmpeg filters.

The filenames explicitly identify these as synthesized cicadas. The library is
**Sound Armoury — SynFauna: Crickets & Cicadas**, listed in the
[SFX Mega Bundle](https://sound-armoury.itch.io/sfx-mega-bundle). On 2026-10-23 the
owner supplied that product URL and a purchase-confirmation screenshot dated
**2025-07-09**. No screenshot, account details or private download URL is committed.
The listed library matches the previously recorded source package; the current
storefront does not establish the exact historical bundle/archive version.

The [itch.io-specific EULA](https://drive.google.com/file/d/1KS3b6fp_ulzvbuZRgXyl_SFrKeL2ctJz/view?usp=sharing),
linked by the [creator](https://sound-armoury.itch.io/), is saved in full as
[LICENSE.txt](LICENSE.txt). It allows commercial/non-commercial game use and
modification, with attribution optional, but prohibits redistribution of content
(including modified content) without explicit permission. It also restricts team
sharing to the licensed project, prohibits generative-AI training and requires
agreement for political campaigning. Public raw WAV distribution is not cleared.
These assets are not relicensed by the project's general asset/code licenses.

The current one-page EULA PDF SHA-256 is
`dbe4c3981179b7bd00f15cd76ad0d32355f0106b23e9c5437333ade90afdfd26`.
The local text is extracted with pypdf with line-end whitespace/outer blank lines
removed. No effective date is stated; this is not a recovered purchase-time EULA.
See [bundle provenance](../SOUND-ARMOURY.md) for the other two installed sounds.
No special raw-file redistribution permission has been supplied.

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
