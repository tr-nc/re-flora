# Butterfly v2 — isolated Blender-to-pixel prototype

This is an editable, reproducible experiment, not a production replacement. No files under the game repository or the v1 `../blender/` directory were changed. The final review page is `../comparison-v2.html`, served locally at `http://127.0.0.1:8788/butterfly-method-comparison/comparison-v2.html`.

## What changed

- Rebuilt for final 16px readability: one connected, two-lobe wing shape per side; four broad opaque colors; no spots, micro-stripes, image textures, or per-view geometry.
- Replaced the stationary body and pure wing sine with five authored poses: wing stroke, body lift and pitch, two abdomen follow-through segments. The amplitude and rhythm are stylized art direction, not a recovered anatomical motion or biological simulation.
- Raised every source camera to the same 45° elevation and set the same 2.95 orthographic scale. The original 2.55 framing clipped, so the scale was changed globally. No per-frame or per-view fit/recentering.
- Primary sprite frames are rendered directly at 16×16. The v1 approach used 128px renders downsampled with BOX; `diagnostic-v1-box-method-80.png` retains that sampling method on the new model to separate geometry/motion changes from rendering changes.

## Inspect the actual model and animation

`butterfly-prototype.blend` is the source and remains editable. The true 3D `butterfly-prototype.glb` is exported by reopening that saved source, not rebuilt as a separate browser model. The GLB has one shared 0–1 second animation clip containing the body translation/rotation, both abdomen rotations, and both wing hinges.

`blender-source-turntable.webm` and `turntable-frames/` are real 256px Blender renders from that same saved `.blend`: 60 frames at 15fps, a four-second 360° orbit and four wingbeat cycles. The preview orbit uses 35° elevation to expose the geometry; it is not a sprite camera or exact screenshot of the final 16px output. The comparison page supplies both real GLB interaction and exact five-frame sprite playback.

## Reproduce with one command

From this directory:

```sh
/usr/bin/python3 run.py
```

The command creates the source, saves it, reopens it for GLB export and all renders, reopens it again for an independent replay, builds the atlases, compares pixels, and creates the true 3D turntable video. It requires Blender 4.5.13 LTS, system Python with Pillow, and ffmpeg. Set `BLENDER=/path/to/blender` to override the discovered executable. The tested local installation is `~/.local/opt/blender-4.5.13-linux-x64/blender`.

To keep intentional edits in the saved `.blend` and export them without recreating the model:

```sh
/usr/bin/python3 run.py --export-existing
```

The script-created `source-manifest.json` documents this supplied authored model. After manual source edits its original key-pose table is historical unless deliberately updated; the replay and hashes always describe the actual saved source that was rendered.

For a fast render-only art check, append `--draft`; this explicitly does not claim replay or turntable validation.

## Export contract

| Parameter | Value |
| --- | --- |
| Rows | azimuth 0°, 45°, 90°, 135°, 180° |
| Columns | t = 0, .2, .4, .6, .8 seconds |
| Source frames | 1, 6, 11, 16, 21 at 25fps; frame26 equals frame1 |
| Sprite playback | 5fps, one-second loop |
| Camera | orthographic, scale2.95, elevation45°, target world(0,0,0) |
| World anchor | fixed origin; intentional body bob/pitch are not canceled |
| Raw renders | RGBA128px, Cycles CPU, 8samples, fixedseed0 |
| Primary final pixels | native16px, 1sample, BOXfilterwidth.01, fixedseed0 |
| Secondary size | native32px |
| Atlas | 5columns × 5rows: 80×80 at16px; 160×160 at32px; 640×640 raw |
| Transparency | raw transparent background; indexed output binary alpha, threshold128 |
| Palette | exactly5 PNG PLTE entries: 1transparent + 4opaque; no dithering |

The original hand-drawn rows are correspondence references, not known geometric camera truth. The source observations and measured reference facts are in `reference-observations.md`; selected first-party workflow precedents and their limits are in `research-3d-to-pixel.md`. The explicit five-key-pose art direction is in `KEY_POSES.md`.

## Art and engineering checks are separate

Read `QUALITY_REVIEW.md` for the visual conclusion and remaining 16px limits. `validation.json` records the independently replayed pixels, exact loop endpoints, actual GLB channels, palette/alpha/format, boundary checks, measured per-pose image differences, and source hashes. Pixel counts are diagnostic evidence, not an artistic acceptance score.

The actual rendered motion-layer controls are `diagnostic-wing-only-80.png`, `diagnostic-no-abdomen-lag-80.png`, and `diagnostic-body-only-80.png`. Suppressed transforms have their animation actions temporarily detached during rendering; the exporter verifies those comparisons really differ from the full result. It never substitutes CSS shapes or independently redrawn 2D art for the Blender output.
