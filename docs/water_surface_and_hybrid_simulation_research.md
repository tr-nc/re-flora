# Water surface and hybrid simulation research

Status: research and target-architecture recommendation; no implementation
Date: 2026-08-09

## Decision

Re: Flora should **not** require every visible water volume to be backed by MLS-MPM
particles.

The recommended production target is a hybrid with three disjoint canonical
representations:

1. **Quiet basin water** is stored as a terrain-aware column/heightfield body and rendered
   with a clipped, tiled surface mesh. A fully sleeping body performs no fluid substeps.
2. **Active water patches** retain the existing weakly-compressible MLS-MPM particles and
   derive a coherent world-space scalar field at a declared simulation time. A GPU pass
   extracts a real surface mesh from sparse active bricks, initially with Marching Cubes or
   an equivalent baseline algorithm.
3. **Detached spray and droplets** remain particles. They are visual/ballistic detail after
   leaving a resolved water surface, not a reason to particle-simulate the whole pond.

The water surface mesh is a derived render product, never the authority for mass, terrain
collision, wake/sleep state, or persistence. Each parcel of water is owned by exactly one
canonical representation at a time.

Screen-space fluid rendering is worth implementing as a bounded A/B spike because it can
quickly prove the desired continuous look. It is **not the recommended production target**:
it produces a continuous image rather than reusable world-space geometry, and Re: Flora's
editable basins, stable pond silhouettes, lighting integration, and future non-camera
consumers favor an extracted mesh.

The first implementation should stop well before automatic hybrid conversion: manually
select one bounded active patch, publish one coherent same-simulation-time surface frame,
and compare a GPU density-field mesh against a screen-space renderer. Do not build parcel
conversion, automatic activity classification, or whole-world meshing until this gate is
green.

## Product and engineering constraints

The game direction treats editable voxel terrain as the physical layer that supports water
basins, while rasterized objects and meshes are a complementary visual layer
(`docs/game_direction.md:47-58`). It asks for low-resolution water highlights and explicitly
values water as atmosphere: ponds, shallow streams, reflections, and ripples
(`docs/game_direction.md:141-148`, `docs/steam_direction.md:92-100`). Intentional temporal
stepping is part of the aesthetic, but unstable topology, stale seams, and mixed-time
ghosting are correctness failures rather than authored stepping
(`docs/steam_direction.md:42-64`).

The practical target is therefore not a film-quality universal fluid solver. It is:

- one or more sizable, readable ponds on consumer PCs;
- local splashes, pours, waterfalls, terrain edits, and player disturbances that visibly
  respond;
- a stable, continuous surface at the game's low internal resolution;
- predictable CPU/GPU budgets and no render-thread wait on the water worker;
- exact accounting when water changes representation.

The README requires a Vulkan-capable GPU but does not require RTX hardware
(`README.md:14-20`). A design that is acceptable only on the current RTX 3060 Ti validation
machine is not yet consumer-PC evidence.

## Current implementation snapshot

### Simulation state and update path

`PondWaterSim` owns the canonical current water state: an AoS `Vec<WaterParticle>` with
position `x`, velocity `v`, affine matrix `c`, and volume ratio `j`, plus a dense MPM grid,
touched-node list, terrain SDF samples, pressure-only terrain ghost density, accumulators,
and diagnostics (`crates/re-flora-water/src/pond.rs:230-255`,
`crates/re-flora-water/src/pond.rs:323-344`).

Each fixed substep is:

```text
clear touched grid nodes
    -> particle-to-grid mass, momentum, ghost-boundary density, and stress
    -> grid update and terrain projection
    -> grid-to-particle gather and repair
    -> quiet-settling damping and diagnostics
```

This order is explicit in `crates/re-flora-water/src/mls_mpm/step.rs:76-128`. The P2G mass
field already records unique touched nodes and persists until the next sparse clear
(`crates/re-flora-water/src/mls_mpm/p2g.rs:17-36`,
`crates/re-flora-water/src/mls_mpm/p2g.rs:38-120`). That field is a promising *derived
surface input*, but it is transient solver state, not a stable water-volume representation.

The default particle edge is `0.035` world units, close to the current 32-cells-per-world-unit
grid. `PondWaterConfig::default()` uses a 240 Hz substep (`1 / 240 s`)
(`crates/re-flora-water/src/pond.rs:12-24`, `crates/re-flora-water/src/pond.rs:71-100`). The
app sizes the runtime grid from the current `2 x 2 x 2` world at 32 cells per unit, yielding
`64 x 64 x 64`; the explicit `performance` profile instead forces 60 Hz
(`src/app/core/mod.rs:815-817`, `src/app/core/mod.rs:1363-1384`). Persisted GUI values and
CLI overrides can still change the effective configuration.

Simulation runs on the dedicated `water-sim` thread. Normal ticks allow at most four
substeps; terrain work temporarily lowers that to two. The main thread polls immutable
snapshots at the water tick interval rather than running the solver itself
(`src/app/core/water/runtime.rs:15-20`, `src/app/core/water/mod.rs:763-792`).

### Terrain boundary and collision data

Water collision is deliberately based on a smooth, filled-solid terrain representation:

```text
revisioned Contree dependency
    -> GPU atlas filled-solid sample
    -> immutable solid grid
    -> signed distance, normal, and pressure-only ghost density
    -> MLS-MPM terrain collision and boundary pressure
```

