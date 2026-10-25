# Root-fed plant water storage

This supersedes the instantaneous soil-to-wilt mapping in [plant-water-status.md](plant-water-status.md). It is a stylized gameplay model, not a calibrated hydraulic model.

## Evidence and simplification

- Oregon State University Extension, [Soil-plant-water relationships](https://extension.oregonstate.edu/catalog/em-9544-soil-plant-water-relationships): roots extract plant-available soil water, availability falls as soil dries, and water leaves plants through transpiration. Full text was retrieved during the design discussion.
- Vogel et al. (2017), [A Simple Representation of Plant Water Storage Effects in Coupled Soil Water Flow and Transpiration Stream Modeling](https://doi.org/10.2136/vzj2016.12.0128): whole-plant storage/capacitance buffers uptake versus transpiration. Search/abstract evidence only; the publisher blocked full-text retrieval. Its spruce parameters are NOT used for grass or flowers.
- US Forest Service, [Hydraulic redistribution of soil water by roots affects whole-stand evapotranspiration and net ecosystem carbon exchange](https://research.fs.usda.gov/treesearch/35144): reverse root-mediated redistribution exists. Deliberately excluded here; no plant-to-soil backflow is simulated.

The sources motivate storage and distinct uptake/loss, not the numerical defaults below. No atmospheric humidity, stomatal physiology, disease, mortality, species-specific capacity, foliar uptake, offline aging, or multi-voxel root network is modeled.

## Contract and ownership

Each ordinary grass and modeled flower instance has a normalized water store H in [0,1]. The eight-byte vertex instance stride is unchanged; an independent float buffer publishes the CPU simulation state to GPU consumers. Trees and apples are outside this feature.

The simulation samples exactly the attached/base terrain voxel, not the visible root one voxel above it and not spray/leaf contact. All plants on one receiver share its supply. It runs for all populated chunks regardless of camera visibility, on world-running time, with a 0.25-second cadence. Sub-cadence elapsed time is saved. No paused/offline catch-up occurs.

For locally constant coefficients:

- a = normalized available root-zone water / Root Uptake Time
- b = 1 / Transpiration Time
- dH/dt = a(1-H) - bH
- equilibrium = a/(a+b)
- alpha = 1-exp(-(a+b)dt)

The implementation integrates both storage and water flows analytically, then apportions limited shared soil supply without draw-order dependence. Soil availability is reevaluated in steps no longer than 1/16 second. Zero supply uses the equivalent exact exponential loss. This is a time-based model, not a fixed alpha per rendered frame.

Current water-related growth potential and wilt geometry both consume H. The existing palette/soil-material wetness effect remains a separate material concern. Wilt begins below an adjustable healthy threshold, with squared normalized deficit. Wilt Strength only scales presentation; disabling it does not stop uptake or transpiration.

## Coarse-soil conservation boundary

The atlas exposes only four moisture levels. Tracking a fractional *debt* while leaving a complete band available to the old diffusion pass would permit that pass to move water already consumed. Instead a root receiver debits a whole atlas band before fractionally consuming it. The remaining reserved water is shared by all plants at that root voxel and serialized separately. Neighboring root voxels do not share it; no plant can spend another plant's private copy of the same soil band.

One atlas band represents 32 gameplay water units; a fully hydrated plant holds one unit. This is an authored scale, not a botanical measurement. Reserved water is already withdrawn from freely moving atlas water: it no longer participates in the existing soil evaporation/spread passes and is consumed through plant uptake/transpiration. It can sustain a root even after visible atlas moisture becomes zero. This root-zone buffer is an explicit approximation, NOT a continuous high-resolution replacement for the entire soil simulation. Removing the last plant does not refund water; the reservation remains associated with that root location. Unsupported non-soil samples discard the corresponding reservation.

External wetting, drying and diffusion continue to operate on unreserved atlas bands. Thus they cannot duplicate an already withdrawn band. Soil graphics can show a band decrement before the entire reserved band has entered plant tissue. A fully continuous soil renderer/transport model would be a separate change.

## Lifecycle and persistence

- New plants start with H=1: their initial tissue water is part of the planting input. Legacy saves with no water section also initialize H=1, rather than inventing an unknown prior drought history.
- Publication remaps by species and root, not draw index. Stream compaction, growth updates, cached/noncached rendering and ordinary rebuilds preserve storage.
- Actual removal drops the live plant state. Replanting starts a new store. Reversible removal receipts retain removed plants' water without rewinding survivors.
- Garden snapshots serialize species-keyed plant stores, root-zone reservations and the fractional simulation clock. Load validates bounds, identity coverage, duplicate entries and finite quantities before restoration. Registry migration follows stable species names.

## Controls

Debug Panel → Flora → Ground Plants → Water Response:

| Control | Default | Range |
|---|---:|---:|
| Grass & Flowers: Root Uptake Time (s) | 8 | 0.1–120 |
| Grass & Flowers: Transpiration Time (s) | 120 | 1–1200 |
| Grass & Flowers: Wilt Start Hydration | 0.65 | 0.05–1 |
| Grass & Flowers: Wilt Strength | 1 | 0–1 |

These are time constants, not times to complete filling/emptying. All are declarative saved controls. Values are gameplay tuning, not literature-derived constants.

## Validation and limitations

- Rust behavioral tests: dry exponential retention, gradual wet recovery, mass balance, scarcity/order invariance, reserved-band nonduplication, elapsed-time partition equivalence, old-save defaults, serialization and malformed state rejection.
- Production Slang CPU test covers healthy thresholds, deficit monotonicity, rooted/bounded geometry over 64 seeds and strength endpoints.
- Native acceptance (`RE_FLORA_GARDEN_SNAPSHOT_SMOKE=seed RE_FLORA_PLANT_HYDRATION_REVIEW=1`) uses actual soil uploads, grass and flower instances, GPU state publication, save/restore, draw reorder, remove/replant, and reversible removal. Logs `PLANT_HYDRATION_REVIEW passed=true`.
- A temporary real garden file was saved and loaded in a new process. `RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify` checked initial load and two runtime replacements: `plant_water=exact`, terrain/trees/flora exact, no duplicates, water/world updates resumed.
- Full Rust suite: 1712 passed / 9 failed / 14 ignored. The same two saved-default assertions and seven raster-tree failures were observed before this change; no all-green claim. Targeted flora/surface tests were rerun after final shader/persistence changes.
- Release build and hidden muted app runs passed. No visible game was launched automatically. Native acceptance and ordinary startup are not a substitute for player assessment of recovery timing.
- Release measurement, ordinary 130,371-plant dry garden: approximately 3 MB of bounded soil readback per update. Initial receiver index construction took about 18 ms; subsequent updates about 4–10 ms, once per 0.25 seconds, including GPU synchronization. This is not an all-wet stress benchmark or a performance acceptance claim. The authoritative CPU implementation has periodic synchronization cost; any future GPU implementation must preserve root grouping, water accounting and persistence rather than reverting to render-driven state.

Example native commands (temporary files only):

```sh
RE_FLORA_GARDEN_SNAPSHOT_SMOKE=seed RE_FLORA_PLANT_HYDRATION_REVIEW=1 \
  RUST_LOG=info cargo run --release -- --hidden --mute \
  --terrain-save /tmp/re-flora-hydration-acceptance.rflora --auto-exit 0.5
RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-load /tmp/re-flora-hydration-acceptance.rflora --auto-exit 0.5
```
