# Native grass review captures

These four files are **original runtime screenshots**, not AI-generated images or imported art. Creator/source: the Re: Flora renderer in this repository. They were copied and renamed only; no crop, resize, color correction, compositing, or format conversion was applied. All are 2560×1440, 8-bit sRGB PNG.

| File | Original filename / role | SHA256 |
|---|---|---|
| [near-reference.png](near-reference.png) | `near-both-b.png`, fixed near view, fine culling off | `153a579fcb514c5c049e822563911cf5b410aac9e2b4568aab17b5773e9f79ce` |
| [near-culled.png](near-culled.png) | `near-both-b.png`, same fixture/frame, fine culling on | `2f1cb8f545cb84d6c6d75a0c279aab7d331b2f55aef5521906ccae15caad5af7` |
| [mow-reference.png](mow-reference.png) | `orbit-both-b-180.png`, moving camera after real mowing, fine culling off | `5ff1c94b3febe1b25409d86609f50a36819691e50d403146eb28d17d880843b4` |
| [mow-culled.png](mow-culled.png) | `orbit-both-b-180.png`, same fixture/frame, fine culling on | `742f61851025ac86f28ad1cd3c90c721b4e8ceba0b4030e443d7007262b48665` |

Generation: Linux RTX 3060 Ti / NVIDIA 580.159.04, native hidden X11 window and Vulkan swapchain. Release binary SHA256 `fed5362a17c75d899eacdc3e1176fd3163cdb9cf917acb051a518040ba1811f1`. Source is the shared grass workset change committed with these captures, based on `e1870de6`.

Commands use `--hidden --mute --windowed --perf --authored-flora-bench` and the existing deterministic frame clock. Environment:

- `RE_FLORA_ROOT`: disposable directory containing a copied config and the project's assets.
- `RE_FLORA_GRASS_STEM_REVIEW`: `near-both-b` or `orbit-both-b`.
- `RE_FLORA_GRASS_STEM_GRID=3`, `RE_FLORA_GRASS_BAND_POSE_REUSE=1`.
- `RE_FLORA_STEM_SAMPLE_FRAMES`: 120 for near, 240 for orbit.
- `RE_FLORA_GRASS_VISIBILITY`: 0 for reference, 1 for culled.
- `RE_FLORA_GRASS_VISIBILITY_AUDIT=1`, `RE_FLORA_STEM_PIXEL_PROFILE=1`.
- `RE_FLORA_GRASS_STEM_CAPTURE`: output folder. `WAYLAND_DISPLAY` unset.

The fixture owns camera, planting, growth override, spawn clock and wind response; orbit also mows at frame 180 and replants at 240. Starting GUI config SHA256: `2cead7ccdd097416d94cd79e264bf1f7937e6c1c6431762f5f4324ac31df565f`; exact reproduction also needs that config. The full runner is `docs/evidence/grass-visible-workset/validate.mjs` in the source repository. Source assets and their existing license/attribution obligations still apply to these internal review captures; this note asserts no additional redistribution license.
