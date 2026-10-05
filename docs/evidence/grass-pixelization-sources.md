# Grass pixelization: primary-source research ledger

This ledger accompanies [the HTML investigation](grass-pixelization-performance.html).
It separates published rendering/production accounts from our own Release timings.
**None of these game sources supplies a controlled GPU benchmark of re-flora's per-plant model-grid atlas.**
Search-provider summaries were discovery aids, not evidence. Passages below were checked in fetched page bodies.

## Released-game accounts

### S1 — Dead Cells: offline 3D animation → 2D frames

- **Owner/author:** Thomas Vasseur, Motion Twin artist; developer-authored Game Developer article.
- **Source:** [Art Design Deep Dive: Using a 3D pipeline for 2D animation in Dead Cells](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-).
- **Checked passage:** “We export each frame of the animation we made with the 3D skeleton to a .png” (section describing animation export).
- The article describes tiny, non-antialiased mesh renders, exported animation frames and normal maps, and a basic toon shader. The motivation is fast authoring/animation iteration with a small art team.
- **Supports:** 3D tools can produce pixel-art sprites offline; production efficiency and runtime rendering efficiency are different questions.
- **Does not support:** runtime mesh-to-model-grid rendering being faster than continuous meshes, or a measured GPU speedup for pixelization.

### S2 — A Short Hike: whole-scene low-resolution rendering

