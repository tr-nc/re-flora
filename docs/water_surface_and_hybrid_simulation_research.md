# Water surface and hybrid simulation research

Status: research and target-architecture recommendation; no implementation
Date: 2026-08-09

## Decision

Re: Flora should **not** require every visible water volume to be backed by MLS-MPM
particles.

The settled production architecture is a hybrid with two canonical volume representations and one
detached-detail class:

1. **Quiet basin water** is stored as terrain-aware conservative columns and rendered with a
   clipped, tiled heightfield mesh. The first implementation stores volume and horizontal
   momentum but performs no flux update while asleep; shallow-water advection is a later update
   law for bodies that demonstrably need resolved horizontal flow.
2. **Active water patches** retain the existing weakly-compressible MLS-MPM particles and
   derive a coherent world-space scalar field at a declared simulation time. The first bounded
   GPU baseline is a dense field plus an extracted triangle mesh. Dense storage is deliberately
   chosen for the current configured `64 x 64 x 64` domain; sparse bricks are an internal scaling
   option only after measurement shows that the dense baseline is the limiting cost.
3. **Detached spray and droplets** remain particles. They are non-canonical visual/ballistic detail
   by default and are not a reason to particle-simulate the whole pond. If gameplay later makes
   spray mass-bearing, it becomes an explicitly ledgered third owner with a return/removal policy.

The water surface mesh is a derived render product, never the authority for mass, terrain
collision, wake/sleep state, or persistence. Each parcel of water is owned by exactly one
canonical representation at a time.

Screen-space fluid rendering remains a useful diagnostic or fallback, but it is **not on the
mandatory first implementation path** and is not the recommended production target. It produces a
continuous camera image rather than reusable world-space geometry. Implement it only if the
bounded world-space mesh misses a declared quality or release budget, or if a lower quality tier
needs a measured fallback.

The first implementation should stop well before automatic hybrid conversion: extend the already
coherent particle publication with same-state surface metadata, select one bounded active patch,
and prove one dense scalar-field mesh behind a small deep Module. Do not build screen-space fluid,
sparse allocation, a quiet-water solver, parcel conversion, automatic activity classification, or
whole-world meshing unless the phase gates below authorize that work.

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
`crates/re-flora-water/src/mls_mpm/p2g.rs:38-120`). That field is a promising _derived
surface input_, but it is transient solver state, not a stable water-volume representation.

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

### Current particle handoff is coherent but debug-only

Commit `6a6c7d2e` completed the prerequisite publication fix. `WaterParticleFrame` now owns a full
position-and-velocity array, a publication revision, and one `sim_time_seconds`; replacement
requires a strictly newer revision plus finite, non-decreasing time
(`src/app/core/water/runtime.rs:183-219`). The worker copies every
particle from one completed `PondWaterSim` state and replaces the mailbox as one immutable value
(`src/app/core/water/runtime.rs:713-758`). The main thread uses `try_lock`; a busy mailbox, a missing
frame, a stale revision, or a regressed time retains the previous complete frame and never waits for
the worker (`src/app/core/water/runtime.rs:301-330`).

While simulation is enabled, the worker publishes one complete frame every fourth eligible
publication opportunity. This replaces four quarter-bucket copies with one full copy and preserves
approximately the previous average particle-copy rate without exposing mixed times
(`src/app/core/water/runtime.rs:20-64`, `src/app/core/water/runtime.rs:562-580`). Deterministic tests
cover complete same-state contents, monotonic revision/time replacement, stale/missing/busy
retention, and the selected cadence (`src/app/core/water/runtime.rs:847-960`). This invariant is
settled and must not be weakened by a surface implementation.

The frame is coherent in particle time but is not yet a complete surface-input envelope. It does
not carry the particle mass/volume, solver cell size and bounds, or the exact filled-solid terrain
generation actually applied by the worker (`src/app/core/water/runtime.rs:189-196`). Main-thread
configuration is updated before a coalescable command reaches the worker
(`src/app/core/water/runtime.rs:263-277`, `src/app/core/water/runtime.rs:607-638`), while terrain
collider and cache changes arrive through separate commands
(`src/app/core/water/runtime.rs:642-677`). Pairing a frame with whichever configuration or terrain
revision is newest on the main thread would therefore reintroduce cross-source incoherence. The
remaining prerequisite is to capture those physics metadata from the same completed worker state;
render-only kernel and isovalue policy stay in a separate revisioned render configuration.

The app currently appends finite samples from the complete frame to the generic particle list,
truncates them to whatever remains of the global 16,384-particle capacity, and sets
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
GPU synchronization must all be measured. A surface builder must reject an entire candidate with
non-finite required samples and retain the prior complete surface; copying the debug path's
per-sample filter would silently change reconstructed volume.

The coherence change was measured before and after with the same hidden, muted, release command,
25,000 particles, the `performance` water profile, the same RTX 3060 Ti, `2400 x 1350` swapchain,
and MAILBOX present mode:

```text
cargo run --release -- --hidden --mute --auto-exit 35 --perf \
  --water-profile performance --water-particles 25000
```

| publication scheme                                                                  | sampled seconds | publications/s | particles/publication | particles copied/s | total publish CPU | mean CPU/publication |
| ----------------------------------------------------------------------------------- | --------------: | -------------: | --------------------: | -----------------: | ----------------: | -------------------: |
| Four historical buckets (`re-flora-20260809-190550.223-71870.log`)                  |          34.342 |         14.792 |                 6,250 |           92,452.4 |         26.050 ms |          0.051280 ms |
| Complete frame every fourth opportunity (`re-flora-20260809-192044.002-163398.log`) |          34.545 |          3.590 |                25,000 |           89,738.0 |         17.651 ms |          0.142347 ms |

