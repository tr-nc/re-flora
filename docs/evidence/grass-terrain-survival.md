# Grass terrain survival is independent of the edit brush

## Contract

- Root support voxel empty: remove grass before the completed terrain publication returns.
- Support present, 0–2 contiguous solid voxels immediately above the root: retain the existing grass instance.
- All three voxels immediately above the root solid: remove grass. More distant or non-contiguous overhead voxels do not count. Material state/moisture bits do not count as solidity.
- Applies to tall and short grass. Authored plants retain their separate existing removal policy. Explicit plant-removal tools still remove plants deliberately.
- Partial burial does not relocate the root, regrow grass on top of the deposit, or resurrect grass already removed. Terrain can occlude retained geometry until uncovered.

This supersedes the one-voxel grass burial rule in `soil-grass-preservation.md` while retaining that fix's prohibition on using the soil sampling halo as a deletion mask.

## Ownership and scheduling

Terrain publications report changed bounds. After physical terrain publication and before shadow/visible revision observers, a separate `SurfaceBuilder::reconcile_grass_terrain` GPU stage observes the current authoritative atlas. Candidate bounds extend three voxels downward so grass rooted in a lower chunk is checked when the third covering voxel lies in the upper chunk. Empty grass chunks are skipped.

The shader owns the support/three-layer predicate (`grass_survival.slang`); the terrain edit brush never decides which grass dies. Existing occupancy staging is reused, with validation independent of the shared surface staging texture and procedural density eligibility. Validation preserves growth/species/spawn state and does not republish authored-plant buffers. The atlas descriptor participates in each published descriptor generation, not just initialization.

Runtime terrain transactions now preserve existing flora instead of procedurally regenerating whole chunks. The legacy terrain brush publication no longer uses its capsule to clear grass. Soil's exact-write removal applies only to authored plants. A transient, non-serialized occupancy bit distinguishes established roots from new planting candidates, so Grow/Trim around a partially buried root cannot accidentally remove it for failing new-plant surface rules.

This is a separate **GPU stage**, not a new CPU thread/process or an asynchronous deletion queue. It runs on the existing ordered GPU submission path and completes before publishing the edit. Therefore it reads the latest atlas and has no delayed-delete snapshot that can race an excavation or world replacement. Snapshot replacement/startup preserve their saved flora rather than applying an edit policy. No latency or throughput improvement is claimed; asynchronous scheduling would be a separate measured change.

## Validation

```sh
cargo test visible_terrain
cargo test builder::surface
RUST_LOG=info RE_FLORA_SOIL_BAG_REVIEW=flora cargo run --release -- \
  --rooftop-poc --hidden --mute --auto-exit 0.5
```

- Native GPU replay: reference/fast Soil, single/spread deposition, grass at y=216 and y=255 across a vertical chunk boundary, 1/2/3-layer transitions, third layer written then cleared before observation, immediate support removal, Grow/Trim over two-layer burial, unaffected neighbor identity positions. All four cases pass. One-voxel and 64-voxel initial deposits now retain all 1,936 grass roots because neither fixture reaches three-layer cover.
- Production Slang CPU test `grass_survival_test.slang`: support absence, 0/1/2/3 layers, air gaps, material-state masking, and preservation of packed fields when the established-root staging bit is set.
- Publication-order tests verify grass observation occurs after physical terrain and before visible revision; loading/snapshot replacement does not invoke the edit observer.
- `cargo check`, Release build, relevant Rust tests and hidden muted run pass. Logs contain no ERROR/panic/VUID in successful final runs.
- Broad `cargo test surface` also selects four previously failing raster-tree tests; 84 pass / 4 fail. These failures are outside the changed grass code. Full Rust suite: 1,703 pass / 9 fail / 14 ignored, plus 4 passing library tests. The nine failures are the same two configuration-default and seven raster-tree failures recorded before this change.
- No user configuration or generated-file edits; no visible game launched automatically.
