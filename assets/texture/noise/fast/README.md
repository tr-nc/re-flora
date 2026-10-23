# FAST noise textures

## Purpose in Re: Flora

These are numerical sampling tables, not decorative art. Each folder contains
64 PNG slices, 128×128 pixels each; frame index selects a slice modulo 64.

| Folder | Values | Current use |
| --- | --- | --- |
| `weighted_cosine/` | Cosine-weighted hemisphere directions encoded in RGB | `shader/slang/tracer.slang`: diffuse bounce direction and sun-disk sampling |
| `unit_vec3/` | Uniform unit-sphere directions encoded in RGB | Loaded by `src/tracer/resources.rs`; declared in the god-ray shader, but no active sample call was found in the current source audit |

All 128 files are still loaded at startup. Loaded does not necessarily mean
sampled by a shader. Removing unused bindings/resources is separate work.

## Source and generation

The original project note identifies **Electronic Arts / SEED FastNoise**:
https://github.com/electronicarts/fastnoise

Authors credited upstream: William Donnelly, Alan Wolfe, Judith Bütepage and
Jon Valdés. This is not Auburn's similarly named FastNoise library.

Historical commands, retained without inventing missing settings:

| Folder | Command |
| --- | --- |
| `weighted_cosine/` | `FastNoise.exe sphere cosine binomial 2.0 exponential 0.04 0.2 product 128 128 64 out -split` |
| `unit_vec3/` | `FastNoise.exe sphere uniform binomial 2.0 exponential 0.04 0.2 product 128 128 64 out -split` |

The generation-time executable revision, random seed, optimization defaults and
any intermediate handling were not recorded. The revision below pins the
**license/source audit**, not a claim that the old PNGs were produced with that
exact version. No texture was regenerated, replaced or re-encoded in this audit.
The installed PNGs are identified by [SHA256SUMS](SHA256SUMS).

## License / public repository decision

**Keep in the public repository.** Upstream is BSD-3-Clause, allowing source and
binary redistribution and commercial use subject to notices/disclaimer and
non-endorsement conditions. [LICENSE.txt](LICENSE.txt) includes EA's copyright
and complete license. Keep it with these textures in source and release packages.
The project-wide CC BY-NC-SA asset label must not replace the upstream notice.

Source reviewed on 2026-10-23, commit
`2cf53e4bb510d07511fe63a312556d2a2e108c70`:

- https://github.com/electronicarts/fastnoise/blob/2cf53e4bb510d07511fe63a312556d2a2e108c70/README.md
- https://raw.githubusercontent.com/electronicarts/fastnoise/2cf53e4bb510d07511fe63a312556d2a2e108c70/LICENSE.txt

The README documents both generated noise and supplied noise textures. No
separate output-only restriction was found. The project provenance says these
files were generated with the tool, not imported proprietary art. The BSD notice
is included conservatively; it is not a claim that all arbitrary outputs of
BSD software automatically inherit BSD or that the historical seed is known.

License text is copied with LF line endings and trailing whitespace removed;
wording is unchanged. Stored license SHA-256:
`98bf383781ca7a4ac24e332f0dc8c38f43c67b23a2eb5fa7aa13e101e9943b66`.
