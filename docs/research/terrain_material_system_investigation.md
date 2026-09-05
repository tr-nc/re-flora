# Re: Flora terrain material system investigation

Date: 2026-09-06

Investigated revision: `1b996946` (`agent/terrain-material-research`)

Scope: investigation and design only. This document does not authorize or implement a material-system refactor.

Post-investigation update: the fertilizer tool, fertility state, fertilizer-granule shading, and their dedicated GPU path were removed on 2026-09-06. Bits 6-7 of the atlas byte are reserved again. References below to the removed path describe the investigated revision only; the forward material proposal retains moisture but has no fertility control.

## Recommendation

Re: Flora should keep the existing four-bit voxel type as the world and save-file truth, but replace the presentation-only color palette with an authored material catalog and small mipmapped 2D texture arrays. A ray hit already provides the material ID, world-space hit position, and surface normal, so the first textured path needs no UVs and no extra bytes per voxel.

The suggested first visible milestone is deliberately narrower than a general-purpose engine material graph:

1. Permanently remove the base-color dependency on the current two-bit per-surface hash.
2. Add a validated `VoxelMaterialCatalog` whose stable IDs match the existing voxel type IDs.
3. Give each opaque terrain material a tint, world-space tile size, and top/side/bottom albedo layer references.
4. Use dominant-axis world-space projection for one albedo texture fetch at ordinary voxel surfaces. Make blended triplanar projection an opt-in quality mode for surfaces where the smooth stored normal makes hard axis changes objectionable.
5. Let visible primary shading sample detailed textures, but let DDGI and secondary transport use a precomputed average transport albedo from the same catalog. Include the catalog content revision in DDGI radiance identity so material changes invalidate history correctly.
6. Add normal/roughness channels only after the lighting model is ready to consume them and release-mode measurements establish a budget.

Do not add per-voxel blend weights or virtual texturing in the first implementation. Those solve different, more expensive problems that the current request does not establish.

## Current Re: Flora path

### World authority and persistence

The editable world authority is `PlainBuilderResources::chunk_atlas`, a `512 x 512 x 512` `R8_UINT` 3D image (`src/builder/plain/resources.rs:63-74`, using `CHUNK_DIM = 2³` and `VOXEL_DIM_PER_CHUNK = 256³` from `src/app/core/mod.rs:883-884`). Its one byte is fully occupied:

- bits 0-3: voxel type/material identifier;
- bits 4-5: moisture;
- bits 6-7: reserved.

The layout is defined in `shader/slang/voxel_data.slang:5-16` and mirrored by packing helpers in `src/builder/plain/mod.rs:97-136`. Terrain snapshot schema 1 persists exactly that one-byte atlas payload, including all types and soil state (`src/terrain_persistence.rs:7-13`; `docs/terrain_persistence_v1.md:145-171`). The full world payload is 128 MiB before derived structures.

Consequence: the current low nibble supports at most 16 stable material IDs, and there is no spare per-voxel storage for another texture index or blend weight. Keeping that contract makes a first texture migration save-compatible.

### Surface extraction and ray-hit data

Visible terrain is not a conventional UV-mapped mesh. The synchronous rebuild path is:

```text
R8 chunk_atlas
  -> SurfaceBuilder R32 surface scratch
  -> compact Contree leaf data
  -> scene_tex chunk indirection
  -> compute ray marcher
```

`extractSurfaceVoxel` reads the voxel type, computes a smooth occupancy-derived normal, generates a two-bit hash bucket, and packs those into one 32-bit surface value (`shader/slang/surface_extraction.slang:201-249`, `shader/slang/voxel_data.slang:76-97`). `contree_leaf_write.slang:111-165` copies that value into compact leaf storage. `scene_marching.slang:25-40` exposes the hit position, voxel-center position, stored normal, voxel type, and hash bucket through `MarchingResult`.

That hit payload is already sufficient for world-space projection:

```text
material = catalog[result.voxel_type]
projection coordinates = result.position * material.tiles_per_world_unit
projection weights/face = abs(result.normal)
```

