# Grass-covered soil appearance

Dirt remains material 2. Ordinary tall/short grass gives its supporting surface a
continuous green patch; flowers and trees do not. Bare soil is brown. This is an
appearance cache, not a second block type, inventory item, or physical material.

## Ownership and rules

`src/builder/surface/soil_cover.rs` derives coverage from the live root identities
published alongside flora hydration. Plant additions/removals and snapshot restore
invalidate it through the shared flora publication boundary. Terrain publication
invalidates it even when no grass is removed. The app refreshes after planting,
mowing and terrain edits, before recording rendering commands; unchanged frames
perform no terrain readback or upload.

Each root covers a circular radius of four voxels on exposed Dirt at its support
height, or one voxel lower. It never reaches above the supporting height, so new
soil deposited on top does not inherit the green surface. The root itself must
have exposed Dirt support. A single layer of cover therefore hides the green
surface immediately, independently of the existing three-layer grass survival
policy. Uncovering a surviving root restores its patch. Overlapping roots are
combined before publication, including across chunk boundaries.

Atlas bit `0x40` caches this appearance on Dirt only. Type bits, moisture bits,
other materials, fixed-scene ownership, geometry and collision remain unchanged.
The read/modify/write path batches reads of bounded regions, then uploads only
changed regions. Previously affected regions are included when roots disappear.
All roots are currently considered on invalidation; very large planted worlds
have not been performance-qualified.

Snapshot terrain uploads discard the derived Dirt coverage bit. The restored
flora publication rebuilds it; legacy saves need no material/schema migration.
The old `voxel_dirt_color` setting and its saved green value remain intact, now
labelled **Grass-Covered Dirt Color**. The new **Bare Dirt Color** defaults to
`#785438`. Both use the existing declarative save/load path under Terrain.

Primary terrain shading and the glass fallback select the live color before
terrain variation (and primary shading's existing wetness response). DDGI retains
its dry-material approximation, now using bare brown soil for indirect transport:
live grass coverage, like moisture, is not frozen into the multi-frame radiance
build. This change does not claim grass-colored indirect bounce.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo build --release` passed.
- Four deterministic soil-cover unit tests cover overlap/removal, exposure,
  deposition above roots, moisture preservation, other materials and region
  planning across all three chunk boundaries.
- Native acceptance exercises actual atlas transfers and GPU flora removal,
  tall/short roots, flower exclusion, overlapping and last-root removal,
  cross-chunk footprints, one-layer burial/uncovering, and flora JSON round-trip
  plus terrain restore. It restores the original world afterward.
- Final full `cargo test`: 1716 passed, 9 failed, 14 ignored in the app suite.
  All nine remaining failures were reproduced on unmodified `9e2b7977`: two
  saved-default expectations (apple size and sprinkler sampling), and seven
  `tracer::raster_tree` tests. The palette ABI-size assertion was updated for the
  new color; capture serialization now uses the fixture's fixed model identity
  while independently asserting the live shader identity.
- Hidden muted Release native review and screenshot passed with no ERROR/panic/
  VUID, and shutdown `failures=0`. Screenshot inspected at
  `target/grass-soil-visual.png`: contiguous green soil below the planted patch,
  brown surrounding bare ground. The visual seed uses the production planting
  path and adds 1014 ordinary roots.
- Diagnostic Release timings for that patch: 53,808 uploaded bytes, roughly
  3.8–5.2 ms for the invalidated coverage update in these runs. These are isolated
  observations, not a comparative benchmark or large-world performance approval.

Reproduce native acceptance:

```sh
RE_FLORA_GRASS_SOIL_COVER_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 0.5
```

Reproduce the planted visual fixture without changing saved defaults or saves:

```sh
RE_FLORA_GRASS_SOIL_COVER_REVIEW=visual cargo run --release -- \
  --hidden --mute --screenshot garden-arrival target/grass-soil-visual.png \
  --screenshot-delay 2 --auto-exit 3
```

Use `--latest-log` / `--tail-latest-log 200` in the same worktree. Optional
`RUST_LOG=info,re_flora::builder::surface::soil_cover=debug` prints update costs.
On systems with an unavailable Wayland settings portal, `env -u WAYLAND_DISPLAY`
selects the existing X11 path; the first ordinary Wayland smoke encountered a
portal color-scheme timeout, but the subsequent X11 review runs were clean.
