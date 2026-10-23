# NVIDIA spatiotemporal blue-noise textures

## Purpose in Re: Flora

Four 128×128×64 volumes, stored as 256 individual PNG slices. These are numerical
sampling data, not visible surface textures.

| Folder | Current use |
| --- | --- |
| `scalar_2d_1d_1d/` | Scalar noise for shadow-filter rotation and god-ray ray-march jitter |
| `unitvec2_2d_1d/` | 2D unit vectors for shadow UV jitter |
| `unitvec3_2d_1d/` | 3D unit-vector bank is loaded; no active sampling call found in current shaders |
| `unitvec3_cosine_2d_1d/` | Cosine-weighted bank is loaded; current tracer samples FAST's cosine bank instead |

Evidence: `src/tracer/resources.rs`, `shader/slang/tracer_shadowing.slang`,
`shader/slang/god_ray.slang`, `shader/slang/noise_tex.slang`.
All four banks still load at startup. Removing unused banks is a separate code
change, not accomplished by deleting PNGs.

## Exact source match

NVIDIA's original `NVIDIAGameWorks/SpatiotemporalBlueNoiseSDK` URL redirects to:
https://github.com/NVIDIA-RTX/STBN

On 2026-10-23, **all 256 installed PNGs matched byte-for-byte** the same-named
entries inside `Assets/STBN.zip` at upstream commit:
`48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b`.
The upstream archive has no separate README/license entry; its source repository
license must not be ignored.

- Archive: https://github.com/NVIDIA-RTX/STBN/blob/48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b/Assets/STBN.zip
- Archive SHA-256: `f262aaa79704b913ad1ac22b11674931c5c16a788688f8ce49ec43d59eb5c747`
- Installed files: [SHA256SUMS](SHA256SUMS). Folder regrouping is the only observed
  difference from the archive; PNG contents are unchanged.
- Original explanatory reference: https://developer.nvidia.com/blog/rendering-in-real-time-with-spatiotemporal-blue-noise-textures-part-1/

The precise original download date is unknown; this match identifies an exact
upstream copy, not a reconstructed download history.

## License status — do not equate technical data with unrestricted assets

[UPSTREAM-LICENSE.txt](UPSTREAM-LICENSE.txt) preserves **both** sections of the
upstream license. It is source evidence, not a declaration that the files are
CC0, BSD, MIT or licensed under this project's general asset license.

1. The non-commercial section grants redistribution with the license/notices,
   but section 3.3 limits use to research/evaluation.
2. A separate **Commercial Use License / NVIDIA RTX SDKs LICENSE** grants
   distribution as incorporated in object-code applications (§1(c)), subject to
   additional-functionality and other requirements (§2). It does not clearly
   grant raw PNG redistribution in an open-source asset folder, and restricts
   standalone SDK distribution (§4(b)).
3. Upstream commit `ba4da3b54298889d187c0d621fd96b34e7567d18` explicitly says the
   update adds language for commercial and non-commercial usage. Thus describing
   the entire repository as simply “non-commercial only” is also incomplete.

**Decision: source verified; public raw-file/commercial distribution not yet
cleared. Do not automatically move or replace the files in this documentation
change.** Ask NVIDIA whether these exact pregenerated PNGs (or generated output
in general) may be distributed as raw data in a public GPL game repository and
bundled in a commercial game. Private storage alone would not settle commercial
usage conditions. The general SDK language does not establish a CC0 exemption.

Contact given in the upstream terms: `nvidia-rtx-license-questions@nvidia.com`.
No permission request has been sent by this audit.

Pinned license:
https://raw.githubusercontent.com/NVIDIA-RTX/STBN/48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b/License.txt

Stored text uses LF and strips trailing whitespace, without changing wording.
SHA-256: `49894c9c6869c217c8573fdd40f60d76502a009850d3a1d8ee61fed0569d4976`.
