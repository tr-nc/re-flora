# Glass voxel secondary-visibility research

## Scope

This note narrows the three `t1`, `t2`, and `t3` regressions in the dedicated Glass
scene. It keeps the existing rasterization plus compute/software-voxel-RT architecture:
no hardware ray tracing, acceleration structures, ray queries, new voxel type, save schema,
or experiment statistics are proposed.

## Local evidence

- `shader/slang/scene_query.slang::walkVoxelMediaToNextEvent` is the authoritative dense
  atlas traversal for Glass. Its current relative tie tolerance is `1e-5`, while the CPU
  reference in `src/scene_query.rs` uses double precision and `1e-10`. At the `t2` camera,
  the GPU tolerance merges distinct grid-plane crossings into edge/corner events. The
  `glass-front` diagnostic then shows 2,436 non-primary-axis pixels in a fixed interior ROI,
  and those pixels line up with the visible one-voxel color outliers.
- `shader/slang/glass_resolve.slang::sampleGlassTerminalRadiance` only projects the final
  voxel-DDA terminal. It cannot discover raster-only geometry earlier on the refracted
  segment. This explains the missing part of the amber raster sentinel at `t3`.
- The same terminal-only lookup requires one sampled depth to match the projected world
  endpoint. At `t1`, only 775 of 10,824 visible Glass pixels report a valid screen hit;
  rejected endpoints fall back to simplified voxel shading and make the distant pane look
  gray or black.

### R1/R2 distance audit

The later `r1` and `r2` snapshots exposed a second depth-unit error. Glass resolve used one
fixed `0.003` NDC tolerance for three different jobs: foreground ordering, Glass-front versus
terminal ordering, and screen-ray intersection thickness. Perspective depth is hyperbolic, so
that fixed interval represents an increasingly large world-space slab with camera distance.

- At `r1`, the raster-only amber sentinel is physically in front of Glass, but all 177,757
  Glass pixels reported zero foreground pixels. The oversized NDC tolerance classified the
  sentinel and Glass front as the same depth and allowed cached Glass radiance to overwrite it.
- At `r2`, 28,127 of 88,461 Glass pixels reported rejected depth. Their valid secondary color
  fell back to simplified voxel/sky shading, producing the reported gray appearance.
- Raising the projected segment cap from 256 to 1,024 left the `r2` counters unchanged, so
  projected traversal distance was not causal. Raising the scene-query cap from 8 to 16 did
  not change the depth rejections and converted bounded residual paths into 3,325 interface
  exhaustion pixels. Neither larger budget is a valid fix.

## Primary-source findings

1. Amanatides and Woo traverse a uniform grid by repeatedly selecting the smallest next
   grid-plane distance and advancing the corresponding axis. A tolerance that classifies
   visibly separated crossings as ties changes which cell is entered, so tie detection
   should be limited to floating-point equality uncertainty rather than a world-scale
   geometric band. Source: John Amanatides and Andrew Woo, [A Fast Voxel Traversal Algorithm
   for Ray Tracing](https://www.researchgate.net/publication/2611491_A_Fast_Voxel_Traversal_Algorithm_for_Ray_Tracing),
   Eurographics 1987 (author-uploaded full text).
2. McGuire and Mara show that uniform 3D steps both oversample and skip screen pixels. Their
   screen-space ray tracer instead walks the projected line with a perspective-correct 2D
   DDA and intersects the ray-depth interval covered in each pixel with the scene depth.
   Unit pixel stride is deterministic and contiguous; jitter is only suggested when using a
   larger stride to hide banding. Source: Morgan McGuire and Michael Mara,
   [Efficient GPU Screen-Space Ray Tracing](https://jcgt.org/published/0003/04/04/),
   JCGT 3(4), 2014, including the authors' GLSL implementation. The reference code compares
   ray intervals and scene samples in camera-space Z and expresses `csZThickness` in that
   linear space; it does not use a fixed NDC-depth thickness.
3. AMD's production SSSR documentation uses a depth hierarchy to accelerate the same class
   of depth-buffer intersection and calls out confidence-based hit validation. That is a
   useful later optimization, but it requires a depth pyramid and multiple new lifecycle
   stages. Source: AMD GPUOpen, [FidelityFX Stochastic Screen Space Reflections
   1.5](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/stochastic-screen-space-reflections/).
4. A single camera depth layer is not complete secondary visibility. Layered distance maps
   can represent occluded surfaces but require extra scene captures/layers. The current
   Glass experiment should therefore keep an explicit voxel/sky fallback for off-screen or
   unrepresented raster geometry instead of claiming complete visibility. Source: NVIDIA
   GPU Gems 3, [Chapter 17: Robust Multiple Specular Reflections and
   Refractions](https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-17-robust-multiple-specular-reflections-and-refractions).

## Chosen design

1. Replace the broad GPU DDA tie band with a small ULP-bounded comparison. Preserve
   multi-axis advancement for true edge/corner crossings and keep the existing stable
   axis-aligned normal contract.
2. Replace terminal-only screen reuse with a deterministic, bounded projected-segment walk.
   Visit screen pixels in path order at unit stride, compare each pixel's ray-depth interval
   against the unified opaque depth, and accept the first raster intersection. Validate the
   final voxel terminal with the final interval rather than an unrelated fixed world-depth
   tolerance.
3. Run this once per cached visible Glass voxel, retaining one normal and one final transport
   color per voxel. Do not add random jitter, temporal accumulation, a Hi-Z resource, or a
   second raster lifecycle for these acceptance cases.
4. Keep explicit off-screen, missing-layer, query-budget, and voxel/sky fallbacks. Measure
   the new traversal in Release mode before considering a hierarchy or larger step budget.
5. Treat front/back ordering as topology: require a strict ULP-bounded ordering between the
   two R32F depths. Express only screen-ray intersection thickness in linear camera-space
   world units. This prevents the tolerance from changing meaning with camera distance and
   avoids adding traversal work.

## Acceptance and performance gates

- `t1`: the distant pane must not collapse to the simplified gray/black voxel fallback.
- `t2`: the fixed Glass-front ROI must contain no false near-tie axes, and the final image
  must not contain isolated one-voxel transport outliers.
- `t3`: the amber raster sentinel must remain continuous through the Glass region.
- All three views must remain deterministic across repeated captures, with zero non-finite
  pixels and zero path exhaustion.
- The fixed Release Glass workload and feature-OFF Release gate remain authoritative. A
  correctness change that causes a material regression must be optimized or rejected.