No mesh UV generation, chunk-local UV seam repair, or second volumetric material channel is needed for the first version.

### Exact hash-color path to retire

The visual hash is deterministic and position-based, but it is not authored texture detail:

1. `voxelHashBucketFromBlueNoise` hashes integer world voxel coordinates to one of four buckets (`shader/slang/surface_extraction.slang:56-64`). Despite its name, it does not sample the repository's blue-noise textures.
2. The bucket is stored in bits 30-31 of each derived Contree leaf (`shader/slang/voxel_data.slang:86-97`).
3. `voxelColorByComponentsAndHash` converts the chosen base color to HSV and applies one of four fixed hue/saturation/value offsets (`shader/slang/tracer_material.slang:21-46`). Dirt and sand receive full strength, rock 0.6, stucco 0.15, and wood/emissive no hash variation.
4. On the investigated revision, the primary tracer called that function before moisture and fertility overlays (`shader/slang/tracer.slang:208-216`). DDGI repeated the same base hash color in probe ray radiance (`shader/slang/ddgi_probe_trace.slang:191-203`). The Glass fallback had a second coordinate-hash implementation (`shader/slang/glass_resolve.slang:283-294`).
5. The shared strength originates at the Debug GUI's `voxel_color_variance`, crosses `MaterialFrameInput`, is uploaded to `U_VoxelColors`, and is also frozen into `DdgiRadianceSnapshot` identity (`config/gui.toml:2594-2603`, `src/app/core/render_frame_input.rs:52-70`, `src/tracer/buffer_updater.rs:204-226`, `src/environment_lighting.rs:46-107`).

The checked-in GUI default is already `0`, so this exact tint is dormant in a default checkout. The producer, storage bits, controls, main-view consumer, Glass fallback, DDGI consumer, and history identity nevertheless remain live architecture and can be re-enabled at runtime. A future removal should delete the presentation dependency end-to-end; it does not need to reclaim the Contree bits in the same commit.

Two other hashes are separate behaviors and should not be accidentally removed with it:

- `shader/slang/chunk_init.slang:21-57` uses a hash to choose dirt versus rock across the procedural subsurface transition. This changes voxel type distribution, not shading variation.
- The investigated revision also had position-seeded fertilizer granules. That feature and its state were subsequently removed rather than carried into the material proposal.

### The current “material” is split across concerns

`src/voxel_material.rs` and `shader/slang/voxel_material.slang` already define collision, water solidity, terrain support, DDGI visibility, soil eligibility, shadow policy, and the Glass experiment's optical parameters. They do not define textured appearance. Opaque appearance is a small per-frame uniform of five colors plus one global hash strength (`shader/slang/tracer_types.slang:167-177`). Stucco and emissive values are compiled constants.

The main opaque lighting model currently multiplies albedo by direct and indirect irradiance (`shader/slang/tracer.slang:219-243`). It has no terrain roughness, metallic, or texture-normal input. A “PBR material” schema added today would therefore promise controls that the renderer cannot yet display.

There is also an existing consistency boundary: the primary path reads moisture from `chunk_atlas`, while DDGI transport currently uses only base palette/hash albedo. A new catalog should make the visible-versus-transport approximation explicit rather than silently widening this mismatch.

### Texture infrastructure constraints found on this revision

Re: Flora already creates sampled 2D arrays for blue noise and particle sprites (`src/tracer/resources.rs:1507-1542` and `:1674-1698`), so array layers and reflected descriptors are proven local patterns. However:

- `ImageDesc` has no mip-count field (`crates/re-flora-vkn/src/memory/texture/desc.rs:4-30`).
- `Image::new` hardcodes `mip_levels(1)` (`crates/re-flora-vkn/src/memory/texture/image.rs:63-83`).
- the default sampler is nearest/clamp with `max_lod = 0.25` and is explicitly marked “no mipmaps” (`crates/re-flora-vkn/src/memory/texture/desc.rs:358-395`).

