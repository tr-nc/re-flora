# Grass-covered soil appearance

Dirt remains material 2. Only ordinary tall/short grass changes its surface color;
flowers and trees do not. Bare soil is brown; grass-covered soil retains the old
green palette. Moisture, collision, geometry and inventory behavior are unchanged.

## Current design

Grass stems are one voxel wide at their base. Color covers their rooted cell and
one-cell *interior* gaps bracketed by opposite live roots in the same horizontal
plane. It does not dilate the grass outline or climb onto newly deposited soil.
A single covering layer hides the green immediately, independently of the
existing three-layer grass survival rule. Uncovering surviving grass restores it.

`surface/soil_cover` owns a root index, spatial tiles and pending destinations.
Flora publication supplies only changed ordinary-root identities, independent of
growth/hydration/draw order. Duplicate species at a root are combined. Terrain
publication queries nearby indexed roots only; unrelated grass is not revisited.
Restore invalidates appearance even when root identities are unchanged.

CPU upload is a sparse list of destinations plus nine neighboring root membership
bits. The GPU samples actual terrain support/headroom in one pass, then updates
only Dirt's `0x40` appearance bit in a second pass. All exposure reads complete
before any writes: no read/write race across neighboring atlas cells. Destinations
are unique, and each write preserves current moisture and all other data. There
is no terrain readback or region copy/upload in this path. A bounded managed GPU
job completes before rendering; unchanged publications issue no job.

Snapshot terrain upload discards this derived bit, then live roots regenerate it.
No material/schema migration is needed. Saved **Grass-Covered Dirt Color** retains
`voxel_dirt_color`; **Bare Dirt Color** defaults to `#785438`. Both use the common
saved Terrain controls. Primary terrain and glass fallback select these colors;
DDGI retains dry bare-soil indirect transport (no grass-colored bounce claim).

## Regression and evidence

The first version, `ebd7c0f5`, recomputed every root on any publication, read back
whole bounded regions, and dilated each root by four cells. This caused both the
reported editing hitch and the broad green border. A Release native fixture
reproduced the scaling failure by toggling exactly one root in an otherwise
unchanged garden:

| Ordinary roots | Old coverage median / max | Sparse GPU median / max |
|---|---:|---:|
| 1,024 | 3.290 / 3.353 ms | 0.447 / 0.858 ms |
| 65,536 | 202.454 / 206.667 ms | 0.854 / 1.138 ms |

Each row has 12 alternating one-root add/removes. Dense old updates uploaded zero
changed terrain bytes yet still spent ~200 ms. New one-root updates touch nine
cells, upload 144 bytes, and read back zero terrain bytes. These timings measure
coverage refresh after root publication, not total planting/meshing/frame time.
Cold full-world reconstruction still scales with the changed world; not claimed
constant-time. GPU waiting/scheduling can affect the small-job timings.

- `cargo fmt --check`, `cargo check`, focused soil-cover tests, Release build pass.
- Native acceptance passes: tall/short grass, flower exclusion, cross-chunk
  interior gaps, strict exterior boundary, last-root removal, surviving roots,
  moisture, one-layer burial/uncovering, and JSON plus atlas restore.
- Pure tests prove one-root invalidation remains nine cells with 65,536 distant
  roots, no-op publication does no work, and headroom/tile boundaries are handled.
- Hidden muted Release benchmark/review logs have no ERROR/panic/VUID. Inspected
  `target/grass-soil-tight.png` shows grass meeting bare soil without the old halo.
- Prior full-suite baseline has nine unrelated failures (seven raster-tree tests,
  apple size default, sprinkler sampling default); those were reproduced on
  `9e2b7977`. No claim that the entire existing suite is green.

## Reproduction

```sh
RE_FLORA_GRASS_SOIL_COVER_BENCH=1 RUST_LOG=info,re_flora::builder::surface::soil_cover=debug \
  cargo run --release -- --hidden --mute --auto-exit 0.5

RE_FLORA_GRASS_SOIL_COVER_REVIEW=visual cargo run --release -- \
  --hidden --mute --screenshot garden-arrival target/grass-soil-tight.png \
  --screenshot-delay 2 --auto-exit 3
```

The benchmark/native acceptance restore the original terrain/flora. `visual`
additionally plants a 1014-root patch through the real planting path for capture;
it does not save it or modify default settings. Inspect logs with `--latest-log`
and `--tail-latest-log 200` from the same worktree. If the Wayland settings portal
is unavailable, use `env -u WAYLAND_DISPLAY` for the existing X11 path.
