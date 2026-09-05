# Butterfly sprite workflow investigation

Date: 2026-09-06
Scope: inspect the current handmade butterfly source and runtime consumer; compare low/no-cost creation workflows. No replacement art was generated, no service was purchased, and no game code was changed.

## Recommendation

Use a deliberately short, free PixelLab trial as a feasibility gate, not as the production pipeline:

1. Give PixelLab the current sprite (integer-upscaled, without smoothing) as the style/object reference and ask for rotations plus a short wing animation.
2. Spend at most two attempts checking identity, framing, wing markings, and column-to-column phase alignment across the two runtime-critical views.
3. If any of those drift, stop generating. Keep the result only as design or motion reference.
4. Produce the shippable atlas from one minimal Blender butterfly rig, rendered through five fixed orthographic cameras, and finish it in Pixelorama (or Aseprite if a license is already owned).

The reason is structural rather than aesthetic. Current AI products can make attractive individual sprites and some can create rotations or animation, but none of the reviewed first-party documentation promises Re: Flora's exact invariant: five camera views of the same five wing poses, in the same columns, at 16 × 16 pixels, with one transparent plus four opaque grayscale palette entries. A single 3D rig animated once gives every camera the same pose at a given frame by construction.

At 16 × 16, the 3D asset does not need sculpting. An elongated body, head, two pairs of flat wings, and one mirrored wing hinge are enough to establish view and pose. Pixel cleanup—not detailed modeling—defines the final style.

## What is in the repository now

### Authored and runtime files

| File | Actual role | Audited facts |
| --- | --- | --- |
| `assets/texture/butterfly_16px/butterfly.aseprite` | Editable source | File metadata reports a 16 × 80 canvas, 5 animation frames, indexed mode, transparency index 0, and 5 colors. Runtime does not read it. |
| `assets/texture/butterfly_16px/butterfly.png` | Runtime atlas | 80 × 80 indexed PNG, 5 × 5 cells of 16 × 16, exactly 5 palette entries, binary alpha, grayscale RGB. SHA-256: `c2d7c8169301cabcb93cc666f12f9ab3f36801eb36bbb42170a8a0f1502c9b82`. |
| `assets/texture/butterfly_16px/Blue.png` and other named colors | Original style references | 80 × 16, five horizontal frames, single side view. They are not selected while `butterfly.png` is present. |
| `assets/texture/butterfly_16px/PLAN.md` | Intended authoring contract | Defines five rows as front 0°, front-side 45°, side 90°, rear-side 135°, rear 180°; columns are five synchronized wing phases. |

The Aseprite layout explains the export: each Aseprite animation frame is 16 × 80 and contains all five view rows vertically; exporting its five timeline frames horizontally produces the 80 × 80 PNG. This preserves the crucial meaning that one output column is one common animation time.

Visual inspection of the actual PNG confirms the five intended silhouettes: symmetric front/rear forms at the first and fifth rows, increasingly lateral silhouettes through the middle rows, and five different wing poses from left to right. It also shows that the plan's requested “remove black outline” step is not reflected in the final asset: black remains the darkest opaque palette entry. The runtime palette code explicitly models that entry as `Border` and recolors rather than removes it (`src/tracer/butterfly_palette.rs:6-43,100-163`).

### Exact import and palette contract

The resource loader:

- enumerates every PNG in `assets/texture/butterfly_16px`, sorts them, and preferentially selects a case-insensitive `butterfly.png`; only if it is missing does it fall back to the first PNG (`src/tracer/resources.rs:1585-1631`);
- asserts that the selected file is exactly 80 × 80 (`src/tracer/resources.rs:1633-1648`);
- asserts indexed PNG mode, then infers exactly five RGBA roles: one transparent, border, dark shade, mid shade, and light shade (`src/tracer/butterfly_palette.rs:165-187`, `src/tracer/palette_remap.rs:51-87`);
- expands the grayscale source into seven runtime palettes: yellow, purple, orange, white, red, blue, and brown (`src/tracer/butterfly_palette.rs:47-97`);
- extracts only physical atlas rows 1 and 3, with five frames each (`src/particles/animation.rs:18-27`, `src/tracer/resources.rs:1650-1672`).