World-space terrain textures without mipmaps will alias and shimmer in the compute tracer. Mip allocation, views, upload regions, transitions, and wrap/linear sampling are an infrastructure prerequisite, not optional polish. The compute shader also cannot rely on fragment-stage implicit gradients; it should use an explicit LOD derived from ray footprint/distance. Slang exposes texture types and mipmapping, while `SampleLevel` selects an explicit (including fractional) mip level ([Slang texture types](https://docs.shader-slang.org/en/latest/external/slang/docs/user-guide/02-conventional-features.html), [HLSL `SampleLevel`](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/samplelevel-s-float-float-int-uint-)).

## Existing voxel material systems

These are reference designs, not drop-in dependencies for the Rust/Vulkan renderer.

| System / technique | What its owner documents | Lesson for Re: Flora | Why not copy it whole |
| --- | --- | --- | --- |
| Minecraft Bedrock material instances | A block maps named or directional faces to texture references and render methods. Material instances can distinguish up/down/cardinal faces; the current component is limited to 64 instances ([Microsoft block material instances](https://learn.microsoft.com/en-us/minecraft/creator/reference/content/blockreference/examples/blockcomponents/minecraftblock_material_instances?view=minecraft-bedrock-stable)). Weighted position-stable texture variations are also available, currently documented with up to 256 variants ([Microsoft variations textures](https://learn.microsoft.com/en-us/minecraft/creator/documents/variationsblocktexture?view=minecraft-bedrock-stable)). | Stable semantic block ID plus authored top/side/bottom textures is a proven, legible authoring model. | It assumes block geometry and Minecraft's asset/runtime pipeline. Re: Flora should not reproduce its atlas format or position-selected variant arrays merely to replace a disliked hash. |
| Godot Voxel Tools, blocky terrain | Voxel `TYPE` indexes a reusable model library; cube models select atlas tiles per face; material reuse reduces draw calls ([official blocky-terrain docs](https://voxel-tools.readthedocs.io/en/latest/blocky_terrain/)). | Separate voxel identity from the resource that maps identity/face to appearance. | It builds chunk meshes and UVs. Re: Flora ray-marches compact surface voxels and can project at the hit instead. |
| Godot Voxel Tools, smooth terrain | Smooth surfaces lack ordinary UVs, so the documented path is world/object triplanar mapping. Simple slope/height rules can stay in the shader with no voxel data. If texture is voxel-authored, “Single” stores one 8-bit index; “Mixel4” stores four 4-bit indices and four 4-bit weights across two 16-bit channels ([official smooth-terrain docs](https://voxel-tools.readthedocs.io/en/latest/smooth_terrain/)). | World-space projection and shader-only auto-material rules avoid a volumetric storage tax. Per-voxel weights are a separately priced feature. | Re: Flora already has its material ID. Copying Single would duplicate it; Mixel4 would add four bytes/voxel and a much wider edit/persistence contract. |
| Voxel Plugin 2 Surface Types / MegaMaterial | Surface Type assets refer to ordinary Unreal materials; stamps and generation carry the surface data; a MegaMaterial tracks and combines used materials. Voxel worlds have no mesh UVs, and the documented norm is world coordinates/triplanar. Smart Surface Types apply slope/height-style rules ([workflow](https://docs.voxelplugin.com/knowledgebase/materials/working-with-materials/), [authoring](https://docs.voxelplugin.com/knowledgebase/materials/working-with-materials/authoring-materials), [smart surface types](https://docs.voxelplugin.com/knowledgebase/materials/working-with-materials/smart-surface-types)). Its smooth blend uses one material per pixel plus dithering and depends on TAA/TSR to hide speckle ([blend docs](https://docs.voxelplugin.com/knowledgebase/materials/working-with-materials/smooth-alpha-blends)). | The useful ownership split is semantic surface asset → world assignment → derived GPU composition. Branch only into the material actually needed at a hit. | It is a commercial Unreal-specific compiler/cache and its docs warn of texture/sampler limits; bindless does not remove all limits. Re: Flora does not need an unbounded material graph or TAA-dependent dither blend. |
| Texture arrays | Each array image has the same dimensions and mip-count, layers are sampled independently, and unlike a hand-packed atlas each layer gets an independent mip chain with no cross-tile padding problem ([Godot `Texture2DArray`](https://docs.godotengine.org/en/stable/classes/class_texture2darray.html), [Vulkan `VkImageCreateInfo`](https://docs.vulkan.org/refpages/latest/refpages/source/VkImageCreateInfo.html)). | A few fixed arrays are the simplest bounded resource set for 16 schema-compatible IDs. Validate dimensions/formats/mips at import. | Arrays do not solve unique world-scale painting, and all layers must share format/resolution/mip shape. |
| Runtime/streaming virtual texturing | Unreal's RVT generates and caches GPU shading tiles on demand; SVT streams prebuilt disk data. They target large textures, composited layers, decals, and landscape-scale shading ([Epic overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-texturing-in-unreal-engine), [RVT](https://dev.epicgames.com/documentation/unreal-engine/runtime-virtual-texturing-in-unreal-engine), [settings and pool trade-offs](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-texturing-settings-and-properties-in-unreal-engine)). Vulkan sparse images allow rectangular blocks to be independently resident, but require explicit features and application-managed residency ([Khronos sparse-residency spec](https://docs.vulkan.org/spec/latest/chapters/sparsemem.html), [Khronos sparse-image sample](https://docs.vulkan.org/samples/latest/samples/extensions/sparse_image/README.html)). | A later system could cache roads, footprints, decals, or unique macro color over a large garden. | It adds page tables, feedback, physical pools, upload/eviction scheduling, synchronization, capability checks, fallbacks, and failure modes. It is not justified for a small repeatable texture library. |

Godot Voxel Tools is MIT licensed ([official license](https://github.com/Zylann/godot_voxel/blob/master/LICENSE.md)). Voxel Plugin 2 is useful here as a design reference, not a purchase recommendation; its current official licensing page lists a USD 349 perpetual license but should be rechecked with the vendor before any procurement decision ([official licensing](https://docs.voxelplugin.com/resources/licensing)).

## Material ownership proposal

The design should preserve one meaning for each stable voxel ID while keeping world state, authored inputs, and derived GPU state distinct:

```text
authoritative voxel state
  chunk_atlas: [type_id:4 | moisture:2 | reserved:2]
                    |
                    v
authoritative authored material catalog
  id/name + simulation/optical policy + appearance references/parameters
                    |
          validate + content fingerprint
                    |
          +---------+------------------+
          v                            v
derived visible resources          derived transport snapshot
  material GPU table                 average albedo/emission
  albedo texture array + mips         catalog revision in DDGI identity
          |                            |
          v                            v
primary ray-hit shading            DDGI / secondary bounce shading
```

One catalog should own the association between stable type ID and its material definition. The catalog should not own voxel placement, chunk revisions, or DDGI lifecycle. A focused render-resource owner should validate/decode/upload images and publish an immutable GPU generation. DDGI should observe only an immutable catalog revision plus the compact transport values it needs.

A conceptual authored definition could contain:

```toml
# Design sketch only; not a committed format.
id = 2
name = "dirt"

[semantics]
surface_class = "opaque"
collision_solid = true
water_solid = true
terrain_support = true
soil_state_allowed = true

[appearance]
projection = "dominant_axis"
meters_per_tile = 0.125
tint_srgb = "#8B7258"
albedo_top = "dirt_top"
albedo_side = "dirt_side"
albedo_bottom = "dirt_bottom"
transport_albedo_linear = [0.22, 0.17, 0.12]
```

The exact file format is undecided. The important contract is that startup fails clearly on duplicate/out-of-range IDs, missing layers, inconsistent dimensions/mips/formats, invalid scales, or a catalog type that lacks the semantic data existing CPU consumers require.

## Mapping, quality, and cost

### Projection modes

**Dominant-axis world projection (recommended default).** Select X/Y/Z from the largest absolute normal component, select top versus bottom by sign for Y, and sample one layer at the other two world coordinates. It gives stable scale, no chunk seams, sharp voxel character, top/side/bottom art control, and one sample per texture channel. A small hysteresis or authored blend band can address popping around equal components if measurements and captures show it.

**Full triplanar (quality option).** Sample all three projections and blend by normalized `abs(normal)^sharpness`. This is the standard response when smooth voxel surfaces have no UVs ([Godot triplanar documentation](https://docs.godotengine.org/en/stable/tutorials/3d/standard_material_3d.html#triplanar-mapping), [Voxel Tools smooth terrain](https://voxel-tools.readthedocs.io/en/latest/smooth_terrain/#triplanar-mapping)). It softens axis transitions but triples texture reads and can blur pixel-styled textures.

**Per-face block UV/atlas.** This is excellent for conventional cube meshes and directional block art, but an atlas adds padding and mip bleed management. A texture array gives Re: Flora the same face choice without generating UVs or atlas rectangles.

### Sample budget

The following is a static operation count, not performance evidence:

| Visible mode | Albedo reads | Albedo + normal + ORM reads |
| --- | ---: | ---: |
| Current flat color | 0 | not supported |
| Dominant-axis | 1 | 3 |
| Full triplanar | 3 | 9 |
| Two materials, full triplanar blend | 6 | 18 |

Voxel Tools' own example uses three axis reads per texture map; the multiplied figures above are direct arithmetic from that algorithm, not a measured Re: Flora cost. Since the tracer is full-screen compute and the same functions may be called for path-tracing bounces, primary hits, Glass fallback, and DDGI rays, release-mode GPU timings and sample-count instrumentation must decide the final quality tier.

### Texture memory illustration

With 16 layers and a full mip chain:

- 256² RGBA8 albedo array: about 5.33 MiB;
- 256² RG8 normal array: about 2.67 MiB;
- 256² RGBA8 ORM array: about 5.33 MiB;
- total uncompressed: about 13.33 MiB.

At 512² the same three arrays are about 53.33 MiB. These are calculated allocation sizes (including the approximately 4/3 mip-chain factor), not measured driver memory. Block compression can reduce them, but compressed upload, mip generation, and target-GPU format support are separate implementation work.

### Visible versus transport evaluation

Two materially different policies are possible:

1. **Exact texture evaluation in every ray.** DDGI and path-traced secondary hits sample the same detailed albedo as the camera. This is the closest visual/transport match but multiplies texture traffic across many non-primary rays.
2. **Preintegrated transport material (recommended first).** The visible hit samples the texture; DDGI and secondary bounces use a catalog-provided or offline-computed average linear albedo/emission. The same catalog revision still invalidates DDGI when appearance changes. This intentionally filters detail that probes cannot resolve while bounding texture traffic.

This should be an explicit policy and test fixture. It should not emerge accidentally from which descriptor happened to be convenient to bind.

## Staged migration

### Stage 0 — Baseline and remove the unwanted presentation hash

- Capture fixed-camera screenshots and release-mode GPU/CPU timing at `voxel_color_variance = 0` and at a representative nonzero value.
- Remove `hash_color_variance` and base-color hash calls from primary, DDGI, and Glass fallback paths; remove the GUI/material-frame/DDGI-identity field.
- Leave the two derived Contree hash bits temporarily unused unless a separate surface ABI change has value.
- Preserve procedural dirt/rock distribution and moisture behavior.

Acceptance: fixed material color no longer changes by per-voxel hash in any renderer/transport path; moisture behavior is unchanged.

### Stage 1 — Mip-capable bounded material resources, no visual change

- Extend the Vulkan texture wrapper for explicit mip counts, per-mip upload/transition/view ranges, wrap/linear sampling, and array-layer validation.
- Add a catalog validator and immutable content fingerprint.
- Upload a small same-size albedo texture array and a compact per-ID GPU table, while still rendering the old flat colors.

Acceptance: invalid assets fail closed; mip/layer readback or a narrow shader fixture proves addressing; no generated or persisted voxel schema changes.

### Stage 2 — One-fetch textured albedo

- Replace primary-hit flat color with dominant-axis world-space albedo sampling.
- Retain moisture as a post-albedo transform initially, and use preintegrated catalog albedo for DDGI/secondary transport.
- Record an explicit analytic LOD and visualize LOD bands in a diagnostic mode.

Acceptance: no chunk/world-position seams, stable scale after edits/rebuilds, no obvious distance shimmer, material revision invalidates DDGI, and release-mode budget is recorded.

### Stage 3 — Authored automatic surface rules

- Add bounded, data-defined top/side/bottom and optional slope/height rules that derive solely from hit position/normal and material parameters.
- Keep these presentation rules out of voxel persistence unless gameplay must query them.

Acceptance: the same world/save can change art direction by swapping a catalog, with deterministic rendering and no edit/rebuild expansion.

### Stage 4 — Optional richer shading

- Add full triplanar only for materials/surfaces that need it.
- Add normal maps only with correct per-axis world-space normal reconstruction and a measured lighting integration.
- Add roughness/metalness only alongside a deliberate opaque BRDF change; the current diffuse-only model cannot display those controls honestly.

Acceptance: capture comparisons establish visual value; release-mode sampling cost fits the target GPU budget; DDGI approximation remains documented.

### Stage 5 — Painted blends only if a real use case requires them

If the player must paint persistent soft boundaries, first evaluate a bounded two-material control field or lower-resolution per-chunk surface control map. A direct Mixel4-like layout would add four bytes per voxel—512 MiB for the current 512³ world—before derived/render copies, and would require snapshot schema, edit tools, uploads, rebuilds, and migrations to change together.

### Stage 6 — Virtual texturing only after measurements

Consider virtual/sparse textures only if unique macro detail or the physical texture set exceeds a measured memory/streaming budget. Require capability detection, fallback, residency/eviction telemetry, stale-update rules after terrain edits, and release-mode proof. Do not make this a prerequisite for repeatable dirt, sand, rock, and wood materials.

## Decisions for the user

Recommended defaults are listed first.

1. **Visual character:** crisp voxel surfaces with dominant-axis projection (recommended), or softer organic surfaces with full triplanar blending?
2. **Material scope:** textured albedo first (recommended), or commit now to a new specular/PBR lighting model so normal and ORM maps have meaning?
3. **Transport policy:** detailed camera texture plus average DDGI/secondary albedo (recommended), or exact texture sampling in every transport ray at higher cost?
4. **Stable capacity:** retain schema-1's 16 IDs and one byte per voxel (recommended), or accept a save-format/world-memory migration for more materials or blend weights?
5. **Art controls:** should dirt/rock/sand expose distinct top, side, and bottom layers (recommended), or share one layer with rotation/scale only?
6. **Initial texture scale:** choose the authored physical target after viewing real assets. A useful prototype comparison is 16, 32, and 64 texels per voxel-sized tile, but the final value should follow the chosen pixel-art or hand-painted style rather than be hardcoded by engineering.

## What the HTML demo proves—and does not

The companion HTML is a disposable design visualization. It demonstrates control grouping, dominant-axis versus triplanar intent, texture scale, moisture, ownership, and migration choices using generated browser canvas patterns. It does not use Re: Flora shaders, camera, lighting, textures, Vulkan sampling, or performance data. Visual approval of the HTML therefore approves a direction and vocabulary, not the final in-game look.

## Source quality and inference boundary

All market comparisons above use first-party engine/plugin documentation, owner-maintained source, Microsoft creator documentation, or Khronos specifications/samples, checked on 2026-09-06. Statements about which path best fits Re: Flora, the sample-count arithmetic, memory arithmetic, and the staged recommendation are inferences from those sources plus the inspected Re: Flora revision. They are not claims made by the referenced vendors.
