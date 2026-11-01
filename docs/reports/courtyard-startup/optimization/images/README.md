# Native courtyard screenshots

These three PNGs are original in-app screenshots, not AI-generated images, imported photographs, textures, or runtime game resources. Provider/model/prompt/seed/negative prompt are not applicable: the images were rendered by this repository's native Release application at runtime commit `70179a06`, on the local RTX 3060 Ti. Garden seed: 20260519. Format: 1600×1000, 8-bit RGBA PNG, non-interlaced.

| File | Role | Capture |
|---|---|---|
| [tree.png](tree.png) | Original visual evidence: fresh courtyard, fixed specimen camera | `--screenshot courtyard-tree` |
| [arrival.png](arrival.png) | Original visual evidence: optimized save reloaded, arrival camera | `--terrain-load target/courtyard-optimized-validation.rflterrain --screenshot garden-arrival` |
| [old-save.png](old-save.png) | Original visual evidence: pre-optimization save reloaded, specimen camera | `--terrain-load target/courtyard-veteran-validation.rflterrain --screenshot courtyard-tree` |

Exact common capture invocation:

```sh
RUST_LOG=info target/release/re-flora --hidden --mute \
  [the terrain-load arguments above, if any] \
  --screenshot <preset> docs/reports/courtyard-startup/optimization-assets/<file> \
  --screenshot-delay 3 --auto-exit 5
```

The files were captured in the temporary `optimization-assets/` folder, then moved unchanged to this self-contained `optimization/images/` bundle. Source logs are [../raw/accepted-tree.log.gz](../raw/accepted-tree.log.gz), [../raw/accepted-load.log.gz](../raw/accepted-load.log.gz) and [../raw/accepted-old-save.log.gz](../raw/accepted-old-save.log.gz). No crop, resize, color correction, denoising, sharpening or format conversion was performed on these files. The report uses responsive CSS display sizing; that is not a derivative image file. Native rendering includes the game's normal pixel/post-process appearance and can vary with animation/lighting time; these are not pixel-equality tests.

SHA-256:

```text
bbb08851bc7ebe07255d307fba0f9c48366452d49b07323bea86e14b29ac0ec5  tree.png
084621b38ff70a093ffae5cea95beb7dea3063d02918953e484840baf3b03c52  arrival.png
4dcf08ca315676a7aa6ab920935f44c8c5de889e704013865ba00e7588d5d52b  old-save.png
```

Creator/source: re-flora's procedural world and existing repository assets, rendered locally for this development report. No external reference image was incorporated. Existing game assets retain their original licenses and attribution requirements; this note does not grant independent redistribution rights for all depicted game content. No new third-party asset license is introduced by this screenshot capture.