The source PNG satisfies the machine-level format today. ImageMagick inspection found these exact entries:

```text
(0, 0, 0, 0) transparent
(0, 0, 0, 255) border
(148, 148, 148, 255) dark shade
(179, 179, 179, 255) mid shade
(255, 255, 255, 255) light shade
```

There are no intermediate alpha values: the atlas alpha minimum is 0 and maximum is 255. RGB is grayscale. Each extracted cell uses a subset of the same global palette, which is valid because the loader validates colors at atlas scope.

### Direction switching and playback

The runtime is a camera-facing billboard, not a five-direction actor:

- Five frames advance in ascending order every 0.2 seconds and wrap modulo five, giving a 5 fps, one-second loop (`src/particles/system.rs:444-459,584-592`). New particles start at frame offset zero (`src/particles/system.rs:350-369`). The Aseprite file's own frame durations or tags are not imported.
- Planar velocity dotted with camera-forward chooses logical view 0 when non-negative and view 1 when negative. Those logical views map to physical rows 1 and 3 respectively (`src/tracer/mod.rs:6006-6014,6041-6058`; `src/particles/animation.rs:21-27`). Rows 0, 2, and 4 are currently unused.
- Planar velocity dotted with camera-right determines a horizontal flip. The high bit of the texture index carries the flip into the vertex shader, which mirrors UV.x (`src/tracer/mod.rs:6016-6034,6074-6083`; `shader/slang/particle_lod_textured.vert.slang:112-117`).
- Velocity below 0.01 units/s selects view 0 and no flip. There is no directional hysteresis, so mismatched centering between rows 1 and 3 would be visible as a pop near the camera-forward boundary.
- The texture uses nearest min/mag/mipmap filtering and no mipmaps (`src/tracer/resources.rs:1674-1686`; `crates/re-flora-vkn/src/memory/texture/desc.rs:377-394`). The fragment shader applies an alpha cutoff at 0.5 (`shader/slang/particle_lod_textured.frag.slang:18-31`).

The practical art implication is that rows 1 and 3, plus their horizontal mirrors, are the immediate acceptance priority. Keeping all five authored rows is still sensible because the loader requires the 80 × 80 shape and the unused rows preserve a future route to finer view selection.

## Tool findings from first-party sources

Prices and free allowances below were checked on 2026-09-06 and can change.

### 1. PixelLab: best direct-AI trial