The new path published at 24.3% of the old frequency, copied four times as many particles per
publication, and retained 97.1% of the old average copy throughput. These are machine-local handoff
measurements, not surface-renderer or consumer-PC performance proof. The resulting roughly 3.6 Hz
publication rate in this loaded run is also a temporal-quality risk: if a mesh cannot tolerate it,
the response is to measure a position-only/shared coherent publication, coherent worker field, or
safe interpolation between whole frames—not to restore partial buckets.

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
| --------: | ----------------------------: | ------------------------ |
|    10,000 |                          3.52 | realtime                 |
|    25,000 |                          6.65 | realtime                 |
|    50,000 |                         13.02 | behind                   |
|   100,000 |                         26.09 | behind                   |

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
[university-hosted copy](https://wstahw.win.tue.nl/edu/2IV06/andrei/particle_rendering/provided/p91-van_der_laan.pdf)). NVIDIA
FleX is a concrete implementation precedent: its official manual exposes smoothed particle
positions and anisotropy vectors specifically for ellipsoid splatting and screen-space
surface reconstruction
([FleX 1.2 manual](https://nvidiagameworks.github.io/FleX/1.2/lib_docs/manual.html#fluids)).

Advantages for Re: Flora:

- no triangle generation, mesh capacity management, or world-space crack stitching;
- work follows visible pixels and naturally gains view-dependent LOD;
- thickness is directly available for approximate absorption and refraction;
- a half/quarter-resolution implementation may fit the low-resolution aesthetic;
- the same coherent position frame can test it later without changing the input contract.

Limits:

- it is a continuous _view_, not a surface mesh;
- only the camera-visible front layer is reconstructed; stacked sheets and air gaps are
  ambiguous, as the original paper explicitly notes;
- reflection/refraction are scene-color/depth approximations; off-screen information needs
  another representation;
- it supplies no reusable geometry for shadow maps, world-space reflection captures, DDGI
  occlusion, selection, or collision;
- screen edges, disocclusions, thin sheets, smoothing radii, and camera cuts need explicit
  artifact tests;
- it still requires a coherent full-frame particle input; the current `WaterParticleFrame`
  satisfies particle-time coherence, but the remaining metadata/terrain stamp must also match.

The last three integration conclusions are engineering inferences from a camera-space-only
representation, not direct performance claims from the paper.

### Particle scalar field plus surface extraction

The world-space pipeline is:

```text
coherent particles or coherent solver-grid mass
    -> bounded density / occupancy / level-set field
    -> isovalue classification
    -> hidden baseline isosurface extraction
    -> vertex normals and optional velocity/thickness attributes
    -> translucent raster water mesh
```

Marching Cubes constructs triangles for a constant-density isosurface by classifying cube
corners and interpolating edge crossings; gradients of the scalar data provide shading
normals ([Lorensen and Cline 1987](https://doi.org/10.1145/37402.37422),
[paper copy](https://www.cs.toronto.edu/~jacobson/seminar/lorenson-and-cline-1987.pdf)). A naive isotropic
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
- a bounded world field makes the first result inspectable and reusable without committing the
  caller to one extraction algorithm or storage layout.

Costs and risks:

- scalar splatting and extraction add GPU/CPU bandwidth, field clearing, append capacity, and
  synchronization;
- per-frame independent isosurface extraction can change topology as samples cross the
  threshold. Stable input time, deterministic field borders, and temporal quality
  gates are required;
- isotropic kernels are the correct first baseline but may produce blobs or holes; anisotropy
  adds neighborhood search, covariance, and more expensive splats;
- any baseline extractor needs a bounded worst-case output capacity and deterministic shared
  samples; a later sparse layout additionally needs crack-free brick halos;
- refraction still needs thickness or back-surface/depth support; a closed mesh does not make
  transparency free;
- using the render mesh for simulation collision would add lag and couple correctness to LOD.

An explicitly advected mesh can improve temporal coherence, but it introduces topology
repair and projection back to the particle implicit surface. Yu et al. demonstrate that
trade-off in an SPH context
([Explicit Mesh Surfaces for Particle Based Fluids](https://faculty.cc.gatech.edu/~turk/my_papers/mesh_surfaces_particle_fluids.pdf)).
That complexity is disproportionate for the first Re: Flora surface; independent extraction with
measured temporal stability is the preferred baseline.

The current configured application water domain is `64 x 64 x 64` cells
(`src/app/core/mod.rs:815-817`). A dense `R32F` field at that resolution is 262,144 samples,
or 1 MiB before extra buffers. This does not prove GPU cost, but it makes a bounded dense field the
smallest honest first Implementation. Starting with sparse bricks would add allocation, halos,
compaction, and crack policy before the project has shown that one dense active domain is too
large. Sparse storage becomes a measured internal optimization when active water grows beyond the
bounded baseline; it is not part of the external Interface.

### Side-by-side decision matrix

| criterion               | screen-space fluid                                                       | scalar field plus extracted mesh                                                             |
| ----------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| Output                  | Continuous camera image; no mesh                                         | Real world-space triangle surface                                                            |
| First prototype effort  | Lower                                                                    | Higher                                                                                       |
| Temporal stability      | Sensitive to depth filter, screen edges, disocclusion, and input cadence | Sensitive to isovalue crossings, stale jobs, topology changes, and input cadence             |
| Thin sheets             | Can disappear after projection/filtering; ellipsoid splats help          | Isotropic fields can perforate/shrink; anisotropic kernels help at added cost                |
| Thickness               | Natural additive screen thickness, approximate when layers overlap       | Requires back surface, volume integration, or a separate thickness pass                      |
| Refraction              | Straightforward scene-color offset, screen-space limited                 | Mesh supports geometric interface; scene-color/ray integration still required                |
| Reflection              | Screen-space or probe fallback; missing off-screen data                  | Fits ordinary planar/probe/ray paths, subject to renderer support                            |
| Shadows / DDGI          | Needs a separate proxy or particle path                                  | Mesh can participate as raster/shadow geometry; DDGI role is still a product decision        |
| Collision / buoyancy    | No reusable collision state                                              | Mesh exists but should remain derived; query canonical columns/particles instead             |
| CPU-to-GPU handoff      | Coherent particle frame                                                  | Coherent particles first; coherent solver field only after a measured gate                   |
| GPU bandwidth           | Depth/thickness render targets and filters                               | 3D field writes/reads, triangle append, vertex/index reads, and optional sparse active lists |
| Multi-camera / captures | Reconstruct per view                                                     | Reuse mesh until next coherent generation                                                    |
| Re: Flora target        | Conditional diagnostic or low-tier fallback                              | **Recommended production active-water surface and first Implementation**                     |

## Coherent surface input

The surface path must reuse the complete-frame invariant rather than invent a second particle
mailbox. The target worker publication is conceptually:

```text
WaterWorldFrame {
    publication_revision,
    sim_time_seconds,
    bodies: [
        WaterBodyFrame {
            body_id,
            body_revision,
            parcels: [
                WaterParcelFrame {
                    parcel_id,
                    representation_revision,
                    applied_filled_solid_terrain_dependencies,
                    source:
                        ActiveParticles {
                            positions, optional velocities,
                            particle_mass, particle_volume,
                            solver_bounds, solver_cell_size,
                        }
                        | QuietColumns { volume, horizontal_momentum, wet_mask }
                        | BallisticSpray { policy-tagged particles },
                }
            ],
        }
    ],
}
```

Phase 0B need not introduce every body variant. It extends or wraps today's
`WaterParticleFrame` with the exact worker-applied metadata needed by the first active surface. The
important rule is that all physics fields above are captured from one completed worker state.
Kernel radius, field precision, extraction isovalue, material, and LOD belong to a separate
`surface_policy_revision`; putting them into canonical physics would couple mass state to render
quality.

Each derived parcel job is keyed by
`(body_id, parcel_id, publication_revision, sim_time_seconds, representation_revision,
terrain_dependencies, surface_policy_revision)`. A view-dependent path additionally includes the
view/camera/internal-extent identity. An incomplete, non-finite, mismatched, or stale input retains
the prior complete surface and never contributes a row, brick, or particle to it. A completed GPU
job publishes only if its entire key is still current.

| input choice                              | benefit                                                                                                                | cost / hazard                                                                                                                        | recommendation                                                                 |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| Existing full coherent particle positions | Already implemented; simplest truth-preserving input to a bounded field; also supports a later screen-space comparison | O(N) worker copy and upload occurs in larger, lower-frequency bursts; current frame also carries velocity                            | **Use first**, for one bounded patch, after same-state metadata is stamped     |
| Worker-produced scalar/active-brick field | Can reuse P2G real mass and `touched_grid_nodes`; avoids a second GPU splat and may reduce transfer                    | Adds work to the CPU bottleneck; the grid exists at a precise substep phase and is cleared next step; ghost density must be excluded | Conditional second input only if measured particle handoff/splat misses budget |
| GPU-resident simulation/render state      | Removes recurring CPU particle copies and naturally shares GPU field state                                             | A major solver migration, synchronization redesign, and new terrain-collision data path                                              | Long-term only; not authorized by this research step                           |

The current frame retains velocity for existing debug behavior. A future surface-only shared view
may omit it if no accepted visual effect consumes it; position-only input halves the tightly packed
payload relative to position plus velocity. That is a measured optimization, not permission to
create another partial publication. If visual interpolation is introduced, it may use two complete
frames only and must account for spawn/remove identity rather than assuming particle indices are
stable.

## Representations for large quiet water

### Comparison

| representation                                | good fit                                                                               | limitations                                                                                          | Re: Flora role                                                               |
| --------------------------------------------- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Static or planar mesh clipped to a basin mask | Decorative pools with a fixed level; cheapest update cost                              | Does not conserve changing volume or propagate flow; terrain edits require re-clipping               | Derived render LOD/debug view only; never canonical mass state               |
| Column heightfield / shallow-water grid       | Ponds, shallow streams, wet/dry fronts, depth-averaged currents, ripples, and sleeping | Cannot represent overturning surfaces, stacked layers, waterfalls, or fully 3D splashes              | **Canonical quiet-water target**                                             |
| Sine/Gerstner displacement                    | Cheap authored surface motion on a mesh                                                | Cosmetic; does not react to arbitrary terrain or conserve local water                                | Normal/displacement detail only                                              |
| FFT spectral waves                            | Large wind-driven ocean surfaces with stationary statistical spectra                   | Periodic/spectral ocean model; excessive and poorly coupled for small editable basins                | Not first-generation pond simulation; possible distant ocean decoration only |
| Whole-body MLS-MPM                            | Fully 3D motion and direct reuse of current solver                                     | Cost scales with particles even when visually still; current historical 50k/100k cases missed 120 Hz | Active patches only                                                          |

The settled quiet model is one `QuietColumns` state, not separate “static pond” and
“shallow-water pond” representations. Each horizontal cell canonically stores liquid volume (or
mass), wet/dry state, and depth-integrated horizontal momentum. The free-surface height is derived
by fitting that volume into the matching filled-solid terrain column; cosmetic wave displacement
never changes it. `Sleeping` is the initial update mode and performs zero fluid substeps.
`ShallowWaterActive` is a later conservative flux update over the same state, enabled only when a
named stream, wet/dry-front, or disturbance-propagation scenario proves static columns inadequate.

This representation is valid only where each horizontal cell contains one connected water
interval with a single-valued free surface. Stacked layers, an overhang that disconnects the
column, overturning water, a detached sheet, or a waterfall remain active 3D water. A terrain edit
first waits for the matching filled-solid dependency, then preserves column volume while resolving
a new level; if the edited region no longer satisfies the heightfield condition, only that region
is proposed for wake. The old canonical state and last complete surface remain until that
transaction can commit.

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
([official documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/water-meshing-system-and-surface-rendering-in-unreal-engine?application_version=5.7)).
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

Procedural waves remain useful as visual detail. GPU Gems describes the water used in Cyan
Worlds' _Uru: Ages Beyond Myst_: a base mesh displaced by summed sine/Gerstner-like waves plus a
higher-frequency normal map, explicitly as a flexible rendering approximation rather than
rigorous fluid simulation
([NVIDIA GPU Gems, Chapter 1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models)).
Tessendorf's spectral construction is specifically an ocean-water model
([course notes](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2002.pdf)).
The recommendation to defer FFT for bounded editable ponds is therefore a Re: Flora product
inference, not a claim that FFT water cannot render a pond.

## Recommended target architecture

### Design It Twice outcome

Three independent Interface designs were compared before selecting the Seam:

| design                       | Interface                                                                                         | leverage                                                                                                     | main risk                                                                                  | decision                                               |
| ---------------------------- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------ | ------------------------------------------------------ |
| Tier-spanning `WaterWorld`   | `start / advance / shutdown` owns worker, terrain coordination, canonical state, and presentation | Smallest caller and maximum hidden scheduling                                                                | Makes a camera-aware renderer part of the same Module that owns mass; high god-Module risk | Reject as one Module; retain its canonical-worker half |
| Extensible `WaterSystem`     | `try_submit / update / record_view` plus named architecture profiles                              | Explicit command backpressure and multi-view lifecycle                                                       | Exposes representation profiles early and still combines physics and renderer lifetimes    | Useful internal vocabulary, not the external target    |
| Derived `WaterSurfaceModule` | one normal `update` call returns a complete opaque render state or the previous one               | Keeps the common caller simple while hiding cadence, storage, extraction, capacity, stale work, and fallback | Requires the worker publication to carry a complete cross-source stamp                     | **Selected**                                           |

The selected architecture has two deep Modules rather than one broad coordinator:

- the water-worker Module owns mutable canonical `WaterWorld` state, filled-solid terrain
  dependencies, mass/momentum transactions, and immutable publications;
- `WaterSurfaceModule` owns only derived GPU fields, meshes or screen targets, generation
  scheduling, last-good retention, capacity, and render telemetry.

This split gives Locality to both kinds of correctness. Deleting `WaterSurfaceModule` would force
the App and Tracer to learn field storage, extraction, job generations, stale-result rejection,
capacity, and fallback policy, so it has genuine Depth rather than being a pass-through. Conversely,
putting wake/sleep or mass transfer into it would make camera-dependent code canonical and violate
the physics Seam.

Do not publish generic `WaterSolver`, `TerrainProvider`, `SurfaceInputProvider`, GPU-backend, or
conversion traits. These are in-process dependencies with only one real Implementation. The first
dense mesh path can remain concrete. A private active-surface Adapter Seam is justified only when a
second production path—screen-space fluid or coherent worker fields—actually exists; a test fake by
itself is not a reason to expose abstraction.

### Canonical state and ownership

Preserve the current worker-thread isolation and deepen it incrementally:

```text
App / gameplay
    -> revisioned water intents and filled-solid terrain dependencies
    -> water worker owns mutable canonical WaterWorld
        WaterBody
          id, applied terrain dependency, total mass/momentum ledger
          disjoint parcels:
            QuietColumns { volume, wet state, horizontal momentum }
            ActiveMpmPatch { particles, MPM grid/cache, ownership mask }
            BallisticSpray { visual or explicitly mass-bearing policy }
    -> immutable complete WaterWorldFrame
        -> WaterSurfaceModule owns derived render generations only
        -> gameplay query cache owns revision-tagged derived queries only
```

Today's `PondWaterSim` is the existing active-water Implementation; it already owns particle state,
grid/cache state, terrain input, and `sim_time_seconds`
(`crates/re-flora-water/src/pond.rs:230-255`, `crates/re-flora-water/src/pond.rs:323-344`). The
`WaterWorld`, body, and quiet variants above are target domain state, not existing code.

Canonicality rules:

- A unit of water mass belongs to exactly one stable parcel representation.
- Quiet columns and the active ownership mask never claim the same control volume. Numerical
  stencils and visual cross-fades may overlap, but canonical ownership may not.
- `terrain_ghost_density`, render density, extracted triangles, screen thickness, normal maps,
  foam, and procedural ripples are derived and never enter the mass ledger.
- Terrain collision keeps the current filled-solid SDF source identity and sign semantics.
- A representation change is a prepared and validated worker transaction. If any prerequisite is
  missing, the old state remains canonical and no partial publication escapes.

### Small deep surface Interface

The following pseudocode fixes the caller contract without choosing a public meshing algorithm:

```rust
struct WaterSurfaceInput<'a> {
    /* newest complete WaterWorldFrame, immutable terrain payloads, and one render view */
}

struct WaterRenderableState<'a> {
    /* opaque draw-ready state borrowed from WaterSurfaceModule */
}

impl WaterSurfaceModule {
    fn update<'a>(
        &'a mut self,
        input: WaterSurfaceInput<'_>,
    ) -> Result<WaterRenderableState<'a>, WaterSurfaceFatal>;

    fn diagnostics(&self) -> &WaterSurfaceDiagnostics;
}
```

Normal usage is intentionally one path:

```rust
let water = water_surface.update(WaterSurfaceInput::from_runtime(
    water_runtime.latest_complete_world_frame(),
    render_view,
))?;
tracer.render_water(water)?;
```

The value types keep representation storage private. The App never calls
`build_marching_cubes`, `render_screen_space`, `update_quiet_heightfield`, `wake_patch`, or
`sleep_patch`. Canonical intents stay on the water runtime Interface; the surface call cannot
mutate them.

`update` has this contract:

1. Accept only a complete, finite, strictly newer publication with non-regressing simulation time.
2. Require its worker-applied terrain dependency and same-state particle/column metadata; never
   pair it with live main-thread configuration.
3. Coalesce superseded desired generations before expensive work where possible.
4. Record bounded asynchronous upload/build work and poll prior completion without waiting for a
   worker lock, GPU fence, or readback.
5. Publish a candidate only when every resource is complete and its full input/policy/view key is
   still current. Retire prior GPU ownership on the renderer's normal completion clock.
6. Return the new complete generation, otherwise the last complete generation, otherwise an empty
   valid state before first success.

This scheduling shape is already grounded in local code: the particle runtime polls with
`try_lock` and retains its last complete frame (`src/app/core/water/runtime.rs:301-330`); deferred
terrain surface work polls readiness and discards a non-latest revision
(`src/app/core/terrain_rebuild.rs:465-505`); DDGI promotes only a matching ready token and retires
the prior GPU owners on the frame-completion clock (`src/tracer/mod.rs:2379-2478`). Water reuses
these publication patterns, not those domains' data types.

Missing input, mailbox contention, a pending job, a terrain mismatch, recoverable allocation
pressure, or a stale completion are telemetry outcomes, not caller branches. Non-finite samples,
field/mesh overflow, and invalid topology reject the whole candidate; truncation or partial success
is forbidden. Only device loss or another existing renderer-fatal condition escapes as
`WaterSurfaceFatal`.

Repeated submission of the same key is O(1) metadata work. New input may perform one bounded
staging write, but must not clone another full particle vector on the main thread merely to simplify
lifetimes. A screen-space generation additionally belongs to a view/camera/internal-extent key, so
a camera cut or resize cannot reuse stale targets. A world-space mesh can be reused across views.

### Surface data flow

First active mesh Implementation:

```text
ActiveMpmPatch at completed state T + matching filled-solid dependency
    -> immutable complete surface input for T
    -> asynchronous GPU upload, no render-thread wait
    -> isotropic particle kernel splat into one bounded dense scalar field
    -> terrain exclusion/clipping against the same filled-solid generation
    -> bounded baseline isosurface extraction and normals
    -> atomically publish only the complete draw-ready generation
    -> translucent Raster Consumer with the water material
```

Extraction choice, scalar layout, and resource generations are private Implementation. The
world-space output does not by itself solve refraction thickness, reflection source, direct shadows,
or DDGI participation; those remain separate renderer policies. Under `CONTEXT.md` terminology,
the first mesh is a **Raster Consumer**, not automatically a DDGI Occluder.

If the matched dense-particle path fails a declared budget, test alternatives one at a time without
changing canonical physics:

```text
P2G real mass at a declared completed phase T
    -> worker copies touched nodes into one coherent field publication
    -> GPU extraction and render

or

complete particle frame T + view key
    -> screen depth/thickness/filter/composite
```

The worker field must exclude `terrain_ghost_density`; screen-space work is per view and must pass
edge/disocclusion/multiple-layer gates. Neither alternative silently activates at runtime: it is a
named benchmark profile and a new measured decision.

Quiet surface Implementation:

```text
QuietColumns + matching filled-solid dependency
    -> volume-derived wet footprint and shoreline
    -> clipped/tiled heightfield mesh
    -> optional camera-distance tessellation and cosmetic ripple
    -> same optical/material contract as active mesh
```

Both mesh paths share color, Fresnel response, refraction/reflection policy, fog/absorption, and
depth composition so those choices do not change at a representation seam.

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

1. A connected patch and one-cell guard band remain below tuned velocity, kinetic,
   affine, height-gradient, and flux thresholds for a dwell interval.
2. There is no pending terrain revision, spawn, external contact, or active-neighbor flux.
3. Every fixed-mass particle is integrated into column volume and horizontal momentum; vertical
   momentum and affine/kinetic energy that cannot survive depth averaging are explicitly logged.
4. The column result becomes canonical in one transaction; particles are then retired.

Wake trigger:

- a terrain edit changes the basin or support SDF;
- water is spawned/removed or a source/sink changes the level;
- rain, sprinkler, player, rigid body, or scripted impulse exceeds a threshold;
- column velocity/height gradient or neighbor flux exceeds a threshold;
- a heightfield-only configuration would create an overhang, waterfall, detached sheet, or
  other non-heightfield feature.

Wake transaction:

1. Defer until the matching filled-solid terrain/SDF dependency is available; do not block the
   main thread or seed particles against stale terrain.
2. Expand the proposed active region by a guard band and measure its quiet mass/momentum.
3. Because the current solver uses one `particle_mass` for every particle
   (`crates/re-flora-water/src/pond.rs:44-69`,
   `crates/re-flora-water/src/mls_mpm/p2g.rs:38-58`), choose an integer particle count and adjust a
   deterministic boundary column so the debit is exactly `N * particle_mass`. The leftover mass
   remains quiet outside the final active ownership mask; never create one differently weighted
   particle or round mass away.
4. Move the exact debit into transaction escrow, seed particles at the configured reference
   volume, initialize hydrostatic `j`, and map column horizontal momentum into particle velocity.
5. Validate finite state, capacity, terrain identity, exclusive ownership, mass equality, and
   momentum residual. Atomically install the target generation and retire the source; on failure,
   abort and leave the old owner untouched.

Stable body accounting is:

```text
body mass = quiet-column mass
          + active-particle count * particle_mass
          + explicitly mass-bearing spray mass
```

Transaction escrow exists only between prepare and commit. It is not a fourth stable water
representation. Default detached spray is non-canonical visual detail; if gameplay later makes it
mass-bearing, its return/removal policy must enter this ledger.

Once Phase 3 enables a body mass ledger, `particle_mass` and reference `particle_volume` become part
of an active parcel generation's canonical identity. Current GUI/config propagation can update both
on the running worker (`src/app/core/water/mod.rs:106-111`,
`src/app/core/water/runtime.rs:607-638`). Hybrid mode must not silently reinterpret existing
particles after such a change: accept the measure before parcel creation, or run an explicit
mass-preserving reparameterization/reseed transaction and bump the representation generation. This
is a future ownership gate; the present research step does not change current solver behavior.

Active/quiet face transport is a later gate, not something a guard band solves. When implemented,
one coupling ledger owns one signed mass-and-momentum flux record per interface face and fixed
simulation interval. The outgoing representation debits and the incoming representation credits
the same record exactly once; two solvers must never independently estimate and apply the same
flux. Patch-boundary motion first settles all prior flux records, then changes the ownership mask.

### Interface, terrain, and conservation risks

The hybrid boundary is harder than either representation alone. Principal risks are:

- double mass from overlapping particles and columns;
- mass loss from fractional particle counts during conversion;
- momentum/energy jumps when depth-averaged flow becomes 3D particles or settles back;
- a pressure reflection at an active/quiet boundary;
- visible height, normal, thickness, foam, or update-rate seams;
- repeated wake/sleep thrashing around a threshold;
- terrain edits publishing a new visual basin before the matching water SDF/state is ready;
- stale GPU mesh jobs overwriting a newer representation generation;
- future scalar-brick cracks from missing kernel halos or mismatched shared samples;
- apparent volume shrink from smoothing or anisotropic reconstruction even when canonical
  mass is conserved.

Terrain edits use the same prepare/commit discipline. Negative SDF remains solid, positive remains
empty/water space, and the gradient points away from solid
(`crates/re-flora-water/src/collider.rs:40-65`). The quiet resolver preserves canonical volume
against the matching immutable filled-solid grid. If one connected free-surface interval no longer
exists, it proposes only the affected region for active representation. No new shoreline or active
surface publishes against a different terrain dependency, and pressure-only ghost density never
becomes render occupancy.

Mitigations are explicit ledgers, hysteresis plus dwell time, conservative single-owner flux,
deterministic integer-particle reservation, shared field samples, one water material, and
latest-complete publication. Rendering may cross-fade old/new derived surfaces for a few authored
frames, but physics ownership changes only once.

## Phased implementation and acceptance gates

### Phase 0A: complete particle publication — completed in `6a6c7d2e`

Accepted evidence:

- every publication owns all particle positions/velocities from one completed simulation state;
- publication revision increases strictly and finite simulation time never regresses;
- missing, stale, or lock-busy polls retain the prior complete frame without a main-thread wait;
- enabled simulation publishes one complete frame every fourth eligible opportunity;
- deterministic unit tests cover completeness, time/revision coherence, stale/missing/busy
  behavior, and cadence;
- the matched hidden release measurement above shows 3.590 complete publications/s, 25,000
  particles/publication, and 89,738.0 particles copied/s for the measured 25k case.

This gate must remain green. Later work may optimize ownership or payload shape, but may never
restore partial merging.

### Phase 0B: cross-source surface input and named benchmark

Scope:

- Extend or wrap the worker publication with the exact applied filled-solid terrain dependency,
  particle mass/volume, solver bounds/cell size, and representation generation from that same
  completed state.
- Add the non-blocking `WaterSurfaceModule` generation coordinator with an empty/debug builder.
- Add one deterministic named hidden scenario for a bounded active patch to
  `config/perf_scenarios.toml`; record effective configuration, terrain identity, publication key,
  camera, internal/swapchain extents, and GPU.

Acceptance:

- pure tests reject partial/non-finite input, live-main-config pairing, time/revision regression,
  mismatched terrain, stale GPU completion, and capacity overflow;
- missing input, busy mailbox, pending job, and stale completion return the same last-good identity
  in bounded time;
- the main/render wait count is zero;
- no surface algorithm or representation branch appears at the App call site;
- a 10k/25k/50k/100k release sweep refreshes solver and complete-handoff evidence at fixed effective
  profiles where the workload remains stable.

### Phase 1: one bounded dense world-space mesh

Scope:

- Consume one accepted active-particle input; use a fixed manual patch, dense bounded field,
  isotropic kernel, explicit field border, bounded extraction output, and separate GPU profiler
  scopes.
- Publish only complete generations and draw through the shared water material as a Raster
  Consumer.
- Do not add screen-space fluid, sparse bricks, anisotropy, worker scalar fields, quiet water,
  automatic activity, persistence, or whole-world allocation.

Acceptance:

- the active water is a continuous surface rather than capacity-limited marker quads;
- every field/vertex is finite, output stays within declared capacity, stale jobs never publish,
  and terrain clipping uses the input's filled-solid dependency;
- fixed settle, splash, pour, orbit, and camera-cut captures expose no persistent particle-scale
  holes, terrain-contact crack wider than one surface cell, or cadence-driven topology flash under
  the review thresholds declared before implementation;
- dense field clear/splat/extract/draw and main-thread ingest pass the matched release protocol;
- the result on a development GPU is recorded as machine-local until the minimum-spec gate exists.

### Conditional Phase 1C: diagnose a failed active-mesh gate

Enter only when Phase 1 names the failed metric. Test one change at a time:

- a position-only/shared complete publication or identity-aware interpolation between two complete
  frames if publication cadence—not field construction—is the temporal-quality bottleneck;
- a coherent worker P2G-real-mass field if particle handoff or GPU splat is the bottleneck;
- sparse field storage if dense field memory/clear/extraction scales beyond the approved active
  domain;
- anisotropic kernels if the accepted thin-sheet scene fails the isotropic quality gate;
- screen-space depth/thickness if the world mesh misses the consumer-PC budget or a lower tier needs
  a measured fallback.

Each alternative uses the same complete input identity and named workload. Screen-space adds edge,
disocclusion, nearest-layer, camera-cut, resize, and multi-view gates. Changing the production
default requires an explicit measured decision; an Adapter cannot silently fall back.

### Phase 2: one particle-free sleeping quiet pond

Scope:

- Store one authored/edit-derived basin as canonical `QuietColumns` and render its clipped/tiled
  heightfield through the same optical/material contract.
- Preserve volume against the matching filled-solid terrain dependency.
- Keep it in `Sleeping`; cosmetic ripple may update, but there is no canonical shallow-water step.

Acceptance:

- increasing quiet pond area does not increase MLS-MPM particles or worker substeps;
- 60 seconds of sleeping reports zero fluid-solver work and no canonical mass/level drift;
- wet/dry and single-connected-column representability are deterministic;
- a stale terrain result cannot publish, and a deterministic terrain edit preserves volume or
  explicitly reports that the affected region must wake;
- active and quiet reference surfaces share material policy and meet the declared seam threshold.

### Conditional Phase 2C: conservative shallow-water update

Enter only if a named shallow stream, wet/dry front, or horizontal disturbance scenario cannot be
served by sleeping columns plus cosmetic detail. Advance volume and horizontal momentum with one
stable explicit time-step policy. Acceptance requires deterministic wet/dry behavior, equal and
opposite per-face flux accounting, bounded volume/momentum residual, and a return to zero substeps
after sleep. A cosmetic ripple alone does not justify a canonical solver.

### Phase 3: manual whole-region wake/sleep transaction

Scope:

- Add explicit debug intents to wake and sleep one selected bounded region.
- Implement prepare/escrow/validate/atomic-commit with an adjusted ownership mask and integer
  fixed-mass particles.
- Repeat one deterministic cycle 100 times. Do not add automatic thresholds or live active/quiet
  face transport yet.

Acceptance:

- the body ledger accounts for every unit of mass before and after each commit; any numerical
  residual remains an explicit diagnostic and no percentage budget permits unowned mass;
- the stable transaction has no fourth residual representation, no differently weighted particle,
  and no parcel with two canonical owners;
- horizontal momentum maps within a declared deterministic numerical tolerance; vertical momentum
  and energy loss are separately logged as intentional settling or numerical residual;
- 100 cycles show no directional mass trend, and an abort caused by terrain/capacity/stale input
  leaves the source canonical owner unchanged at the Interface test surface;
- a derived visual cross-fade does not alter the ledger, and the seam does not grow while idle.

### Phase 4: one conservative active/quiet interface flux

Scope one interface face or one small deterministic boundary before general coupling. One ledger
record owns its signed mass/momentum flux and both representations consume it exactly once.

Acceptance:

- debit equals credit for every sequence; duplicate, missing, out-of-order, terrain-stale, and
  representation-stale records are rejected deterministically;
- moving an ownership boundary first settles its prior flux sequence;
- there is no growing height/momentum discontinuity or reflected-pressure instability in the named
  boundary workload;
- all Phase 3 ownership and terrain gates remain green.

### Phase 5: automatic activity and render LOD

Scope:

- Calibrate hysteresis/dwell from recorded speed, kinetic/affine motion, height/flux, terrain edit,
  source/sink, and external-impulse scenarios; reuse the one conversion transaction.
- Add distance-based scalar resolution, mesh update rate, tessellation, and material detail without
  changing canonical activity.
- Add spray to the ledger only if gameplay explicitly makes it mass-bearing.

Acceptance:

- off-camera water remains physically correct; only render LOD changes with visibility;
- the same deterministic input sequence produces the same transition sequence, representation
  revisions, flux log, and water-mass log;
- threshold-adjacent disturbances do not cause more than one transition per dwell interval;
- every earlier conservation, terrain, stale-publication, and no-wait gate remains green;
- a large quiet pond plus the approved maximum active patch count passes on the designated
  minimum-spec consumer PC.

## Release-mode benchmark protocol

Performance conclusions use hidden, muted, release-mode application runs. Unit tests prove pure
state invariants; they are not performance evidence. An ad hoc solver/handoff sweep can be refreshed
with:

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

Those commands are preparatory evidence only. Before drawing a production conclusion, add named
workloads to `config/perf_scenarios.toml`, run `scripts/perf_suite.py`, warm separate release
binaries, execute `A,B,B,A`, pool repeated samples, and reject mismatched workloads as required by
`docs/performance-benchmarking.md:1-54`.

Required named scenarios by gate:

1. `water-active-handoff-{10k,25k,50k,100k}`: no-surface solver plus complete-frame baseline;
2. `water-active-dense-mesh-25k`: one fixed active patch and camera path;
3. `water-quiet-sleeping-large`: scalable quiet footprint with zero active particles;
4. `water-combined-quiet-active-25k`: final surface budget workload;
5. `water-manual-wake-sleep-100`: conversion ledger and surface preemption;
6. `water-interface-flux`: added only with Phase 4;
7. screen-space, sparse-field, anisotropy, or worker-field variants only when the corresponding
   conditional gate is entered.

Required matching fields:

- commit, dirty state, host, CPU, GPU, driver, Vulkan device, present mode;
- effective water profile and every water tuning override;
- world/collider bounds, grid dimensions, particle mass/volume/count, touched/active-node signature;
- accepted publication revision/time, representation generation, worker-applied terrain dependency
  and SDF/source hash;
- active ownership bounds, quiet footprint/wet-cell count, and coupling-face count when applicable;
- field dimensions/precision/kernel/isovalue/border, extraction policy revision, triangle/capacity
  counts, and optional active-brick count;
- camera snapshot, view count, internal render extent, and actual swapchain extent;
- warm-up duration, sample count, run order, and workload hash.

Required metrics:

- `[PERF][WATER]` average and P2G/G2P breakdown;
- `[PERF][WATER_THREAD]` publication frequency, particles/publication, particles copied/s, collect,
  lock, accepted revision/time, and optional published-brick counts;
- main-thread input validation, handoff, staging/upload, scheduling, promotion, and observed wait
  count;
- separate GPU scopes for dense clear, particle splat, terrain exclusion, extraction, mesh draw,
  and—only in a conditional path—screen depth/thickness/filter/composite;
- full-frame median and p95;
- allocated field, mesh, staging, history, quiet-column, and optional screen-target bytes;
- quiet solver substeps/wet cells, mass/momentum/flux ledgers, stale/deferred/dropped generations,
  capacity high-water/overflow, non-finite values, terrain penetration, and Vulkan validation/fatal
  errors.

Hard correctness gates are zero render/main waits, partial publications, unowned/double-owned mass,
duplicate/missing flux records, stale promotions, capacity overflows, non-finite output, Vulkan
validation errors, and fatal log lines. Sleeping quiet-water solver substeps are also exactly zero.

The following remain **provisional review budgets**, not established performance facts, for the
selected path on the designated minimum-spec Vulkan consumer PC:

- water-surface GPU work at or below `1.5 ms` median and `2.5 ms` p95 in the combined quiet
  pond plus 25k active-patch scenario;
- surface gather/handoff/upload at or below `0.5 ms` p95 on the main thread;
- no more than `5%` regression in `[PERF][WATER] avg ms/substep` versus the matched no-surface
  build;
- no more than `5%` full-frame median and `10%` p95 regression versus the matched debug-marker
  disabled baseline;
- MLS-MPM cost depends on active particles rather than total visible quiet-pond area.

No absolute millisecond gate can be accepted until the minimum-spec CPU/GPU, approved maximum active
water workload, and review resolution are named. RTX 3060 Ti results remain machine-local. Do not
relax a failed budget or change workload silently: lower dense resolution/update cadence or active
area first, then enter exactly one conditional Phase 1C alternative and repeat the matched protocol.

## Non-goals

- No Rust or shader implementation in this research step.
- No migration of MLS-MPM to the GPU.
- No replacement of the filled-solid terrain SDF, ghost-boundary density, or terrain revision
  system.
- No use of the render mesh as the canonical water collider.
- No automatic wake/sleep before the manual transition gate.
- No screen-space fluid, sparse field, shallow-water stepping, or hybrid conversion in the first
  active-mesh Implementation unless its explicit gate is entered.
- No ocean-scale FFT simulation, breaking-wave ocean, or infinite water plane.
- No film-quality anisotropic reconstruction, explicit mesh tracking, foam system, caustics,
  underwater renderer, or multi-layer refraction in the first surface spike.
- No persistence-format decision for hybrid water bodies in this document.
- No guarantee of rigid-body buoyancy or player swimming; those consumers need a separate
  canonical-state query contract.

## Settled decisions and required implementation inputs

The architectural questions in scope are settled:

- complete full particle frames are the first active-surface input; same-state physics/terrain
  metadata is the remaining prerequisite;
- the first continuous surface is a bounded dense world-space scalar field and hidden baseline
  extractor; screen-space, sparse storage, worker fields, and anisotropy are conditional;
- large quiet basins use conservative `QuietColumns`, sleep with zero substeps, and add
  shallow-water transport only for a named failed scenario;
- the worker owns one mass/momentum ledger and atomic representation transaction; the renderer owns
  only derived generations;
- stable conversions use two canonical volume representations plus optional explicitly
  mass-bearing spray, use fixed-mass particles, and leave fractional wake mass in adjusted quiet
  ownership rather than dropping it;
- quiet terrain edits volume-resolve against a matching filled-solid dependency and wake only an
  affected non-heightfield-representable region;
- the first surface is a Raster Consumer; reflection, direct-shadow, and DDGI participation are
  separate measured renderer decisions.

Implementation still requires these product/calibration inputs; they do not reopen the Module or
ownership architecture:

1. the minimum-spec CPU/GPU, approved render resolution, and maximum simultaneous active-patch
   volume/particle count;
2. review captures and numeric visual thresholds for the low-resolution settle, splash, pour,
   shoreline, camera-cut, and seam scenes;
3. measured field precision/resolution, isotropic kernel radius, isovalue, extraction algorithm,
   and capacities inside the selected dense Implementation;
4. the water reflection/refraction quality tiers and whether later direct-shadow or DDGI roles are
   worth their measured cost;
5. a persistence schema when runtime hybrid-water persistence enters scope.

## Evidence and inference table

| ID  | class                                        | source                                                                                                                                                                                                                                                                         | direct support                                                                                                                                        | inference boundary                                                                                         |
| --- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| L1  | Direct local code                            | `crates/re-flora-water/src/pond.rs`, `crates/re-flora-water/src/mls_mpm/*`                                                                                                                                                                                                     | Particle/grid canonical state, fixed-step order, transient touched-node mass, defaults                                                                | Does not prove that its P2G mass is the best render field                                                  |
| L2  | Direct local code                            | `src/app/core/water/runtime.rs`, `src/app/core/water/mod.rs`                                                                                                                                                                                                                   | Full immutable particle frames, monotonic revision/time, every-fourth-opportunity cadence, non-blocking retention; separately revisioned terrain work | Current frame still lacks worker-applied terrain/config metadata required for a cross-source surface stamp |
| L3  | Direct local code                            | `src/app/core/particles.rs`, `src/particles/system.rs`, `src/tracer/mod.rs`                                                                                                                                                                                                    | Debug water is capacity-limited generic `Leaf` rendering; `WaterDroplet` is separate translucent billboard rendering                                  | Does not predict final mesh material cost                                                                  |
| L4  | Direct local evidence                        | `docs/water_sim_performance.md`                                                                                                                                                                                                                                                | Historical release-mode particle-count scaling and G2P/P2G timing                                                                                     | Results are not current apples-to-apples evidence because setup/defaults changed                           |
| L5  | Direct local evidence                        | `docs/water_boundary_density.md`, `docs/voxel_collision_architecture.md`                                                                                                                                                                                                       | Pressure-only ghost-density rule and required filled-solid terrain SDF semantics                                                                      | Does not choose a free-surface renderer                                                                    |
| L6  | Direct local code                            | `shader/slang/surface_extraction.slang`                                                                                                                                                                                                                                        | Existing terrain extraction consumes binary occupancy and emits packed surface voxels                                                                 | Scheduling ideas may transfer; the algorithm is not a fluid mesher                                         |
| E1  | Direct external research                     | [van der Laan, Green, Sainz 2009](https://wstahw.win.tue.nl/edu/2IV06/andrei/particle_rendering/provided/p91-van_der_laan.pdf), [DOI](https://doi.org/10.1145/1507149.1507164)                                                                                                 | Screen-space depth, thickness, curvature smoothing, composite, view-dependent LOD, nearest-layer limitation                                           | Re: Flora integration and budget are unmeasured                                                            |
| E2  | Direct official implementation documentation | [NVIDIA FleX 1.2 manual](https://nvidiagameworks.github.io/FleX/1.2/lib_docs/manual.html#fluids)                                                                                                                                                                               | Smoothed positions and neighbor anisotropy support ellipsoid splatting/screen-space reconstruction                                                    | FleX performance does not transfer to this CPU MLS-MPM implementation                                      |
| E3  | Direct external research                     | [Yu and Turk 2010](https://faculty.cc.gatech.edu/~turk/my_papers/sph_surfaces.pdf)                                                                                                                                                                                             | Anisotropic particle kernels improve flat/thin/sharp surface reconstruction; reconstruction may visually shrink volume                                | Flora admits anisotropy only after an isotropic visual gate fails                                          |
| L7  | Direct local measurement                     | Same-worktree hidden release logs `re-flora-20260809-190550.223-71870.log` and `re-flora-20260809-192044.002-163398.log`                                                                                                                                                       | Matched old-bucket versus complete-frame publication frequency, burst size, copy throughput, and CPU publication time                                 | One RTX 3060 Ti/25k workload is not surface or consumer-PC proof                                           |
| E4  | Direct external research                     | [Lorensen and Cline 1987](https://www.cs.toronto.edu/~jacobson/seminar/lorenson-and-cline-1987.pdf), [DOI](https://doi.org/10.1145/37402.37422)                                                                                                                                | Triangle extraction from a sampled constant-density field with interpolated crossings and gradient normals                                            | Dense/sparse GPU layout, exact extractor, capacity, and timing are project decisions                       |
| E5  | Direct external research                     | [Chentanez and Müller 2010](https://matthias-research.github.io/pages/publications/hfFluid.pdf)                                                                                                                                                                                | Shallow-water grid plus particles for non-heightfield events, with mass/momentum exchange                                                             | Re: Flora's disjoint quiet-column/MLS-MPM parcel design is an adaptation, not their algorithm              |
| E6  | Direct external research                     | [Chentanez and Müller 2011](https://matthias-research.github.io/pages/publications/tallCells.pdf)                                                                                                                                                                              | Concentrating full 3D work near an interesting surface reduces large-water cost                                                                       | Active MLS-MPM patches are an analogous architectural inference                                            |
| E7  | Direct official engine documentation         | [Unreal Water meshing](https://dev.epicgames.com/documentation/en-us/unreal-engine/water-meshing-system-and-surface-rendering-in-unreal-engine?application_version=5.7)                                                                                                        | Shared tiled water surfaces, quadtree LOD/morphing, and a Fortnite example are production patterns                                                    | It is engine/game precedent, not Re: Flora performance proof                                               |
| E8  | Direct official technical presentation       | [GPU Gems, Chapter 1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models)                                                                                                                                | Uru used base-mesh geometric waves plus higher-frequency normal detail as convincing rather than rigorous physics                                     | Recommended only as cosmetic quiet-water detail; no performance transfer                                   |
| E9  | Direct external course notes                 | [Tessendorf, _Simulating Ocean Water_](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2002.pdf)                                                                                                                                                            | Spectral/FFT techniques model ocean-water environments                                                                                                | Deferring FFT for editable ponds is a product inference                                                    |
| E10 | Direct first-party production presentation   | [LIGHTSPEED STUDIOS, Photon Water System slides](https://media.gdcvault.com/gdc2023/Slides/Open-World%2BWater%2BRendering%2Band%2BReal-Time%2BSimulation_Mao_Zhenyu%26Wu_Kui.pdf) and [GDC session](https://www.gdcvault.com/play/1028829/Advanced-Graphics-Summit-Open-World) | Precomputed and runtime-updated height/velocity/foam data feed an adaptive water mesh with CDLOD acceleration                                         | It is production precedent, not proof for Re: Flora's solver split or budget                               |
| I1  | Recommendation / inference                   | This document                                                                                                                                                                                                                                                                  | Every visible pond should not be particle-backed; use disjoint quiet columns and active MLS-MPM patches, with spray visual by default                 | Must pass manual-patch, conservation, seam, and minimum-spec gates                                         |
| I2  | Recommendation / inference                   | This document                                                                                                                                                                                                                                                                  | Production active water should start with a bounded dense world-space scalar field and extracted mesh; screen-space is conditional                    | May be reversed only by a named failed gate, matched evidence, and an explicit decision                    |
| I3  | Recommendation / inference                   | This document                                                                                                                                                                                                                                                                  | Extend the existing full coherent frame with worker-captured physics/terrain metadata before surface work                                             | Exact payload ownership and bandwidth require implementation and release measurement                       |
| I4  | Recommendation / inference                   | Three independent Interface designs plus local code                                                                                                                                                                                                                            | Keep canonical `WaterWorld` ownership in the worker and derived generations behind one small deep `WaterSurfaceModule`                                | Exact Rust placement is implementation work; the ownership Seam is settled                                 |

## Final answer to the design question

The mixed-time prerequisite is already fixed: Re: Flora now publishes complete immutable particle
frames at one simulation time and retains the last complete frame without waiting. Before any
surface is built, that publication must also stamp the worker's matching particle measure, solver
bounds/cell size, representation generation, and applied filled-solid terrain dependency.

The practical continuous surface path is then one bounded dense world-space scalar field from the
coherent active particles, followed by a hidden baseline extractor and an atomically published
triangle mesh. This is the production target and first Implementation. Screen-space fluid is a
conditional diagnostic/fallback, not a mesh and not mandatory work; sparse bricks, worker scalar
fields, and anisotropic kernels are likewise admitted only by a named failed gate.

Large still ponds should be canonical conservative columns with a clipped/tiled heightfield surface
and zero fluid substeps while sleeping. Shallow-water transport is a later update mode for the same
columns, not a prerequisite. Only locally active, non-heightfield water needs MLS-MPM particles.

The decisive hybrid rule is exclusive ownership: the worker alone moves fixed particle mass and
momentum through a prepared atomic transaction against the matching filled-solid terrain state.
The selected small deep `WaterSurfaceModule` owns only complete derived render generations. The
phases therefore prove cross-source input, one dense active mesh, one particle-free quiet pond,
manual conversion, one conservative interface flux, and only then automatic wake/sleep and LOD.