The sparse Contree surface shell is not interchangeable with this payload. A measured
experiment found only 109-296 occupied shell samples where the filled atlas produced
12,844-17,322, and the deepest negative SDF collapsed from about `-0.5` to `-0.016`
(`docs/voxel_collision_architecture.md:115-145`). The surface project must preserve this
revisioned filled-solid path; a render mesh must not replace it.

Terrain ghost density contributes to EOS pressure/stress but not real grid mass or grid
velocity normalization. The cached implementation measured approximately
`0.556 ms/substep` rather than `0.960 ms/substep` in the recorded 1,000-particle startup
case (`docs/water_boundary_density.md:17-27`). This distinction matters when deriving a
surface field: render occupancy must come from real water mass, not from pressure-only
solid-side ghost density.

### Current renderer handoff is debug-only

The worker snapshot contains only position and velocity
(`src/app/core/water/runtime.rs:155-179`). While simulation is enabled, it publishes one of
four interleaved particle buckets per snapshot; the main thread merges each new quarter
into the prior display snapshot (`src/app/core/water/runtime.rs:286-324`,
`src/app/core/water/runtime.rs:556-579`, `src/app/core/water/runtime.rs:705-743`). The merged
array therefore contains particles from up to four different simulation times.

That mixed-time cache is acceptable for debug points but **must not** be splatted into a
density field or meshed. It would turn smooth particle motion into changing implicit-field
noise and topology flicker. A surface input must carry one simulation revision and one
`sim_time_seconds`, and all of its samples must belong to that time.

The app currently appends valid water snapshots to the generic particle list, truncates them
to whatever remains of the global 16,384-particle capacity, and sets
`kind: ParticleRenderKind::Leaf` (`src/particles/system.rs:6-10`,
`src/app/core/particles.rs:529-565`). These debug markers use the ordinary opaque/textured
particle path. `WaterDroplet` is a separate kind routed into a sorted translucent instance
buffer and a premultiplied fragment pipeline (`src/tracer/mod.rs:6003-6130`,
`src/tracer/pipeline_builder.rs:917-934`). Neither path is a continuous MLS-MPM surface.

At a tightly packed 12 bytes per position, a coherent position-only upload is
`12N` bytes; position plus velocity is `24N` bytes before allocator, padding, and staging
overheads. At 100,000 particles and 60 publications per second, those payload lower bounds
are about 72 MB/s and 144 MB/s respectively. Raw bus bandwidth is unlikely to be the only
cost: worker-side gathering, allocation, locking, main-thread copies, staging writes, and
GPU synchronization must all be measured.

### Existing surface extraction is not a fluid mesher

`shader/slang/surface_extraction.slang` preloads binary voxel occupancy and type rows,
rejects solid voxels hidden by six solid neighbors, estimates a normal from a 5-cubed
occupancy window, and writes one packed surface-voxel record
(`shader/slang/surface_extraction.slang:17-25`,
`shader/slang/surface_extraction.slang:169-225`,
`shader/slang/surface_extraction.slang:228-249`). It does not construct a continuous scalar
field, interpolate an isosurface, or emit fluid triangles.

Its managed GPU-job, sparse-workgroup, active-brick, buffer-publication, and profiling
patterns may inform scheduling. Its extraction algorithm and output are not a ready-made
Marching Cubes implementation and should not be presented as direct reuse.

### Recorded water costs and their limit

`docs/water_sim_performance.md` is dated 2026-05-20 and describes an older
`160 x 64 x 160`, `5 x 2 x 5`, 120 Hz setup. Its strongest directional result is that water
cost scales materially with particle count and G2P dominated the recorded 100k workload.

| particles | historical average ms/substep | historical 120 Hz result |
| ---: | ---: | --- |
| 10,000 | 3.52 | realtime |
| 25,000 | 6.65 | realtime |
| 50,000 | 13.02 | behind |
| 100,000 | 26.09 | behind |

After the recorded G2P/P2G optimizations, the long 100k average was `23.10 ms/substep`, of
which `18.31 ms` was G2P and `4.47 ms` was P2G
(`docs/water_sim_performance.md:124-159`). The original breakdown put G2P at 77.8% of total
(`docs/water_sim_performance.md:54-78`). These measurements explain why a large quiet pond
must not casually multiply active particles.

They are not a confirmed current benchmark: current world bounds, default rate, GUI state,
code, and hardware conditions differ. The implementation gate requires a new release sweep
at fixed configuration and matched render extent. Main-thread `water_handoff` is also not
solver compute time (`docs/water_sim_performance.md:18-38`).

## Two practical ways to make particles look continuous

### Screen-space fluid rendering

The standard screen-space pipeline is:

```text
coherent particles
    -> nearest particle-sphere depth
    -> additive thickness
    -> depth smoothing / curvature flow
    -> normals reconstructed from depth
    -> transparent water composite using scene color/depth
```

