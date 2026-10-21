# Watered grass: inexpensive wet-material response

Research handoff for garden delivery **scope v1**, wetness. Repository inspected at
`a66d9c5144b4a687a4e0c3c7cfa5f8ae7b6cc527`; research only, no production edits or runtime evidence.

## Recommendation

Start with modest **linear-albedo darkening** of tall and short grass, driven directly by
the two moisture bits of its supporting soil voxel. Reuse the atlas fetch already made by
`floraEnvironmentGrowthFactor`; expose the decoded moisture separately from its growth
factor. Apply the same material helper after growth-stress tint and before lighting in
cached and uncached consumers. Do not derive wetness from growth potential: competition,
Debug growth override and species response curves would otherwise change the apparent water.

This is a **root/soil-moisture proxy**, not measured water on the leaf surface. It will wet
grass after a manual Water brush or subsurface moisture transfer too; it cannot tell whether
spray actually struck a blade. Saturated soil may keep grass looking wet after airborne
spray stops. That is an intentional, inexpensive Garden Moment approximation, not a physical
foliar-water simulation. Do not add a second painted footprint or scan sprinklers to hide
this distinction.

## Primary sources and what they establish

Retrieved directly from the original rendering author's site during this research:

1. Sébastien Lagarde, [Water drop 3a — Physically based wet surfaces](https://seblagarde.wordpress.com/2013/03/19/water-drop-3a-physically-based-wet-surfaces/), 2013.
   The opening physical explanation distinguishes pore filling from a surface water layer;
   repeated absorption lowers diffuse reflectance and may increase saturation. Water can
   smooth the interface and concentrate specular reflection. The article's layered-BRDF
   experiment is **general rendering analysis**, not proof that every material darkens by
   one constant. It explicitly excludes thin translucent materials from that discussion.
   Therefore its rough porous stone examples are not quantitative evidence for waxy grass.
2. Lagarde, [Water drop 3b — Physically based wet surfaces](https://seblagarde.wordpress.com/2013/04/14/water-drop-3b-physically-based-wet-surfaces/), 2013,
   especially the opening parameter-tweaking discussion, material measurements and drying
   section. Interpolating dry/wet BRDF parameters retains the existing lighting model and
   avoids evaluating a full extra water layer. Authoring another full texture set costs
   storage/fetches; modifying existing parameters can avoid those textures. Measurements
   vary by material/channel; universal diffuse/specular multipliers are not physically
   reliable. Specular can disappear sooner than diffuse darkening during drying.
   The author's list of other games is **his report about those games**, not independently
   verified first-party implementation evidence here; do not present that list as such.
3. Lagarde, [Water drop 2b — Dynamic rain and its effects](https://seblagarde.wordpress.com/2013/01/03/water-drop-2b-dynamic-rain-and-its-effects/), 2013,
   wet-surface process and wet/flood-level sections. This is **first-party implemented
   rendering-demo practice**: one scalar wet level modifies diffuse/gloss, remains distinct
   from instantaneous rain intensity, and decreases after rain ends. The article provides
   a RenderMonkey implementation/demo, not evidence that this exact implementation shipped
   on grass in a named game. Puddle accumulation and reflection machinery are separable
   from parameter adjustment and are unnecessary here. Its strong illustrative attenuation
   and gloss multipliers should not be imported as grass calibration.

These are three articles from one authoritative original author, not three independent
replications. They support a cheap material-parameter approach and its limits, not a
verified botanical grass BRDF or a claimed shipped-game grass recipe. No proprietary code,
images, demo assets or shader snippets are copied. Our proposed coefficients below are art
starting points, not sourced constants. A separate official Filament documentation fetch
returned an access/error response; it is not used as evidence.

## Actual repository seams (read-only inspection)

| Owner / file | Existing behavior and implication |
| --- | --- |
| `src/builder/plain/mod.rs` | Atlas byte type mask is low four bits; `VOXEL_MOISTURE_MASK = 0x30`, maximum 3. Do not repurpose reserved bits. Atlas readback APIs exist but should not enter a per-frame vegetation path. |
| `shader/slang/surface_flora_vertex.slang` | `floraEnvironmentGrowthFactor(localBase, chunkWorldOffset, species)` bounds-checks and fetches `chunk_atlas[chunkWorldOffset + localBase]`, decodes moisture and maps it through the species growth curve. This is the authoritative reusable read seam. Add soil/type validation for material wetness and return dry on invalid/non-soil support; retain existing growth fallback semantics. |
| `shader/slang/flora_types.slang` | `Instance` is two uints (8 bytes), position plus growth packed in the first; `surfaceFloraStemWorldPosition` adds one Y voxel above support. Read moisture at **localBase**, not at that elevated stem position. Do not change the aligned instance stride. |
| `shader/slang/flora_vertex.slang` | `sampleFloraBaseColor` reads linear RGB10 height colors, grass band variation; `applyGrassGrowthStressTint` is separate. Shading is stylized diffuse/environment, not a standard roughness-driven material pipeline. Add one shared grass-only material-color helper here (or a small imported module), not a post-lit global tint. |
| `shader/slang/flora_lighting_cache.comp.slang` | Dispatch has one lane per grass layer/instance; already reads support through the environment-growth function. Cached-band variant computes final color in `GrassBandPose.color_valid`. Apply material response here before `shadeFloraVertexWithEnvironment`. Glass/band variants import this source. |
| `shader/slang/grass_band.vert.slang` | Uncached stem-band path independently computes base color/stress/shading; use the same helper here. Do not update only the compute cache and leave runtime alternatives dry. |
| `shader/slang/grass_band_cached.vert.slang`, `grass_band_world.slang`, `stem_band.frag.slang`, `flora.frag.slang` | Cached geometry consumes pose/color, fragments output supplied color. No existing fragment material/roughness slot to turn down. Keep atlas/sprinkler work out of fragments and keep flat layer color stable. |
| `src/tracer/grass_band_cache.rs` | Frame-slot-owned GPU-only 32-byte entries, two float4s, power-of-two allocation. Existing compute writes must stay synchronized with vertex reads. No CPU grass readback needed. |
| `src/tracer/mod.rs` | Owns cache enablement, preparation dispatch and descriptors (notably grass pose reuse and lighting-cache setup around lines 4715 onward). Serialize any descriptor/frame-plan changes with performance work. |
| `src/app/core/moisture.rs`, `shader/slang/terrain_moisture_brush.slang` | Sprinkler contact-source irrigation and manual brush update existing soil moisture. Reuse results rather than recomputing reach/coverage in vegetation. |
| `src/app/core/moisture.rs`, `shader/slang/terrain_moisture_spread.slang`, `terrain_moisture_dry.slang` | Existing spread and drying each budget one chunk/task per frame. Drying uses stochastic per-visit probability, solar and residual modifiers; zero drying speed pauses evaporation, not transfer/watering. Appearance must follow those results, not add independent evaporation. |
| `src/app/core/terrain_persistence.rs` | Save reads each atlas region and writes its byte payload (`run_terrain_save`); garden flora data is captured separately. Derive appearance from restored atlas rather than introducing a new saved wet mask or altering snapshot formats. |

Flower model stems are a separate path (`flower_stem_bands.comp.slang`,
`flower_stem_experiment.slang`, `src/tracer/stem_band_paths.rs`); tree leaves/apples are
separate species too. Initial scope is grass only: do not indiscriminately darken flowers,
fruit, tree canopies or soil by modifying a generic lighting function.

## Smallest implementation and bounded costs

1. Refactor the support-sample seam to provide raw moisture plus the existing growth factor
   from one atlas load; preserve callers' growth behavior. Grass wet weight initially
   `w = moisture / 3`, clamped, with invalid/non-soil support dry. Keep Debug growth override
   independent. No extra persistent state, allocation, texture, CPU scan or dispatch.
2. Apply `dryColor * lerp(1, wetScale, w)` in linear space, after stress tint. Begin near
   `wetScale = 0.75` for review; try roughly 0.65–0.85, not the article's extreme demo values.
   Start neutral darkening, retaining authored greens/height variation. Only add modest
   saturation if real screenshots need it; excessive saturation can look like healthier
   grass rather than recently watered grass.
3. **Sampling frequency:** use the existing once-per-layer preparation, at most 8 layers for
   tall grass and 4 for short, per prepared instance per frame. This is a bounded GPU read
   pattern independent of pixel coverage and sprinkler count, and the fetch already exists.
   Reuse its value instead of another load. Uncached vertex alternatives repeat existing
   support reads per vertex; do not claim those are once-per-plant. Keep the first change
   coherent across paths; measure alternatives before adding an instance cache.
4. If measurement shows support reads matter, a once-per-instance preparation result can
   feed every layer at a bounded cadence (e.g. 10 Hz, proposed not measured), but it needs
   explicit initialization/reindex invalidation and scheduling. That is a later optimization,
   not a prerequisite for a handful of ALU operations attached to an existing fetch.
5. Four moisture levels imply visible stepped transitions. First assess them at garden
   scale. Do not silently add a second water simulation for smoothing. If unacceptable,
   use transient material interpolation toward the authoritative level with reset on
   instance/chunk replacement, and document its latency and frame-slot consistency.

A future **optional** cheap sun glint could be evaluated once per layer from a stable
normal, light and view direction, multiplied by sun visibility and `w*w` so sheen disappears
before the diffuse response. This is an artistic directional highlight, not reflection or
a genuine roughness control. Bound intensity, avoid sparkle on wind/pixelization and avoid
sun glints in shadow. Current final-color cache requires view-dependent glints to refresh
with camera motion. Do not cache them at a slow moisture cadence. Prefer darkening first;
there is no justification yet for a new BRDF, reflection pass, SSR, ray query or droplets.

Estimated incremental cost of the first slice: a species branch and a few scalar/vector
ALU operations per existing shaded layer (or alternative vertex); **zero intended extra
atlas loads** after sample reuse, no extra passes/buffers. These are structural estimates,
not measured timings or an assertion of a performance pass.

## Lifecycle, persistence and ownership contract

- Atlas remains sole water authority. Sprinkler removal stops new water, not existing wet
  appearance. Pause obeys the existing simulation policy. Do not dry grass on removal/mute.
- Derived color is rebuilt in the current preparation path; no saved mask, offline drying,
  user-world migration or preference rewrite. On load, use the restored soil level.
- New cache state, if later needed, must reset on world load, root movement, replanting,
  chunk rebuild, species reindex, capacity changes and frame-slot reuse; never inherit
  another plant's wetness. Existing final pose cache is ephemeral and frame-slot owned.
- Keep GPU writes/reads ordered with existing moisture and vegetation preparation. Review
  `src/app/core/mod.rs` command recording before implementation; do not introduce atlas
  hazards by moving moisture writes between preparation and draw.
- Proposed shared interface: a support sample with `moisture_level` and `growth_factor`,
  plus one grass material helper. Preserve 8-byte `Instance` and atlas encoding. No new
  sprinkler API, contact clock, source enumeration or soil-wetting policy changes needed.
- Wetness worker should own material/helper consumers. Controller/performance owner must
  serialize `src/tracer/mod.rs`, `src/app/core/mod.rs` and descriptor/frame-plan changes.
  Prefer no edits there for the initial slice. Any controls belong together under moisture
  response in declarative saved config, not App-only sliders. If delivered as an A/B
  experiment, provide the required runtime Debug checkbox: unchecked original, checked wet
  response, with additive default-compatible persistence.

## Later validation (not executed in Research phase)

Use disposable diagnostic worlds only; preserve user saves/preferences. Capture matching
camera, resolution, seed, lighting/exposure and grass age/competition. Hold or disclose wind
and growth changes so stress tint is not mistaken for moisture darkening.

- Dry reference (soil level 0), manually watered levels 1/2/3, sprinkler-watered patch and
  untouched neighboring grass: save overview and close/player-scale crops plus actual
  atlas levels. Verify the effect is immediately legible without turning the patch black.
- Remove/disable the sprinkler, observe levels and appearance during real drying; test
  drying-speed zero and resumed drying without altering irrigation. No invented fixed
  drying duration: current drying depends on visits/chunks and stochastic modifiers.
- Same saved wet world after reload; grass planted into already-wet soil; terrain edits,
  removed support, chunk boundaries and non-soil supports. Verify no stale wet plants.
- Compare cached/uncached grass, plain/pixelized rendering, tall/short grass, sun/shade and
  different camera directions. Inspect that flowers/trees remain unchanged. If glints are
  added, test grazing/high views and shadow leakage explicitly.
- Retain dry/wet/A/B screenshots under `target/garden-delivery/wetness/` with metadata;
  inspect actual images, not only their existence. Visual approval remains the player's.
- After implementation only: use the shared heavy-resource lock for fmt/check/tests,
  Release build and hidden muted native runs, inspect logs for Vulkan errors. Regenerate
  shader-derived code through cargo check/build only.
- Measure matched Release runs with wet response off/on, fixed populations and 0/1/4/8/16
  sprinklers, repetitions/warmup and settings disclosed. Compare grass preparation/draw
  GPU and CPU timings separately from irrigation. No timing claims from unit/debug runs.

## Research completion and remaining uncertainty

Clean assigned branch/base confirmed; AGENTS, CONTEXT, scope v1 and domain guidance read.
No ADR directory was present. Only this note is delivered; no generated files, saved worlds
or preferences changed. No builds, tests, games, GPU work, audio or screenshots were run.
No implementation blocker found. Leaf-surface fidelity, appropriate art strength, stepped
transitions and actual incremental Release cost remain unverified acceptance work.