- **Owner/author:** Adam Robinson-Yu / `@adamgryu`, the game's creator.
- **First-party account:** [Crafting a tiny open world](https://blog.playstation.com/?p=351732). The art-direction discussion explicitly covers flat shading, no anti-aliasing, outlines, adjustable pixel size, and 4K display on PS4 Pro/PS5.
- **Technical account:** [Original developer thread](https://twitter.com/adamgryu/status/1113100182655262721), available as a [Thread Reader mirror](https://threadreaderapp.com/thread/1113100182655262721.html).
- **Checked passage in the mirror's main thread, tweet 5:** “The pixellated look is achieved by rendering the world to a low-res RenderTexture.”
- **Retrieval caveat:** readable extraction selected unrelated recommended threads and omitted the relevant main-thread text. Raw HTML verified `article:author=https://twitter.com/adamgryu`, the original thread link, and the exact main-thread passage. The mirror is a reproduction of a developer statement, not a new independent primary author.
- **Supports:** an intentionally low-resolution whole-scene target, not a separate model-local tile atlas for every grass plant.
- **Does not support:** a published smooth-versus-pixelized GPU timing or guaranteed speedup at every pixel-size setting.

### S3 — Return of the Obra Dinn: grayscale 3D + 1-bit conversion

- **Owner/author:** Lucas Pope / `dukope`, the game's creator.
- **Sources:** [Developer's November 2017 devlog archive](https://dukope.com/devlogs/obra-dinn/tig-32/), especially “Fullscreen, Round 3”; [developer-authored PlayStation retrospective](https://blog.playstation.com/archive/2019/10/17/lucas-pope-on-return-of-the-obra-dinns-art-style/).
- **Checked passage:** “Obra Dinn renders everything internally in 8-bit grayscale then converts the final output to 1-bit in a post-processing pass.”
- The retrospective describes the shipped 800×450 rendering and work to stabilize dithering relative to camera motion. The devlog describes screen-offset and sphere-mapped dithering compromises.
- **Supports:** a low-resolution 3D scene plus a deliberately designed conversion/stabilization stage; a 1-bit appearance does not mean all intermediate buffers or calculations are 1-bit.
- **Does not support:** re-flora's object-grid/depth semantics, or a matched runtime benchmark proving dithering free/cheaper. No invented milliseconds or FPS are assigned to this game.

### S4 — Prodeus: modern rendering under a retro aesthetic

- **Owner:** developer/publisher-authored [Steam description](https://store.steampowered.com/app/964800/Prodeus/); `Prodeus`, explicitly marked `[developer]`, in [a support thread](https://steamcommunity.com/app/964800/discussions/4/3200369112092215164/?l=english#c3200369647695191547).
- **Checked support passage:** “disable ssr and ssao. Lower the resolution” (comment `3200369647695191547`, 27 December 2021).
- **Retrieval caveat:** readable extraction omitted replies. Raw HTML verified the developer badge, author/profile, comment ID and passage; a player's quoted repetition was not substituted for the developer's own reply.
- **Supports:** retro appearance can coexist with modern 3D/effect costs; the developer directs performance troubleshooting to particular effects and resolution.
- **Limits:** this advice predates the 1.0 release, not a controlled benchmark of that release. Steam's [first-party appdetails endpoint](https://store.steampowered.com/api/appdetails?appids=964800&filters=release_date) was checked and returns `coming_soon=false`, `date=23 Sep, 2022`. The store's marketing description is not proof of any particular pixelizer implementation. We do not infer its exact shader/atlas pipeline.

## Engine, hardware and API guidance

### S5 — Unity Pixel Perfect Camera

- **Owner:** Unity manual, [Pixel Perfect Camera component reference](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/2d-pixelperfect-ref.html), “Grid Snapping → Upscale Render Texture”.
- The documented option renders to a temporary texture matched to the reference resolution/aspect and upscales to the viewport.
- **Supports:** distinguish pixel snapping from rendering at reduced resolution. It is a 2D camera reference, not direct evidence about our 3D model-grid backend.

### S6 — Unity render scale and intermediate-target costs

- **Owner:** Unity manual, [URP asset reference](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/universalrp-asset.html), “Quality → Render Scale” and “Store Actions”; [Configure for better performance](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/configure-for-better-performance.html).
- Render Scale changes render-target resolution while UI remains native-resolution. Store/intermediate-texture settings, CPU work, GPU work and memory usage are separately discussed.
- **Supports:** fewer target pixels can save resolution-dependent work; extra intermediate targets/passes have costs. The manual specifically highlights mobile/tile-based bandwidth for store actions; that is not a measurement of our NVIDIA desktop GPU.
- **Does not support:** reducing model-grid resolution automatically reducing every CPU/vertex/lighting cost, or an inverse-resolution formula for whole-frame time.

### S7 — NVIDIA bottleneck diagnosis

- **Owner:** NVIDIA, [System Architecture Guide](https://docs.nvidia.com/nsight-graphics/UserGuide/gpu-trace-system-architecture.html), “Units Throughput”, “SM Throughput”, “L1 Throughput”; [What is Limiting Your Rendering Performance?](https://developer.nvidia.com/blog/what-is-limiting-your-rendering-performance/).
- GPU Trace distinguishes Vulkan/D3D12 hardware-unit activity, throughput and memory/shader limitations.
- **Supports:** locate the limiting unit with counters before claiming ALU, texture, ROP or VRAM bandwidth is the bottleneck.
- **Our limit:** this investigation uses timestamp scopes and CPU timings, not Nsight hardware counters. Atlas bandwidth is a plausible contributor, not a demonstrated bandwidth-bound diagnosis.

### S8 — AMD data-export/attachment guidance

- **Owner:** AMD GPUOpen, [RDNA Performance Guide](https://gpuopen.com/learn/rdna-performance-guide/), “Resource creation”, “Pixel shaders”, “Clears”, “Variable rate shading”.
- **Checked passage:** “Minimize the total amount of data written by a pixel shader to save memory bandwidth.”
- The guide also discusses required resource flags, fast-clear conditions and depth export's interaction with VRS.
- **Supports:** investigate wide attachments and redundant exported data; correctness and format precision still need verification.
- **Limits:** RDNA guidance is not NVIDIA counter evidence. Its warning about `RGB32` must not be misreported as a direct statement about our `RGBA32F` attachment. Its VRS depth-export note does not establish our early-Z behavior.

### S9 — Depth-output semantics

- **Owner:** Microsoft, [HLSL semantics](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-semantics), system-value table (`SV_Depth`, `SV_DepthGreaterEqual`, `SV_DepthLessEqual`).
- Conservative depth variants require a guaranteed relation to rasterizer depth. They are not a switch to apply blindly to sampled model depth.
- **Supports:** early-depth optimization is a separate investigation requiring a proved bound and actual Vulkan/SPIR-V/hardware validation.
- **Does not support:** a measured cause of our current regression.

### S10 — Vulkan acquire/layout ordering

- **Owner:** Khronos, [Synchronization Examples](https://docs.vulkan.org/guide/latest/synchronization_examples.html), “Interactions with semaphores” and “Swapchain Image Acquire and Present”.
- The guide states that a barrier's source stages must be equal to or logically later than the relevant semaphore-wait stages for the barrier to be ordered after that wait. A layout transition itself is a write operation.
- **Supports:** the actual acquire-stage/transition-stage fix, not suppression of synchronization validation or a global idle workaround.
- Additional checked source: [Swapchain Semaphore Reuse](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html). This was diagnostic background; the reproduced hazard was fixed by acquire/transition and attachment scopes, not by claiming a semaphore-reuse bug.

## Evidence boundary

The game accounts describe several **different** strategies: offline sprites, low-resolution scene rendering, final-image conversion, and modern effects with retro styling. They do not demonstrate that model-local pixelization is intrinsically cheap or expensive.
The HTML report's re-flora numbers come only from recorded same-binary Release app runs. Any proposed optimization is identified as unimplemented until measured and validated.
