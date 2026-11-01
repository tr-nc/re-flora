# Glass sampling evidence assets

These are generated research illustrations, not runtime game assets or AI images.
Every bundled image, original capture role/path, transformation, dimensions and SHA-256 is listed in [provenance.json](provenance.json). Source is the Re:Flora native Vulkan renderer; provider/model/prompt/seed are not applicable to these game captures. Underlying game assets retain existing rights; no independent redistribution license is asserted.

- `window-*.jpg`: derivatives of original 3840×2160 game screenshots, crop (1152,540,1728,972), Lanczos resize to 1152×648, JPEG q=2 via ffmpeg. Originals were generated locally, not externally supplied; full PNGs remain in diagnostic output at the recorded paths, not in this bundle.
- `front/side/level/inside-*.png`: lossless world-cell HDR sprites, 48×696 RGBA, 24 frames of 48×29 cells each. One fixed glass layer per pose; no screen motion, blur or temporal accumulation. Display applies the same Reinhard + IEC sRGB mapping to all candidates; this is explicitly not the game's complete post-processing pipeline. Missing cells are transparent.
- The native little-endian 16-byte u32x4/f32x4 record captures are stored under the recorded diagnostic paths; their ordered combined hashes are retained. The temporary readback build's executable hash was not archived. Its read-only probes were removed before final Release GPU measurements.

The assets and their meaning remain self-contained here; regenerating them from raw buffers requires the diagnostic inputs. See ../generate.mjs and ../data.json for processing and numeric evidence.
