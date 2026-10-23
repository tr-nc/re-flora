# NH172 HRTF — ARI artificial / Neumann KU 100

## Purpose

`hrtf_b_nh172.petalhrtf` contains paired left/right impulse responses for 1,550
sound directions. PetalSonic uses these FIR filters to render spatial sounds
binaurally; it is not an ambience recording or sound-effect clip.

There are two byte-identical copies in the repository:

- `assets/hrtf/`: Re: Flora explicitly supplies this runtime override path.
- `crates/petalsonic/asset/hrtf/`: PetalSonic embeds the default at compile time.

The app's explicit override currently does **not** automatically fall back when
its path is missing. Keep both the data and notices until that behavior changes.

## Attribution and license

**Keep publicly, under Creative Commons Attribution-ShareAlike 3.0 Unported
(CC BY-SA 3.0).** Commercial use and redistribution are permitted subject to
attribution and the other terms. Keep this attribution, the source link, change
notice and [full license](LICENSE-CC-BY-SA-3.0.txt) with the data; distribute
adaptations of this data under the applicable ShareAlike terms. Do not label it
MIT (the audio code's license) or CC BY-NC-SA (the project's general assets).

- Dataset: **HRTF B**, ARI (ARTIFICIAL), listener **nh172**, Neumann KU 100.
- Organization: **Acoustics Research Institute, Austrian Academy of Sciences**.
- Author contacts recorded in the source: Piotr Majdak (`piotr@majdak.com`) and
  Michael Mihocic (`michael.mihocic@oeaw.ac.at`).
- Source creation: 2014-05-15; source modification: 2020-03-31.
- Official source: https://sofacoustics.org/data/database/ari%20%28artificial%29/hrtf%20b_nh172.sofa
- Dataset index: https://sofacoustics.org/data/database/ari%20%28artificial%29/
- Source file's own `License` attribute reads exactly:
  `Creative Commons Attribution-ShareAlike 3.0 Unported License`.
- Legal code: https://creativecommons.org/licenses/by-sa/3.0/legalcode.txt

This conclusion uses the **actual source file's metadata**, not a different
ARI catalogue entry's CC BY / CC BY-SA 4.0 label. The supplied license text has
LF line endings and trailing whitespace removed, with wording unchanged.

## Verified provenance and changes

Verified 2026-10-23 without changing any HRTF bytes:

1. The official SOFA download is byte-identical to the repository's original
   `assets/hrtf/hrtf_b_nh172.sofa` at `7610c5a2^`.
2. Commit `7610c5a2b34ffc6d09c2ba8240abfbc103461bd6` introduced the native table.
   Its historical progress note records conversion with the then-sibling
   `../petalsonic/tools/sofa_to_petalhrtf.py`; that original converter is not
   present in this checkout.
3. Independent reconstruction from the SOFA data reproduces the **entire current
   native table byte-for-byte**. This verifies the observed conversion, not the
   identity/version of the old tool.

Observed conversion recipe:

- Source IR shape: `[1550, 2, 256]`; sample rate 48,000 Hz; all source delays zero.
- For each SOFA spherical direction (azimuth `a`, elevation `e`, radians), store
  float32 listener-local `(-cos(e)*sin(a), sin(e), cos(e)*cos(a))`.
- Retain direction order and left/right receiver order. Convert each IR value
  to little-endian float32, without resampling, gain adjustment, normalization,
  trimming or additional filtering.
- Write `PETHRTF\0`, then little-endian u32 values `1, 48000, 1550, 256`; then for
  each direction write three float32 components, 256 left taps and 256 right taps.
- Native format is documented in `crates/petalsonic/src/spatial/native_hrtf.rs`.

Checksums and audit measurements are in [provenance.json](provenance.json).

| File | SHA-256 |
| --- | --- |
| Original SOFA | `22c79a11c384247f59e1c3b76e5f05a5eaf6b4a6632a325b7796163820dbe416` |
| Native table (both copies) | `cc902afd49715e4c0b781dbebb0ca625ed570c702fae2ab2038323d7c7991859` |
| Stored CC BY-SA 3.0 legal text | `3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f` |

No source SOFA was re-added to the repository. The data-specific license does not
purport to relicense unrelated game code; redistribution must still preserve the
data's attribution/ShareAlike obligations, including copies embedded in binaries.