PixelLab is the closest fit among the tools reviewed. Its official product page says it supports text and skeleton animation, automatic 4- and 8-direction views, reference-based style matching, rotation, and inpainting. Its API publishes separate operations for eight-direction objects, per-direction object animation, text animation, forced palettes, and transparent output. The published API shapes are useful evidence that these are real structured operations rather than a generic “draw a sprite sheet” prompt ([PixelLab product](https://www.pixellab.ai/), [PixelLab API model list](https://www.pixellab.ai/pixellab-api)).

Cost and rights are unusually friendly for a feasibility test: the official page lists 40 fast free generations, followed by five slower generations daily that can accumulate to 20, with no credit card. It says free outputs may be used commercially and that the user owns generations except for using them to train models. Tier 1 is $12/month for 2,000 images and the full animation tools ([PixelLab pricing on the official product page](https://www.pixellab.ai/)).

Important limitation: the product can generate directions and can animate a direction, but the official documentation does not state that separately animated directions share an exact skeletal phase. The API's “animate object” prices and runs per direction, which is evidence that cross-direction animation is not one atomic five-view/one-timeline operation. Published templates also commonly use four frames and larger frame sizes. Therefore PixelLab can win a short empirical gate, but it cannot be accepted on feature labels alone.

Recommended probe:

- treat the butterfly as an **object**, not a humanoid character;
- use an integer-nearest upscale of one or more current frames so the reference is legible to a cloud model;
- request an eight-direction rotation, retain the five semicircle angles corresponding to 0° through 180°, then animate the two views current runtime uses;
- reject the candidate if the thorax length, wing markings, bounding-box center, or wing angle differs between same-index frames across views;
- do not manually repair a drifting 25-cell sheet—at that point the 3D route is cheaper.

### 2. Scenario Retro Diffusion: real sprite-sheet support, wrong exact template

Scenario's RD Animation is a genuine pixel-art animation model, not merely a general image model presented as one. Official documentation says it produces low-frame-count sprite sheets, offers dedicated four-direction/idle/VFX layouts, accepts references and custom palettes, and aligns frames for engine import. The same documentation says each style is tied to supported sizes such as 32 × 32 or 48 × 48, and is limited to short loops ([Scenario RD Animation guide](https://help.scenario.com/articles/4202673551-retro-diffusion-models-the-essentials)). Scenario separately documents an image-to-video workflow in which key poses are extracted and assembled into a sheet; it warns that motion may be slow or imperfect ([Scenario sprite-sheet workflow](https://help.scenario.com/articles/9088582240-create-spritesheets-with-scenario)).

The current price page lists 50 free daily credits, no card, Starter at $15/month, and Pro at $45/month with custom-model training. It also says free outputs are personal/evaluation only, while paid plans include commercial rights ([Scenario pricing](https://www.scenario.com/pricing)).

This makes Scenario a useful evaluation-only alternative, not the first production choice. There is no first-party evidence of a custom 16 × 16, five-frame, five-angle butterfly template. Custom model training is also a poor match for the current source: Scenario recommends 5–15 subject images or 10–30 style images, ideally 1024 × 1024, whereas the current authored cells are 16 × 16.

### 3. Runway: motion/reference board, not a final sprite generator

Runway's Gen-4 References can reuse one to three reference images to retain subject or style characteristics, and an image can be moved into an image-to-video workflow ([Runway Gen-4 Image](https://help.runwayml.com/hc/en-us/articles/37053594806419-Creating-with-Gen-4-Image), [Runway References](https://help.runwayml.com/hc/en-us/articles/40042718905875-Creating-with-Gen-4-Image-References)). The free plan has 125 one-time credits and 5 GB storage; free videos carry a watermark and model availability can change ([Runway free plan](https://help.runwayml.com/hc/en-us/articles/50404627334547-Free-plan-details)). Runway states that creations have no non-commercial restriction from Runway and that, as between user and Runway, the user retains rights ([Runway commercial-use statement](https://help.runwayml.com/hc/en-us/articles/21668707517587-Can-I-use-the-content-I-made-in-Runway-for-commercial-purposes)).

However, Gen-4 Image outputs 720p or 1080p rather than structured pixel sprites, and neither image references nor video promises synchronized multi-camera views, an indexed palette, a fixed 16 × 16 silhouette, or transparent sprite cells. It is reasonable for exploring wing motion and choosing five poses, but not for producing the runtime atlas directly.

### 4. Blender plus Pixelorama: strongest production path

Blender supports orthographic cameras, where parallel lines do not converge; orthographic scale controls projected object size. It can render RGBA PNG and transparent film, and its recommended animation workflow writes one lossless image per frame ([Blender cameras](https://docs.blender.org/manual/en/5.0/render/cameras.html), [Blender output formats](https://docs.blender.org/manual/en/5.0/render/output/properties/output.html), [Blender animation rendering](https://docs.blender.org/manual/en/latest/render/output/animation.html), [Blender film transparency](https://docs.blender.org/manual/en/5.0/render/cycles/render_settings/film.html)). Blender is free software and states that images, movies, `.blend` files, and other artwork created with it are the creator's property and may be used commercially ([Blender license](https://www.blender.org/about/license/)).

Pixelorama is a free, MIT-licensed editor with an animation timeline, onion skinning, indexed mode, pixel-specific scaling/rotation, palette management, and PNG/sprite-sheet export. It runs on Linux, Windows, macOS, and the web ([Pixelorama official repository](https://github.com/Orama-Interactive/Pixelorama)). It is the recommended no-cost finishing tool.

If Aseprite is already licensed, it fits the existing source format directly. Its official documentation supports indexed conversion and matrix sprite-sheet import/export, while its FAQ lists a $19.99 minimum purchase and permits commercial game assets ([Aseprite sprite sheets](https://www.aseprite.org/docs/sprite-sheet/), [Aseprite indexed CLI](https://www.aseprite.org/docs/cli/), [Aseprite FAQ](https://www.aseprite.org/faq)). Neither `aseprite` nor `libresprite` is installed in the audited worktree environment, so the source file was inspected through decoded file metadata and its exported runtime PNG, not through an editor UI. This does not affect the runtime conclusions because the engine reads only the PNG.

## Concrete deterministic workflow

1. **Lock the atlas specification.** Keep 5 rows × 5 columns, 16 × 16 per cell, with rows front/front-side/side/rear-side/rear and columns as synchronized wing phases. Preserve the existing 5 fps forward loop.
2. **Build a low-detail rig.** Use a body primitive, head, four simple planar wing meshes, and mirrored left/right hinge controls. Flat grayscale materials are enough. No texture painting is necessary at this resolution.
3. **Animate once.** Key five distinct wing poses. Preview the transition from frame 4 back to frame 0; do not duplicate the first pose as the fifth frame unless the resulting 4→0 timing is intentional.
4. **Fix five cameras.** Use orthographic cameras at 0°, 45°, 90°, 135°, and 180° around the butterfly's vertical axis with identical orthographic scale and target. Do not move the model independently for different views.
5. **Render a lossless intermediate.** Render every camera at a modest integer multiple such as 128 × 128, transparent RGBA PNG, five frames. A single scripted render can emit the 25 images with names that encode `view` and `frame`.
6. **Pixel-finish.** In Pixelorama/Aseprite, reduce each cell to 16 × 16, normalize the pivot/centering, hand-clean the silhouette, and quantize to four opaque grayscale entries plus one fully transparent entry. Avoid partial alpha.
7. **Assemble by meaning.** Rows are views; columns are frame time. The final file is exactly 80 × 80, indexed PNG. Retain an editable source project separately.
8. **Validate before game integration.** See the acceptance gate below, then run the project in hidden muted mode and inspect the log for atlas assertions or shader errors.

The render/assembly step is a good candidate for a small future Blender export script, but writing that script would be implementation and is outside this investigation.

## Acceptance gate for any candidate atlas

- Dimensions are exactly 80 × 80; every cell is 16 × 16 with no padding or gutters.
- Row order is 0°, 45°, 90°, 135°, 180°; column order is the forward five-frame loop.
- At every column, all five views represent the same wing pose—not merely the same character.
- Rows 1 and 3 have a stable body center and apparent scale, including their horizontal mirrors.
- The frame 4 → frame 0 transition reads as a continuous wing beat at 5 fps.
- The PNG is indexed and has exactly five used RGBA values: one alpha-0 entry and four alpha-255 grayscale entries.
- No partially transparent pixel survives quantization; no opaque pixel touches a cell boundary unless intentionally cropped.
- The silhouette remains readable after nearest sampling without mipmaps.
- A hidden muted release run starts without an atlas dimension, color-mode, palette-count, or extraction failure.

## Licensing caveat

Service terms granting commercial use do not establish that a purely generated image is copyrightable in every jurisdiction and do not guarantee non-infringement. Use only source art and references the project owns, avoid named living-artist or franchise imitation, keep prompts/settings/source hashes, and retain meaningful human selection and pixel editing. This report is product research, not legal advice.

## Companion demonstration

The standalone HTML demonstration is intentionally outside the repository and is not committed:

`/home/terence/Documents/Codex/2026-09-06/realtime-voice-chat-3/outputs/butterfly-art-workflow.html`

It embeds the current `butterfly.png` bytes directly, plays all five real rows in synchronized 0.2-second steps, and simulates the actual two-row plus mirror runtime selection. Its route diagrams are explicitly labeled as design illustrations, not generated art or in-game rendering.