Van der Laan, Green, and Sainz describe exactly this depth, thickness, curvature smoothing,
and compositing flow, including its configurable resolution/iteration trade-off and the fact
that it renders only the nearest layer correctly
([paper](https://doi.org/10.1145/1507149.1507164),
[author-hosted copy](https://pure.rug.nl/ws/portalfiles/portal/14497408/05c5.pdf)). NVIDIA
FleX is a concrete implementation precedent: its official manual exposes smoothed particle
positions and anisotropy vectors specifically for ellipsoid splatting and screen-space
surface reconstruction
([FleX 1.2 manual](https://nvidiagameworks.github.io/FleX/1.2/lib_docs/manual.html#fluids)).

Advantages for Re: Flora:

- no triangle generation, mesh capacity management, or world-space crack stitching;
- work follows visible pixels and naturally gains view-dependent LOD;
- thickness is directly available for approximate absorption and refraction;
- a half/quarter-resolution implementation may fit the low-resolution aesthetic;
- the same coherent position frame can test it before committing to a mesh architecture.

Limits:

- it is a continuous *view*, not a surface mesh;
- only the camera-visible front layer is reconstructed; stacked sheets and air gaps are
  ambiguous, as the original paper explicitly notes;
- reflection/refraction are scene-color/depth approximations; off-screen information needs
  another representation;
- it supplies no reusable geometry for shadow maps, world-space reflection captures, DDGI
  occlusion, selection, or collision;
- screen edges, disocclusions, thin sheets, smoothing radii, and camera cuts need explicit
  artifact tests;
- it still requires a coherent full-frame particle input. Feeding the current four-bucket
  display cache would create mixed-time ghosting.

The last three integration conclusions are engineering inferences from a camera-space-only
representation, not direct performance claims from the paper.

### Particle scalar field plus surface extraction

The world-space pipeline is:

```text
coherent particles or coherent solver-grid mass
    -> sparse density / occupancy / level-set bricks
    -> isovalue classification
    -> Marching Cubes, Surface Nets, or similar extraction
    -> vertex normals and optional velocity/thickness attributes
    -> translucent raster water mesh
```

Marching Cubes constructs triangles for a constant-density isosurface by classifying cube
corners and interpolating edge crossings; gradients of the scalar data provide shading
normals ([Lorensen and Cline 1987](https://doi.org/10.1145/37401.37422)). A naive isotropic
particle kernel tends to preserve particle blobs and lose thin sheets. Yu and Turk show that
neighbor-derived anisotropic kernels plus smoothed kernel centers produce flatter surfaces
and better preserve thin streams and sharp features, while also documenting residual bumps
and a visual-volume shift that must not feed back into physics
([paper](https://faculty.cc.gatech.edu/~turk/my_papers/sph_surfaces.pdf)).

Advantages for Re: Flora:

- actual world-space geometry can share ordinary raster visibility, shadow, reflection, and
  material paths;
- the same mesh can be viewed from several cameras and from above or below;
- terrain intersections and seams can be inspected in world coordinates;
- derived world-space density can support rendering queries without making the mesh a
  collider;
- sparse active bricks bound work to active patches rather than the whole world.

Costs and risks:

- scalar splatting and extraction add GPU/CPU bandwidth, active-brick bookkeeping, append
  capacity, and synchronization;
- per-frame independent isosurface extraction can change topology as samples cross the
  threshold. Stable input time, hysteresis, deterministic brick halos, and temporal quality
  gates are required;
- isotropic kernels are the correct first baseline but may produce blobs or holes; anisotropy
  adds neighborhood search, covariance, and more expensive splats;
- Marching Cubes can emit many triangles and needs crack-free shared border samples;
- refraction still needs thickness or back-surface/depth support; a closed mesh does not make
  transparency free;
- using the render mesh for simulation collision would add lag and couple correctness to LOD.

An explicitly advected mesh can improve temporal coherence, but it introduces topology
repair and projection back to the particle implicit surface. Yu et al. demonstrate that
trade-off in an SPH context
([Explicit Mesh Surfaces for Particle Based Fluids](https://faculty.cc.gatech.edu/~turk/my_papers/mesh_surfaces_particle_fluids.pdf)).
That complexity is disproportionate for the first Re: Flora surface; independent sparse
extraction with measured temporal stability is the preferred baseline.

### Side-by-side decision matrix

| criterion | screen-space fluid | scalar field plus extracted mesh |
| --- | --- | --- |
| Output | Continuous camera image; no mesh | Real world-space triangle surface |
| First prototype effort | Lower | Higher |
| Temporal stability | Sensitive to depth filter, screen edges, disocclusion, and mixed-time input | Sensitive to isovalue crossings, brick borders, topology changes, and mixed-time input |
| Thin sheets | Can disappear after projection/filtering; ellipsoid splats help | Isotropic fields can perforate/shrink; anisotropic kernels help at added cost |
| Thickness | Natural additive screen thickness, approximate when layers overlap | Requires back surface, volume integration, or a separate thickness pass |
| Refraction | Straightforward scene-color offset, screen-space limited | Mesh supports geometric interface; scene-color/ray integration still required |
| Reflection | Screen-space or probe fallback; missing off-screen data | Fits ordinary planar/probe/ray paths, subject to renderer support |
| Shadows / DDGI | Needs a separate proxy or particle path | Mesh can participate as raster/shadow geometry; DDGI role is still a product decision |
| Collision / buoyancy | No reusable collision state | Mesh exists but should remain derived; query canonical columns/particles instead |
| CPU-to-GPU handoff | Coherent particle frame | Coherent particles, or coherent scalar/active-brick frame |
| GPU bandwidth | Depth/thickness render targets and filters | 3D field writes/reads, active lists, triangle append, vertex/index reads |
| Multi-camera / captures | Reconstruct per view | Reuse mesh until next coherent generation |
| Re: Flora target | A/B spike and possible low-tier fallback | **Recommended production active-water surface** |

## Coherent surface-input choices

The surface renderer needs a separate contract from `latest_particles()`:

```text
WaterSurfaceFrame {
    simulation_revision,
    sim_time_seconds,
    terrain_source_revision,
    patch_bounds,
    cell_size,
    particle_volume / isovalue metadata,
    one coherent payload,
}
```

Incomplete or stale frames retain the previous complete mesh. They are never merged into a
new field.

| input choice | benefit | cost / hazard | recommendation |
| --- | --- | --- | --- |
| Full coherent particle positions | Simplest truth-preserving surface baseline; supports both screen-space and world-space A/B | O(N) worker copy and upload; reverses the current quarter-snapshot optimization | **Use first**, but only for one manually bounded patch and with dedicated timing |
| Worker-produced scalar/active-brick field | Can reuse already computed P2G mass and `touched_grid_nodes`; avoids a second GPU splat and may reduce transfer when sparse | Adds work to the CPU bottleneck; current grid corresponds to a precise substep phase and is cleared next step; quantization/isovalue semantics need calibration | Second A/B if coherent particle handoff or GPU splat misses budget |
| GPU-resident simulation/render state | Removes recurring CPU particle copies and naturally shares GPU field state | A major solver migration, synchronization redesign, and new terrain-collision data path | Long-term only; not authorized by this research step |

The first frame can omit velocity if no accepted visual effect consumes it. Position-only
input halves the tightly packed payload relative to position plus velocity. If velocity is
later required for foam, stretching, or motion vectors, add it as an explicitly measured
attribute rather than inheriting the debug snapshot shape.

## Representations for large quiet water

### Comparison

| representation | good fit | limitations | Re: Flora role |
| --- | --- | --- | --- |
| Static or planar mesh clipped to a basin mask | Decorative pools with a fixed level; cheapest update cost | Does not conserve changing volume or propagate flow; terrain edits require re-clipping | Render LOD and first quiet-pond visual spike |
| Column heightfield / shallow-water grid | Ponds, shallow streams, wet/dry fronts, depth-averaged currents, ripples, and sleeping | Cannot represent overturning surfaces, stacked layers, waterfalls, or fully 3D splashes | **Canonical quiet-water target** |
| Sine/Gerstner displacement | Cheap authored surface motion on a mesh | Cosmetic; does not react to arbitrary terrain or conserve local water | Normal/displacement detail only |
| FFT spectral waves | Large wind-driven ocean surfaces with stationary statistical spectra | Periodic/spectral ocean model; excessive and poorly coupled for small editable basins | Not first-generation pond simulation; possible distant ocean decoration only |
| Whole-body MLS-MPM | Fully 3D motion and direct reuse of current solver | Cost scales with particles even when visually still; current historical 50k/100k cases missed 120 Hz | Active patches only |

Chentanez and Müller provide the strongest direct hybrid precedent. Their real-time system
uses a shallow-water heightfield over arbitrary terrain with wet/dry tracking, and converts
features the heightfield cannot represent—waterfalls, breaking waves, splashes—into particles
that exchange mass and momentum with the heightfield
([paper](https://matthias-research.github.io/pages/publications/hfFluid.pdf)). Their particles
are simpler than Re: Flora's mass-bearing MLS-MPM patches, so the paper supports the
representation split and conservation requirement, not this document's exact conversion
algorithm.

Their later restricted-tall-cell work is another useful principle: concentrate 3D cells
near the interesting free surface rather than paying for a uniform full-depth 3D domain
([paper](https://matthias-research.github.io/pages/publications/tallCells.pdf)). Re: Flora's
active-patch proposal is an architectural inference in the same spirit, not a direct port of
that solver.

For rendering precedent, Unreal's official Water system draws only tiled surface regions
defined by water bodies, shares the mesh across lake/river/ocean transitions, and morphs
tessellation LOD with camera distance
([official documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/water-meshing-system-and-surface-rendering-in-unreal-engine)).
This supports clipped/tiled mesh LOD as a practical rendering pattern; it does not prove
Re: Flora's simulation or performance.

A closer first-party game-production precedent is LIGHTSPEED STUDIOS' Photon Water System.
Its GDC 2023 presentation separates water data that can be precomputed or updated at runtime
(height, velocity, and foam) from an adaptively generated water mesh, then applies CDLOD-style
render acceleration
([direct slides](https://media.gdcvault.com/gdc2023/Slides/Open-World%2BWater%2BRendering%2Band%2BReal-Time%2BSimulation_Mao_Zhenyu%26Wu_Kui.pdf),
[official session page](https://www.gdcvault.com/play/1028829/Advanced-Graphics-Summit-Open-World)).
This is concrete precedent for separating water-state data, update policy, and adaptive render
geometry. It does not establish that Photon Water's equations, data layout, or performance
transfer to Re: Flora.

Procedural waves remain useful as visual detail. GPU Gems describes a base mesh displaced
by summed sine/Gerstner-like waves plus a higher-frequency normal map, explicitly as a
flexible rendering approximation rather than rigorous fluid simulation
([NVIDIA GPU Gems, Chapter 1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models)).
Tessendorf's spectral construction is specifically an ocean-water model
([course notes](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2002.pdf)).
The recommendation to defer FFT for bounded editable ponds is therefore a Re: Flora product
inference, not a claim that FFT water cannot render a pond.

## Recommended target architecture

### Canonical state and ownership

Introduce one water-domain owner, conceptually `WaterWorld`, while preserving the current
worker-thread isolation:

```text
App / gameplay
    -> revisioned water commands and terrain dependencies
    -> water worker owns mutable canonical WaterWorld
        WaterBody
          id, terrain dependency, total mass ledger
          disjoint parcels:
            QuietColumns { depth/volume, depth-averaged momentum }
            ActiveMpmPatch { particles, MPM grid/cache, patch bounds }
            BallisticSpray { particles with explicit return/evaporation policy }
    -> immutable coherent simulation publications
        -> WaterSurfaceRenderer owns only derived GPU fields and meshes
        -> gameplay query cache owns only revision-tagged derived queries
```

Canonicality rules:

- A unit of water mass belongs to exactly one parcel representation.
- Quiet columns and active particles never both count the same overlap volume.
- The renderer may spatially blend two surfaces during transition, but visual overlap does
  not duplicate canonical mass.
- `terrain_ghost_density`, render density, extracted triangles, thickness buffers, normal
  maps, and procedural ripples are derived caches.
- Terrain collision keeps the current filled-solid SDF and source-revision semantics.
- A representation change is an atomic worker transaction with before/after mass and
  momentum diagnostics. If prerequisites are not ready, the old state remains canonical.

### Surface data flow

Active mesh target:

```text
ActiveMpmPatch at completed substep T
    -> immutable coherent position frame for T (first implementation)
    -> asynchronous GPU upload, no render-thread wait
    -> particle kernel splat into haloed sparse scalar bricks
    -> isovalue extraction into bounded mesh buffers
    -> publish mesh generation only after the complete GPU job finishes
    -> translucent raster consumer with water material
```

If the full particle handoff misses budget, test this alternative without changing canonical
physics:

```text
P2G mass field at a declared substep phase T
    -> copy touched nodes into haloed scalar bricks on water worker
    -> immutable revisioned brick publication
    -> GPU extraction and render
```

Quiet surface target:

```text
QuietColumns + current terrain dependency
    -> water footprint / shoreline clipping
    -> tiled heightfield mesh and optional coarse velocity/ripple field
    -> camera-distance tessellation LOD
    -> cosmetic low-resolution highlights and ripples
```

Both mesh types should enter the renderer through one water material contract so color,
Fresnel response, refraction policy, reflection source, fog/absorption, and depth composition
do not change visibly at the representation seam. Under `CONTEXT.md` terminology, the first
water mesh should be a **Raster Consumer**. Whether water later becomes a DDGI occluder is a
separate measured lighting decision.

### Simulation activity versus render LOD

Keep these classifications independent:

- **Simulation activity** answers whether a parcel needs 3D MLS-MPM, quiet columns, or
  ballistic particles. It must not sleep merely because it is off camera.
- **Render LOD** chooses scalar resolution, mesh update rate, tessellation, material detail,
  reflection source, and whether a low-tier screen-space fallback is used. It may depend on
  camera distance and projected size.

Initial activity signals should be measured from existing diagnostics rather than guessed:

- average and maximum particle speed;
- kinetic energy and affine-motion magnitude;
- surface-height range/curvature and inter-cell flux;
- terrain contact, edit, spawn, rain/sprinkler, player, and rigid-body impulses;
- flux from an active neighbor and pending terrain/SDF revision work;
- time continuously below all quiet thresholds.

The existing quiet-settling code already computes whole-body average/max speed and
local-speed damping gates (`crates/re-flora-water/src/mls_mpm/diagnostics.rs:21-23`,
`crates/re-flora-water/src/mls_mpm/g2p.rs:71-120`). Those constants are useful telemetry
seeds, not approved sleep thresholds.

### Wake, sleep, and reactivation rules

Do not implement these automatically until the manual active-patch and quiet-body phases
have independent acceptance evidence.

Sleep candidate:

1. A connected patch and one-cell/one-brick guard band remain below tuned velocity, kinetic,
   affine, height-gradient, and flux thresholds for a dwell interval.
2. There is no pending terrain revision, spawn, external contact, or active-neighbor flux.
3. Particles are integrated into quiet columns using particle mass and momentum; a diagnostic
   reports the residual.
4. The column result becomes canonical in one transaction; particles are then retired.

Wake trigger:

- a terrain edit changes the basin or support SDF;
- water is spawned/removed or a source/sink changes the level;
- rain, sprinkler, player, rigid body, or scripted impulse exceeds a threshold;
- column velocity/height gradient or neighbor flux exceeds a threshold;
- a heightfield-only configuration would create an overhang, waterfall, detached sheet, or
  other non-heightfield feature.

Wake transaction:

1. Wait for the matching filled-solid terrain/SDF dependency; do not seed particles against
   stale terrain.
2. Expand the active patch by a guard band and reserve its water volume from quiet columns.
3. Seed particles deterministically at the configured rest volume, initialize hydrostatic
   `j`, and map column momentum into particle velocity.
4. Reconcile the last fractional mass at the boundary instead of silently dropping it.
5. Publish the new representation generation atomically, then begin active substeps.

### Interface and conservation risks

The hybrid boundary is harder than either representation alone. Principal risks are:

- double mass from overlapping particles and columns;
- mass loss from fractional particle counts during conversion;
- momentum/energy jumps when depth-averaged flow becomes 3D particles or settles back;
- a pressure reflection at an active/quiet boundary;
- visible height, normal, thickness, foam, or update-rate seams;
- repeated wake/sleep thrashing around a threshold;
- terrain edits publishing a new visual basin before the matching water SDF/state is ready;
- stale GPU mesh jobs overwriting a newer representation generation;
- scalar-brick cracks from missing one-cell kernel halos or mismatched isovalues;
- apparent volume shrink from smoothing or anisotropic reconstruction even when canonical
  mass is conserved.

Mitigations are explicit mass ledgers, hysteresis plus dwell time, guard bands, conservative
flux exchange, deterministic fractional residual handling, shared border samples, one water
material, and latest-revision-wins publication. Rendering may cross-fade old/new surfaces for
a few authored frames, but physics ownership changes only once.

## Phased implementation and acceptance gates

### Phase 0: refreshed evidence and coherent surface contract

Scope:

- Add a deterministic hidden benchmark/capture scenario containing one manually bounded
  active patch and one sizable quiet pond footprint.
- Record effective water config, world/grid dimensions, particle count, active/touched nodes,
  camera snapshot, internal and swapchain extents, GPU, and water publications.
- Add `WaterSurfaceFrame` with one simulation time/revision. Publish position-only data for
  the manual patch without changing the existing debug snapshot.
- Keep the prior complete frame if a new frame is unavailable or stale.

Acceptance:

- Every consumed surface sample has the same simulation revision and time.
- No four-bucket merge feeds the surface path.
- Surface publication never blocks the render thread or waits for the water worker.
- A 10k/25k/50k/100k release sweep refreshes solver and handoff costs at fixed effective
  60 Hz and 240 Hz profiles where stable, with anomalies logged.

### Phase 1: bounded active-water renderer A/B

Scope:

- From the same coherent frame, build:
  - a screen-space depth/thickness renderer;
  - a sparse GPU scalar field and Marching Cubes-equivalent world-space mesh.
- Use a fixed manual patch, fixed isovalue/kernel radius, explicit brick halo, bounded append
  buffers, and independent GPU profiler scopes.
- Do not add automatic activity, quiet conversion, persistence, or whole-world allocation.

Production decision gate:

- The mesh is the default target if it meets the budgets below and produces a stable surface.
- Screen-space may remain a diagnostic or lower-tier fallback. It becomes the production
  target only through a new explicit decision if the world-space mesh cannot meet the
  consumer-PC gate after the worker-brick A/B.

Visual acceptance:

- No visible individual marker quads in the active water.
- In a settled fixed-camera sequence, the free-surface silhouette moves by at most one pixel
  at internal render resolution outside authored ripple/highlight motion.
- No single-frame free-surface holes larger than four internal pixels in the standard splash,
  pour, and settle captures.
- A camera orbit and camera cut reveal no persistent screen-edge/disocclusion gaps for the
  selected production path.
- Terrain contact has no open crack wider than one surface cell.
- Mesh generation is finite, within capacity, and identical at shared brick borders.

### Phase 2: one quiet pond without particles

Scope:

- Represent one authored/edit-derived pond as quiet columns plus a clipped/tiled heightfield
  mesh.
- Stop column simulation entirely when the body is sleeping; cosmetic mesh/normal ripples may
  continue at an authored rate.
- Use the same material contract as the active mesh.

Acceptance:

- Increasing quiet pond area does not increase MLS-MPM particle count or water-worker
  substeps.
- A sleeping pond reports zero fluid-solver work and remains visually stable for 60 seconds.
- Shoreline clipping follows a deterministic terrain revision; a stale terrain result cannot
  publish.
- Active and quiet reference surfaces match color, reflection/refraction policy, and mean
  water level within one surface cell.

### Phase 3: manual conversion and seam prototype

Scope:

- Add explicit debug actions `wake selected patch` and `sleep selected patch`.
- Implement mass/momentum transactions, guard bands, and a renderer seam.
- Repeat the same wake/sleep cycle before adding automatic thresholds.

Acceptance:

- Per conversion, canonical mass residual is at most `0.1%` of the converted patch mass; after
  100 wake/sleep cycles cumulative drift is at most `0.5%`.
- Horizontal momentum residual is at most `1%` when no external impulse is applied. Energy is
  logged but not conserved across intentional settling damping.
- No parcel is simultaneously counted as quiet and active.
- The free-surface height discontinuity across the guard band is at most one surface cell and
  does not grow while idle.
- Repeating a disturbance near the threshold does not cause more than one representation
  change per dwell interval.

### Phase 4: automatic activity and render LOD

Scope:

- Calibrate thresholds from recorded scenarios, then add hysteresis/dwell-based wake/sleep.
- Add distance-based scalar resolution, mesh update rate, tessellation, and material detail.
- Add ballistic spray return-to-body accounting if spray is mass-bearing.

Acceptance:

- Off-camera water remains physically correct; only render LOD changes with visibility.
- The same deterministic input sequence produces the same representation transitions and
  water-mass log.
- Automatic behavior meets all Phase 3 conservation/seam gates.
- A large quiet pond plus the approved maximum active patch count meets the release budgets on
  the designated minimum-spec PC.

## Release-mode benchmark protocol

Performance conclusions use hidden, muted, release-mode runs. The current solver sweep can be
refreshed with:

```bash
cargo run --release -- --hidden --mute --auto-exit 35 --perf \
  --water-profile performance --water-particles 10000
cargo run --release -- --hidden --mute --auto-exit 35 --perf \
  --water-profile performance --water-particles 25000
cargo run --release -- --hidden --mute --auto-exit 35 --perf \
  --water-profile performance --water-particles 50000
cargo run --release -- --hidden --mute --auto-exit 35 --perf \
  --water-profile performance --water-particles 100000
```

Add named benchmark scenarios to `config/perf_scenarios.toml` before drawing surface-renderer
conclusions. Follow `docs/performance-benchmarking.md`: warm both variants, run separate
binaries in order `A,B,B,A`, pool repeated samples, and reject mismatched workloads.

Required matching fields:

- commit, dirty state, host, GPU, driver, present mode;
- effective water profile and every water tuning override;
- world/collider bounds, grid dimensions, particle volume/count, active node count;
- manual patch bounds, scalar dimensions/isovalue/kernel, active brick count, triangle count;
- camera snapshot, internal render extent, and actual swapchain extent;
- warm-up duration and sample counts.

Required metrics:

- `[PERF][WATER]` average and P2G/G2P breakdown;
- `[PERF][WATER_THREAD]` collect, lock, published-particle/brick counts;
- main-thread handoff and upload CPU time;
- separate GPU scopes for splat, filter/field build, extraction, mesh draw, and screen-space
  depth/thickness/filter/composite;
- full-frame median and p95;
- allocated scalar, mesh, staging, and history bytes;
- stale/dropped generations, append overflows, non-finite values, terrain penetration, and
  Vulkan validation/fatal errors.

Provisional go/no-go budgets for the selected water surface path on the **designated
minimum-spec Vulkan consumer PC** are:

- water-surface GPU work at or below `1.5 ms` median and `2.5 ms` p95 in the combined quiet
  pond plus 25k active-patch scenario;
- surface gather/handoff/upload at or below `0.5 ms` p95 on the main thread;
- no more than `5%` regression in `[PERF][WATER] avg ms/substep` versus the matched no-surface
  build;
- no more than `5%` full-frame median and `10%` p95 regression versus the matched debug-marker
  disabled baseline;
- zero render-thread waits, mesh overflows, stale publications, validation errors, or fatal
  log lines;
- quiet-pond simulation cost remains zero while sleeping and MLS-MPM cost depends on active
  particles, not total visible pond area.

These budgets are design gates, not measured results. If no minimum-spec PC has been selected,
RTX 3060 Ti results remain machine-local and cannot close the consumer-PC gate. Do not relax a
failed budget silently: first compare worker-produced scalar bricks, lower scalar resolution,
lower update cadence, bounded active area, and the screen-space fallback.

## Non-goals

- No Rust or shader implementation in this research step.
- No migration of MLS-MPM to the GPU.
- No replacement of the filled-solid terrain SDF, ghost-boundary density, or terrain revision
  system.
- No use of the render mesh as the canonical water collider.
- No automatic wake/sleep before the manual transition gate.
- No ocean-scale FFT simulation, breaking-wave ocean, or infinite water plane.
- No film-quality anisotropic reconstruction, explicit mesh tracking, foam system, caustics,
  underwater renderer, or multi-layer refraction in the first surface spike.
- No persistence-format decision for hybrid water bodies in this document.
- No guarantee of rigid-body buoyancy or player swimming; those consumers need a separate
  canonical-state query contract.

## Unresolved decisions

1. What GPU and CPU define the minimum-spec consumer PC?
2. What maximum simultaneous active-patch volume and particle count must gameplay support?
3. Is the first quiet representation a static clipped level, a conservative shallow-water
   solver, or a static level followed by the solver in a later phase?
4. Should the first scalar field reuse coherent P2G mass or independently splat particles
   after the full-particle baseline?
5. What field precision, brick size, kernel radius, isovalue, and halo produce the intended
   low-resolution look without holes?
6. Is isotropic reconstruction sufficient, or does the accepted splash require anisotropic
   kernels?
7. Which reflection source is authoritative for water: planar, screen-space, environment
   probe, ray traced, or a quality-tier combination?
8. Does water cast direct shadows or affect DDGI, and if so at which LOD/update rate?
9. Are ballistic spray particles canonical mass that must return to a body, or non-conserving
   visual effects?
10. What exact terrain edit policy applies to sleeping water: immediate column recompute,
    forced wake, or a volume-preserving basin re-solve?
11. How will hybrid water be serialized once runtime water persistence is in scope?

## Evidence and inference table

| ID | class | source | direct support | inference boundary |
| --- | --- | --- | --- | --- |
| L1 | Direct local code | `crates/re-flora-water/src/pond.rs`, `crates/re-flora-water/src/mls_mpm/*` | Particle/grid canonical state, fixed-step order, transient touched-node mass, defaults | Does not prove that its P2G mass is the best render field |
| L2 | Direct local code | `src/app/core/water/runtime.rs`, `src/app/core/water/mod.rs` | Worker ownership, four-bucket mixed-time display snapshot, config/runtime cadence | Mixed-time meshing artifacts are a reasoned consequence to verify in a spike |
| L3 | Direct local code | `src/app/core/particles.rs`, `src/particles/system.rs`, `src/tracer/mod.rs` | Debug water is capacity-limited generic `Leaf` rendering; `WaterDroplet` is separate translucent billboard rendering | Does not predict final mesh material cost |
| L4 | Direct local evidence | `docs/water_sim_performance.md` | Historical release-mode particle-count scaling and G2P/P2G timing | Results are not current apples-to-apples evidence because setup/defaults changed |
| L5 | Direct local evidence | `docs/water_boundary_density.md`, `docs/voxel_collision_architecture.md` | Pressure-only ghost-density rule and required filled-solid terrain SDF semantics | Does not choose a free-surface renderer |
| L6 | Direct local code | `shader/slang/surface_extraction.slang` | Existing terrain extraction consumes binary occupancy and emits packed surface voxels | Scheduling ideas may transfer; the algorithm is not a fluid mesher |
| E1 | Direct external research | [van der Laan, Green, Sainz 2009](https://doi.org/10.1145/1507149.1507164) | Screen-space depth, thickness, curvature smoothing, composite, view-dependent LOD, nearest-layer limitation | Re: Flora integration and budget are unmeasured |
| E2 | Direct official implementation documentation | [NVIDIA FleX 1.2 manual](https://nvidiagameworks.github.io/FleX/1.2/lib_docs/manual.html#fluids) | Smoothed positions and neighbor anisotropy support ellipsoid splatting/screen-space reconstruction | FleX performance does not transfer to this CPU MLS-MPM implementation |
| E3 | Direct external research | [Yu and Turk 2010](https://faculty.cc.gatech.edu/~turk/my_papers/sph_surfaces.pdf) | Anisotropic particle kernels improve flat/thin/sharp surface reconstruction; reconstruction may visually shrink volume | Whether Re: Flora needs anisotropy is unresolved |
| E4 | Direct external research | [Lorensen and Cline 1987](https://doi.org/10.1145/37401.37422) | Triangle extraction from a sampled constant-density field with interpolated crossings and gradient normals | Sparse GPU layout, crack policy, and timing are project decisions |
| E5 | Direct external research | [Chentanez and Müller 2010](https://matthias-research.github.io/pages/publications/hfFluid.pdf) | Shallow-water grid plus particles for non-heightfield events, with mass/momentum exchange | Re: Flora's disjoint quiet-column/MLS-MPM parcel design is an adaptation, not their algorithm |
| E6 | Direct external research | [Chentanez and Müller 2011](https://matthias-research.github.io/pages/publications/tallCells.pdf) | Concentrating full 3D work near an interesting surface reduces large-water cost | Active MLS-MPM patches are an analogous architectural inference |
| E7 | Direct official engine documentation | [Unreal Water meshing](https://dev.epicgames.com/documentation/en-us/unreal-engine/water-meshing-system-and-surface-rendering-in-unreal-engine) | Clipped/tiled shared water surfaces and distance-based tessellation LOD are production patterns | It is precedent, not Re: Flora performance proof |
| E8 | Direct official technical presentation | [GPU Gems, Chapter 1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models) | Sine/Gerstner-like mesh displacement plus normal-map detail is cheap convincing rendering, not rigorous fluid physics | Recommended only as cosmetic quiet-water detail |
| E9 | Direct external course notes | [Tessendorf, *Simulating Ocean Water*](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2002.pdf) | Spectral/FFT techniques model ocean-water environments | Deferring FFT for editable ponds is a product inference |
| E10 | Direct first-party production presentation | [LIGHTSPEED STUDIOS, Photon Water System slides](https://media.gdcvault.com/gdc2023/Slides/Open-World%2BWater%2BRendering%2Band%2BReal-Time%2BSimulation_Mao_Zhenyu%26Wu_Kui.pdf) and [GDC session](https://www.gdcvault.com/play/1028829/Advanced-Graphics-Summit-Open-World) | Precomputed and runtime-updated height/velocity/foam data feed an adaptive water mesh with CDLOD acceleration | It is production precedent, not proof for Re: Flora's solver split or budget |
| I1 | Recommendation / inference | This document | Every visible pond should not be particle-backed; use disjoint quiet columns, active MLS-MPM patches, and spray | Must pass manual-patch, conservation, seam, and minimum-spec gates |
| I2 | Recommendation / inference | This document | Production active water should use a world-space scalar field and extracted mesh; screen-space is an A/B/fallback | May be reversed only by new measured evidence and an explicit decision |
| I3 | Recommendation / inference | This document | Start from full coherent position frames, then A/B worker scalar bricks if needed | Exact bandwidth and CPU/GPU balance require release measurement |

## Final answer to the design question

A continuous MLS-MPM surface should be produced from a coherent same-time particle or solver
mass field, not from the current quarter-updated debug snapshot. The recommended production
route is sparse world-space density bricks plus extracted mesh; screen-space depth/thickness
rendering is the fastest comparison spike and a plausible fallback, but it is not itself a
mesh.

Large still ponds should use a much cheaper clipped heightfield/column representation and
sleep completely. Only locally active, genuinely three-dimensional water needs MLS-MPM
particles. The hard part is not drawing the two representations; it is transferring exclusive
mass ownership, momentum, terrain revision, and surface continuity across their boundary.
That is why the implementation sequence starts with one manual active patch and one coherent
surface frame, then proves a particle-free quiet pond, and only then attempts automatic
wake/sleep conversion.
