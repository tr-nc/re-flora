# Spatial ecology → an editable Restorative Garden opening

Research handoff for **garden delivery scope v1**, world slice. Repository inspected at
`a66d9c5144b4a687a4e0c3c7cfa5f8ae7b6cc527`, branch `agent/garden-world`.
**Status: proposal, not implemented or visually accepted.** Only this note changes in this
phase. No compilation, tests, game, GPU, preferences or saved worlds were touched.

## Recommendation in brief

Compose a small garden first; use ecological spatial patterns to soften it, not to replace
composition with noise. An open, gently curved starting clearing leads through low meadow
into two unequal tree groups, with flower drifts on their sunny margins and a second
plantable pocket visible beyond. Keep the foreground low, a diagonal sightline open and
some intentionally empty soil/short-grass patches. The opening is an initial condition
made of ordinary editable terrain and plants, not a protected showroom or an ecological
simulation. Existing saved gardens remain authoritative.

Use a versioned deterministic recipe: authored masks + low-frequency habitat suitability +
species-specific cluster centers + bounded, independently hashed candidates. No universal
species attraction/repulsion matrix; no continuous regeneration or automatic reclamation of
player edits. All numerical layout/population values below are **art/gameplay starting
points**, not ecological constants or established performance budgets.

## Primary evidence and its limits

Four primary empirical papers inform different parts of the recipe. The evidence is mostly
from tropical woody communities, **not a validation of temperate ornamental meadow mixtures**.
Transfer pattern vocabulary and caution, not species parameters, forest densities, meters,
causal mechanisms or claims of biodiversity benefit at this game's scale.

### [1] Conspecific aggregation is common, but not a uniform lattice or one universal scale

