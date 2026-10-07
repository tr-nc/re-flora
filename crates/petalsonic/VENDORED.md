# PetalSonic 0.9.2 — narrow acoustic-start patch

This is the crates.io 0.9.2 source, upstream commit
`a695916b881ee86b892ee645ac2742ef29a81f16` (`petalsonic/`), MIT licensed.
Original crate checksum: `eab4fe7579d15c757d1d172bdb8d7a4ab5e76450d90ae0cbd9a3e4e66241aa0e`.
Repository: https://github.com/tr-nc/petalsonic

The in-tree patch makes Re: Flora's dependency reproducible without an absolute
path, unpublished remote commit, or changes to another checkout. Only the
opt-in finite spatial acoustic-start contract, its native renderer/playback
implementation, timeout telemetry and tests differ from published source.
Ambient/default playback, synthesis, mix gains and final-output limiting are
not changed. In particular the sibling upstream's later peak limiter is not
included by this patch.

`PlayOptions::with_acoustic_start()` preserves the PCM cursor while waiting
for a usable initial transmission response, initializes point/lobe/environment
filters consistently with that response, then retains ordinary smoothing.
The wait is bounded by `ACOUSTIC_START_MAX_DELAY` (100 ms), audio processing never
blocks, and explicit stop/destroy or timeout cancels unrendered voices safely.
Remove this patch when an upstream release contains the equivalent tested fix.