Condit, R. et al. (2000). *Spatial Patterns in the Distribution of Tropical Tree Species*.
**Science 288**, 1414–1418. [DOI](https://doi.org/10.1126/science.288.5470.1414).
Primary abstract retrieved from [publisher-deposited Crossref record](https://api.crossref.org/works/10.1126/science.288.5470.1414).

- Fully mapped 25–52 ha plots at six tropical forest sites; analysis of 1,768 tree species.
  The abstract reports nearly every species aggregated relative to random when stems
  above 1 cm diameter are included; the pattern mostly persists for stems ≥10 cm.
  Rare species were more aggregated than common ones.
- **Use:** repeated local groups of the same species are defensible visual vocabulary;
  a uniform independent draw of species at every point is not the only natural-looking
  baseline. Keep both dense and sparse areas and allow different scales among species.
- **Do not infer:** all plants must form tight circular clusters; aggregation alone proves
  cooperation, limited dispersal, or one optimal density. This source was checked at
  abstract level, not full-method level.

### [2] Dispersal relates to clustering; cluster models are a useful approximation

Seidler, T. G. & Plotkin, J. B. (2006). *Seed Dispersal and Spatial Pattern in Tropical Trees*.
**PLOS Biology 4**, e344. [Full paper](https://journals.plos.org/plosbiology/article?id=10.1371/journal.pbio.0040344),
[DOI](https://doi.org/10.1371/journal.pbio.0040344).

- Results/Figs. 1–4: 561 species at Pasoh, Malaysia; dispersal syndromes correlate with
  fitted cluster size. Ballistically dispersed species tend toward tighter clusters,
  animal-dispersed species toward more diffuse ones. A BCI analysis gives qualitatively
  similar results. The paper reports a stronger dispersal-pattern relationship at small
  than large spatial scales, with no significant syndrome relationship at ≥200 m there.
- Methods: a Poisson cluster process uses centers with radially symmetric Gaussian
  offspring; parameters are fitted to spatial statistics. Discussion explicitly acknowledges
  coarse syndrome categories, recruitment/establishment limitations and potential covariates.
- **Use:** separate cluster-center spacing from within-cluster spread; give each plant
  archetype its own spread and a small outlier component. A finite cluster recipe is simpler
  than neighbor-by-neighbor attraction.
- **Art approximation:** elliptical, overlapping flower drifts, truncated kernels and
  sparse outliers improve readability and bounded generation. We are not reproducing the
  fitted Poisson process or mapping its forest distances into the garden.

### [3] Habitat filtering must not be confused with clustered sampling

Harms, K. E., Condit, R., Hubbell, S. P. & Foster, R. B. (2001).
*Habitat associations of trees and shrubs in a 50-ha neotropical forest plot*.
**Journal of Ecology 89**, 947–959.
[DOI](https://doi.org/10.1111/j.1365-2745.2001.00615.x);
[primary abstract via Europe PMC, AGR IND23300490](https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=EXT_ID:IND23300490%20AND%20SRC:AGR&format=json&resultType=core).
The older `10.1046/j.0022-0477.2001.00615.x` resolves in metadata to the DOI above.

- Abstract: 171 species at BCI. Tests accounting for spatial structure find many fewer
  significant habitat associations than inappropriate independent-stem chi-square tests.
  For the low plateau, 9 species were positively and 19 negatively associated, leaving
  143 not biased; even for the distinctive swamp, more than two-thirds had neither sign.
- **Use:** make broad habitat weights optional/soft for most archetypes; retain overlap.
  A slope, height or canopy proxy can bias placement, but must not make every species a
  narrow habitat specialist. Reject physically invalid surfaces separately from preference.
- **Do not infer:** topographic height is actual soil moisture, shade is measured light,
  or co-occurrence establishes a plant interaction. Source checked at abstract level;
  publisher full text was access-blocked in this session.

### [4] Co-location/segregation is not universal interspecific attraction/repulsion

Wiegand, T., Gunatilleke, S. & Gunatilleke, N. (2007).
*Species Associations in a Heterogeneous Sri Lankan Dipterocarp Forest*.
**The American Naturalist 170**, E77–E95. [DOI](https://doi.org/10.1086/521240);
[primary abstract via Europe PMC, PMID 17891727](https://www.ebi.ac.uk/europepmc/webservices/rest/search?query=EXT_ID:17891727%20AND%20SRC:MED&format=json&resultType=core).

- Abstract: mapped 500 × 500 m plot, 46 common tree species above 10 cm DBH. First-order
  segregation/overlap was strong: 53% of species pairs largely disjoint, 40% partial
  overlap, 6% overlap (rounded reported values). After heterogeneous Poisson null models
  accounted for environmental heterogeneity, only 5% of pairs had significant second-order
  effects. Reported interactions were mostly within 2–4 m and disappeared within 15–20 m.
- **Use:** shared habitat masks can yield mixed groups; different masks can yield separated
  groups without encoding pairwise animosity. Default to no named species interaction.
  Use local minimum spacing only for collisions/readability and population safety.
- **Do not infer:** these percentages or interaction distances apply universally; the
  spatial analysis proves mechanisms of attraction/repulsion. Source checked at abstract
  level; no full-text methodological claims are made here.

Retrieval evidence is under `target/garden-delivery/world/`: PLOS full HTML plus extracted
text, Crossref records and Europe PMC primary-abstract records. Publisher/PubMed HTML
attempts returned access/challenge pages; they are not evidence of having read full papers.
The durable findings, source IDs, qualifications and links are preserved here; the local
retrieval cache is untracked and is not required to interpret this note.

## Opening composition: intentional space before stochastic detail

Use normalized coordinates over the actual editable XZ extent, with a safety inset for
root/canopy bounds. The current default editable region is 512 × 512 × 512 voxels,
`1 world unit = 256 voxels` (not an ecological meter). Resolve actual player capsule and
mature tree bounds before finalizing spacing. A proposed layout, to rotate/translate into
alignment with the fresh-opening camera:

- **Arrival:** slightly off-center oval clearing around `(0.36, 0.72)`, radii roughly
  `(0.13, 0.10)` of map width/depth. Soft short-grass edges and visible soil opportunities,
  not a bare geometric disk. No trunk, tall flower or collidable prop at spawn.
- **Invitation:** a curved low-vegetation corridor sweeps toward `(0.61, 0.43)` and a
  second smaller planting pocket. Its *usable* width must exceed several capsule diameters,
  including tree collision margins; do not rely on fly mode to make the garden accessible.
  Reserve roughly 20–30% of editable surface as clearing/corridor/plantable gaps.
- **Tree framing:** a larger uneven group on one rear/side flank and a smaller group on
  the other, not a symmetrical avenue or ring. Initial target five trees (three plus two),
  cap six; revise counts downward if mature canopies overwhelm the small domain. Stagger
  depths and silhouettes; keep the main diagonal open to a low distant rise/sky window.
- **Flower drifts:** 4–6 overlapping elongated lobes along meadow margins, two or three
  dominant colors/species with different spread; a few outliers bridge groups. Leave a
  quiet foreground. Avoid every species appearing in every drift, evenly spaced bouquets,
  perfect discs, straight color bands or flowers covering every plantable patch.
- **Terrain:** a shallow clearing saddle, two broad unequal rises and a low transition
  between them. Start with about 12–24 voxels of broad relief, not high-frequency crags;
  keep primary route slope around ≤10–12 degrees as an artistic target and eliminate
  capsule-blocking steps. Blend continuously into surrounding terrain without abrupt mask
  edges. Low-amplitude detail is subordinate to traversal and readable surfaces.
- **Layers:** near short grass/soil, mid-height flowers, rear tree masses and exposed sky.
  A few grasses can soften a path edge but not conceal the player's first place to plant.
  Lighting/weather preferences are not silently changed to make screenshots attractive.

These are candidate values for screenshot iteration, not a claim that a mathematically
specified layout is already beautiful. The composition must remain readable from ordinary
walking eye height, not only an aerial camera.

## Simple deterministic bounded generation recipe

1. **Resolve launch policy once.** Produce `FreshGarden(recipe_version, seed)` only for a
   fresh ordinary Garden launch with procedural terrain and no explicit load/fixture.
   Loaded snapshots, flat terrain and diagnostic scenes use their existing paths. Never
   infer fresh-generation eligibility just from `LoadingDirective::Garden` (see below).
2. **Author masks.** A pure layout plan produces clearing, corridor, tree-group and drift
   masks in normalized XZ. Protect the arrival capsule volume and a connected corridor.
   Terrain uses broad analytic lobes blended with a small deterministic detail field;
   initialization writes normal dirt/rock voxels, not a non-editable display mesh.
3. **Make habitat proxies.** Query realized terrain height/slope/material. Use planned
   canopy influence and broad topography as *shade/moisture proxies*, not new live water
   state. Clearing exclusion wins over suitability. Most species retain a nonzero baseline
   weight on valid soil; steep/invalid surfaces are hard rejects. Actual irrigation and
   future plant care continue to use existing owners.
4. **Generate drifts.** For each fixed species key, allocate a few centers within eligible
   authored lobes, sample finite elliptical offsets around centers, then accept according
   to suitability × drift falloff. Most candidates belong to a drift; e.g. a 10–15% sparse
   outlier budget gives connectivity. Neighbor species share only selected masks, not
   universal pairwise attraction/repulsion. Tree centers follow the asymmetric group plan
   with canopy/trunk clearances; they are not drawn from the dense flower sampler.
5. **Use stable random addressing.** `hash(seed, recipe_version, stable_species_key,
   cluster_id, candidate_id, property_salt)` drives every decision, including tree shape,
   orientation and age variation. Use a specified integer hash/PRNG, not `rand::rng()`,
   GPU append order or mutable Debug tree settings. Stable iteration and rejection order
   make the same recipe reproducible. Do not assert bit-identical cross-GPU floating-point
   output without evidence; CPU integer-coordinate candidates help bound that uncertainty.
6. **Bound work and population.** Starting targets: five trees/cap six, 4–6 drifts and
   roughly 120–240 authored flower/ornamental plants total; cap 256 additions and use a
   fixed candidate-attempt ceiling (e.g. 12 per budgeted plant). Ordinary grass has a
   separate density/capacity budget. Stop early rather than filling failed quotas forever.
   Reuse species spacing, authored-placement checks and spatial bins; avoid an O(N²)
   species force solver. Keep roots/canopies within world bounds and outside the arrival
   volume. Read actual allocator/instance limits before implementation: these starting
   targets do not override existing capacities or establish a measured frame budget.
   `src/builder/surface/resources.rs` currently defines 40,000 flora instances per species;
   that storage ceiling is not a sensible target density or a guaranteed performance budget.
7. **Commit to the editable world once.** Place through existing flora/tree transactions,
   batch work where available, then let normal growth, edits, water and saves own the result.
   No per-frame reseeding, no generation on routine terrain publication, no refill of
   mowed/dug/cleared plants. Log seed/version, eligibility, counts, rejections, attempt count
   and a stable layout fingerprint for later verification.

### Seed/config ownership

One small `garden_opening` layout module should own a versioned recipe and its deterministic
seed (fixed default is sufficient initially). Do not add a second seed to unrelated live
render settings or reuse a user's tree-preview seed. If seed selection is genuinely needed,
put it in the startup plan and log the resolved value; a runtime regeneration slider is
unnecessary. Any later saved control must follow `docs/agents/gui-settings.md` and remain
additive. Terrain and vegetation consume the same resolved opening plan; shader constants
must not become an independently maintained layout authority.

Snapshots already serialize realized terrain, flora and trees, so recipe changes must not
regenerate loaded worlds. Persisting optional recipe metadata later is provenance only,
not a license to replace snapshot contents or require migration of old worlds.

## Read-only repository findings and implementation seams

| Owner/files inspected | Current behavior / recommended boundary |
| --- | --- |
| `src/cli.rs`; `src/builder/plain/mod.rs` | `Scenario` selects Garden and special scenes; `TerrainGeneration` currently has Procedural/Flat. CLI rejects flat with loaded terrain or fixed scenes. Add/resolve explicit fresh-opening eligibility in startup planning; preserve `--flat-terrain` and its soil-layer semantics. Prefer a separate opening generation variant/plan instead of changing all legacy Procedural consumers. |
| `shader/slang/terrain_height.slang`, `chunk_heightmap.slang`, `chunk_init.slang`; `PlainBuilder::chunk_init_with_terrain` | Height field is fixed-seed multi-octave noise (42/999/5555), base around 0.5, relief scale 0.85. `computeSurfaceHeight2D` computes island shape but returns base shape, not island masking. Chunk initialization uses heightmap and ordinary dirt/rock layers. Scope the new composed height to fresh-opening initialization, not a global edit of the height function used by other callers. Shader push-constant changes require generated structs from `cargo check`, never manual generated edits. |
| `src/app/core/launch_owners.rs` | **Important trap:** Garden loading directive is also returned for hybrid tests, water edit-soak, lighting acceptance, connectivity, and some specialized launch modes. Gate the new opening on actual ordinary scenario/launch identity plus persistence/generation mode, not this broad directive. Add one semantic eligibility method/plan field here rather than spreading guessed exclusions through App. |
| `src/app/core/loading.rs`, `visible_terrain.rs` | Terrain snapshot bytes supersede procedural initialization. Fresh special scenes can stamp their terrain after initialization. Startup Visible Terrain Publication owns build/collider readiness. Seed opening vegetation only after terrain queries/colliders are ready, once, and before startup save/completion; do not bypass publication or mutate under a loaded garden. |
| `src/app/core/terrain_persistence.rs`; `src/terrain_persistence.rs` | Explicit startup load is validated, garden section decoded and terrain bytes uploaded; startup tuned tree is suppressed, then `restore_startup_garden` restores trees/flora/growth override without trunk restamp. Runtime replacement restores exact flora instead of procedurally regenerating it. Preserve this precedence, IDs and error behavior. Default snapshot filename is an operation target, **not evidence of automatic startup loading**. Never rewrite saved worlds/preferences as a migration. |
| `shader/slang/flora_placement.slang`, `flora_instance_placement.slang`, `src/flora/species.rs` | Existing natural surface placement hashes voxel XZ, adds density/biome noise and slope/material gating; selection here yields tall/short grass, not authored flower drifts. Preserve brush/legacy semantics. Apply opening masks at a fresh-start-only seam or a one-time initial placement pass, not globally to future Free-form Planting or terrain edits. Species already own authored soft spacing (model flowers default to 20 voxels), grass influence and moisture growth factors; reuse these and the existing candidate scoring rather than invent a second live competition system. |
| `src/app/core/vegetation.rs`, `vegetation/` | Ordinary startup currently stamps one tuned Debug tree. `generate_procedural_trees` is marked dead-code, clears/replaces trees and uses random shape seeds: **do not call it as the new opening generator**. Reuse `apply_tree_placement`, terrain-height queries, `try_place_authored_flora` and batch completion (showcase demonstrates placement), not destructive clear helpers or UI brushes whose serial/seed depends on session history. Preserve canopy diagnostics and their tuned/stress trees. |
| `src/app/camera_snapshots.rs`, `src/app/core/mod.rs`, `src/gameplay/camera/{controller,desc}.rs`, `src/app/core/physics.rs`, `src/app/core/physics/surface_motion.rs` | Authored player-default pose is approximately `(0.632, 0.782, 1.556)`, yaw 30.08°, pitch −18.35°, FOV 60; not a terrain-aware spawn recipe. Camera setup precedes terrain settlement; named snapshots and fixtures can override it. Walk camera height is 0.08 world units; the player collision capsule has radius 4 and half-height 8 voxels. For a fresh opening only, derive eye Y from realized ground plus capsule/camera clearance, and orient into clearing. Do not ground-snap explicit screenshots/saved camera choices or change global camera snapshot defaults to move only the new scene. |
| `src/cli.rs`, `src/app/core/screenshot.rs`, `src/app/core/launch_owners.rs` | Existing `--screenshot <preset> <path> --screenshot-delay <sec>` supports hidden actual render captures. Preserve explicit preset behavior; use dedicated review poses without writing user snapshot libraries. |

**Shared-file recommendation:** serialize controller/world edits to `cli.rs`,
`app/core/mod.rs`, `loading.rs`, `launch_owners.rs`, `vegetation.rs` and builder interfaces.
World should primarily own a new pure layout module, a narrowly gated terrain-initialization
adapter and startup placement adapter. Audio should consume existing placed-tree/sprinkler
lifecycle, not own layout. Wetness should consume existing moisture, not the generation
habitat proxy. No source changes are made or requested from other workers in this phase.

## Later acceptance: real behavior and screenshots, not a noise-map preview

Research is complete only as a recommendation; implementation acceptance remains open.
Under the common heavy-resource lock, later implementation must:

1. Test pure generation determinism, bounds, maximum attempts/counts, connected clear route,
   minimum spacing and seed variation; cover an all-rejected habitat without hanging.
   Compare tree descriptors/flower anchors and counts, not just a seed log line.
2. Exercise the policy matrix: fresh ordinary Garden gets the opening; explicit saved world
   gets exactly its terrain/flora/trees; runtime load does not regenerate; Flat, house,
   rooftop, water experience, glass/hybrid/environment, connectivity, lighting/foliage and
   canopy diagnostic paths keep their intended fixtures. Include explicit camera override
   and empty/minimal valid saved gardens. Work on temporary copies only.
3. Build/check/generated output normally, then hidden muted Release startup and log inspection.
   Walk the arrival and corridor with real collision queries; verify no floating/embedded
   plants, trapped spawn or corridor requiring flight/jump. Dig, plant, mow and save/reload
   temporary copies to establish ordinary editability and no automatic repopulation.
4. Capture actual hidden native-render PNGs with identical render extent/settings: default
   arrival at player scale; looking down corridor; reverse view from the second pocket;
   oblique overview. Inspect the files themselves. Record seed/version, pose/FOV, settings,
   counts and capture paths under `target/garden-delivery/world/`. A top-down placement
   plot is useful debugging but does not satisfy visual acceptance.
5. Refine based on those images: is the first patch usable and legible, are color masses
   coherent, do trees frame rather than wall off the view, is there near/middle/far depth,
   are clearings truly open, do slopes look gentle at walking height? Save at least one
   candidate/revision comparison. No screenshot or aesthetic-success claim is made now.
6. Measure resulting population cost in matched Release runs after visual candidate review;
   no inference from unit-test timings. Basic crash/Vulkan/resource safety is mandatory;
   a modest performance miss does not erase the value of showing an honest candidate.

Visual approval remains the player's decision. Nothing here promises ecological succession,
correct real-species niches, habitat restoration, universal plant compatibility, or a
biodiversity simulation. The intended result is a coherent, welcoming **Restorative Garden**
that leaves room for **Free-form Planting** and the player's **Personal Garden Story**.
