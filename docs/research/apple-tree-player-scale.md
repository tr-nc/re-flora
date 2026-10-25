# Apple-tree height relative to the player

Research date: 2026-10-25. Measured code/config baseline: `e0b69371`.

**Implementation follow-up:** the user subsequently approved resizing. See
[human-scale Apple delivery evidence](../evidence/apple-tree-player-scale.md) for the implemented
defaults and current measurements. The research/baseline observations below remain historical.

## Conclusion and scope

For this small, approachable garden, a **managed dwarf apple tree around 2.5–3.0 m tall**
is a defensible visual target. This is an art-direction recommendation, not the one natural
height of all apple trees. Full-size, non-dwarfing apple trees can legitimately be much taller.

Current geometry is substantially larger relative to the player, particularly the configured
planting tree. This investigation changes **no runtime tree parameters, opening positions,
camera/player settings, save files, or generation rules**. It does not implement or visually
approve a smaller candidate.

## Horticultural evidence

Primary sources read on the research date:

1. [Royal Horticultural Society: Rootstocks for fruit](https://www.rhs.org.uk/fruit/fruit-trees/rootstocks).
   Its apple table explicitly gives ultimate dimensions **when trained as a bush**:

   | Rootstock | Stated height | Context |
   |---|---:|---|
   | M27 | 1.2–1.8 m | Extremely dwarfing |
   | M9 | 1.8–2.4 m | Dwarfing, suitable for small gardens |
   | M26 | 2.4–3.0 m | Dwarfing |
   | MM106 | 3–4 m | Semi-dwarfing; not suitable for small gardens |
   | MM111 | 4–4.5 m | Vigorous; smaller on light soils |

   These are managed forms on particular rootstocks, not unpruned species maxima. The same
   table gives M26 a spread of 3.6 m: a low, spreading tree is not inherently unrealistic.
   It also specifies permanent support for M9/M26; choosing their approximate height does
   **not** mean the game's unsupported procedural tree is an exact horticultural simulation.

2. [Iowa State University Extension: Growing Apples in the Home Garden](https://yardandgarden.extension.iastate.edu/how-to/growing-apples-home-garden),
   **Rootstocks** section. Its broader categories are:

   | Category | Source height | Converted height (`1 ft = 0.3048 m`) |
   |---|---:|---:|
   | Dwarf | 8–10 ft | 2.44–3.05 m |
   | Semi-dwarf | About 15 ft | About 4.57 m |
   | Non-dwarfing/seedling rootstock | 25–30 ft | 7.62–9.14 m |

   The source explains that cultivar vigor affects final size and recommends dwarf/semi-dwarf
   rootstocks for home gardens because smaller trees are easier to maintain and harvest.
   Do not combine these broad categories with the RHS trained-form table as though they
   describe identical rootstocks, management, soil, or regional conditions.

3. [RHS: How to grow apples](https://www.rhs.org.uk/fruit/apples/grow-your-own).
   Rootstock controls vigor and size; trained forms and pruning can further restrict growth.
   It recommends dwarfing rootstocks for limited space and discusses small container-grown
   trees. A mature, fruit-bearing small apple tree need not be depicted as an immature sapling.

The Utah State page requested during research returned HTTP 403 and is not evidence for
this recommendation. No external images or other assets were imported.

## In-game reference scale

The project does not establish an authoritative conversion from its world units to human
metres. In particular, neither `TreeDesc.size` nor a world-coordinate height should be
reported directly as metres.

The source provides reliable **relative** dimensions:

- [`src/app/core/physics/surface_motion.rs`](../../src/app/core/physics/surface_motion.rs):
  player capsule radius = 4 voxels, cylindrical half-height = 8 voxels. Overall collision-body
  height is `2 × (4 + 8) = 24` voxels, or `0.09375` world units.
- [`src/gameplay/camera/desc.rs`](../../src/gameplay/camera/desc.rs): default eye height above
  the feet = `0.08` world units = `20.48` voxels. Default vertical field of view = 60 degrees.
  These are defaults, not a measurement of the user's current free-camera position.
- [`src/tree_gen/axis/garden.rs`](../../src/tree_gen/axis/garden.rs): the spreading architecture
  uses `size / 30` as a scale factor. Ascending scaffolds and twigs extend above the leader;
  the profile's leader height alone is not the final tree height.
- [`src/tree_gen/tree.rs`](../../src/tree_gen/tree.rs): generated trunks and leaf placements
  are in voxel space. Leaf-spray size is separately authored; changing wood scale alone
  does not uniformly scale every visual feature.

The metre equivalents below are **illustrative only**, assuming a 1.70 m player represented
by that 24-voxel body. Under that assumption, one voxel corresponds to about 7.08 cm. Using
another human-height reference changes the metre equivalents, not the measured ratios.

## Measurements of the production generator

A local optimized Rust probe included the production branching, tree, growth, geometry and
opening modules, and linked dependencies from the Release build. It did not approximate
branch generation or change source files. All cases below use mature age `1.0`.

Measurement definition:

```text
wood_top = maximum upper Y of generated RoundCone AABBs
leaf_top = maximum generated LeafPlacement.position.y + 0.5 voxel
height   = max(wood_top, leaf_top), measured from the authored root origin
width    = max(X span, Z span) of those same wood/leaf bounds
body-height ratio = height / 24
illustrative metres = height / 24 × 1.70
```

The half-voxel leaf envelope is a measurement convention, not exact GPU occupancy. These are
rest-pose CPU bounds: they exclude wind, quantization, animation, fruit, terrain-height
variation and image perspective. They are not pixel measurements or performance benchmarks.

### Fresh opening: actual version-2 recipe

The four descriptions come directly from
[`garden_layout::tree_descriptor(DEFAULT_SEED, index)`](../../src/garden_layout.rs), not the
Debug planting description. Their positions and deterministic seeds are unchanged.

| Opening tree | Individual seed | Authored size | Height, voxels | Width, voxels | Player-body heights | Illustrative metres |
|---|---:|---:|---:|---:|---:|---:|
| 0 | 2539892922 | 8.874 | 62.67 | 74.61 | 2.61 | 4.44 |
| 1 | 2847798192 | 9.605 | 62.33 | 78.06 | 2.60 | 4.41 |
| 2 | 305502193 | 8.785 | 64.06 | 77.65 | 2.67 | 4.54 |
| 3 | 580287596 | 9.407 | 67.99 | 81.95 | 2.83 | 4.82 |

These are about **3.04–3.32 default eye heights**. The opening already fits horizontal crown
clearance and uses level-4 leaf sprays, but that is not a human-height constraint. This size
can represent a larger managed apple tree; it is above the recommended compact-garden target.

### Configured planting, programmatic fallback, and converted old saves

| Description | Samples | Authored size / leaf level | Height, voxels | Player-body heights | Illustrative metres |
|---|---|---|---:|---:|---:|
| Checked-in Apple planting config | Seeds 7, 122, 907 | 20 / 5 | 135.79–139.78 | 5.66–5.82 | 9.62–9.90 |
| `TreeDesc::default()` | Seed 122 | 30 / 5 | 200.94 | 8.37 | 14.23 |
| Previously converted version-1 opening save | Original four seeds | 12.75–15.28 / 5 | 88.30–114.59 | 3.68–4.77 | 6.25–8.12 |

Planting values were decoded from [`config/gui.toml`](../../config/gui.toml), not inferred from
one screenshot. These are mature descriptions; the actual placement also depends on current
settings/age. The programmatic default is a separate fallback, not the checked-in GUI default.

The old-save sample is the existing `target/garden-delivery/world/apple-opening/migrated.rflterrain`
fixture from the [retirement evidence](../evidence/apple-opening-and-test-tree-retirement.md).
Its serialized tree descriptions were read with the production terrain reader; the file was
not written. This is one known fixture, not a claim about the user's current save. Migration
intentionally retained its larger authored sizes, rather than applying fresh-opening fitting.

Thus there are three distinct experiences: **fresh opening, ordinary configured planting, and
loaded historical trees**. The user's active one is not established by this research. A camera
change cannot by itself fix the measured tree/player geometry ratios.

## Proposed adjustment, not implemented

1. Choose a managed, spreading **2.5–3.0 m** mature garden apple as the visual reference,
   approximately **1.47–1.76 player-body heights**, or **35–42 voxels**, under the explicitly
   stated 1.70 m reference. A practical first target is close to 3 m, with modest seed variation.
2. Preserve the four opening anchors, seeds and usable paths. Relative to the current fresh
   opening, aim for roughly **35–40% less final height** as an initial visual candidate. The
   configured planting tree needs a larger reduction. These are output-height targets, not
   instructions to multiply `size` by a universal constant.
3. Keep a mature tree's connected scaffold architecture. Refit leaf sprays and trunk/branch
   proportions to avoid turning it into a dense shrub. Recheck fixed-voxel fruit attachment
   offsets, fruit visibility and ground clearance. Do not make apples tiny or replace mature
   fruiting with the young-age preset just to reduce height.
4. Keep the player and camera unchanged initially. Validate from the actual walking viewpoint,
   at fixed positions/seeds, alongside numerical height and passage checks. The appropriate
   proportion is a visual decision; horticultural height ranges alone do not approve appearance.
5. Address planting defaults and the explicit opening recipe coherently. Do not shrink Pine,
   Willow or Conifer incidentally. Decide existing-save resizing separately: baked wood would
   require ordinary transactional replacement, not merely changing saved `size` or leaf rendering.

## Local evidence and validation limits

Local, ignored research artifacts are under `target/apple-scale-research/`:
`probe.rs`, `tree_gen/` source links, `build-probe.mjs`, `build-artifacts.jsonl`, `build.log`,
`probe`, and `measurements.jsonl`. These are diagnostic artifacts, not installed/runtime assets.
With those local artifacts present, the measurement is rerunnable from the repository root:

```sh
cargo build --release --message-format=json > target/apple-scale-research/build-artifacts.jsonl
node target/apple-scale-research/build-probe.mjs
target/apple-scale-research/probe \
  target/garden-delivery/world/apple-opening/migrated.rflterrain \
  > target/apple-scale-research/measurements.jsonl
```

The scratch harness uses this checkout's absolute source paths and locally resolved Release
artifacts; it is not a portable installed tool. The definitions, baseline, inputs and complete
opening table above remain available without it. Initial harness-only module-resolution and
Serde artifact-selection errors were corrected before collecting the measurements; no game
source fix was required.

`cargo fmt --check`, `cargo check`, and `cargo build --release` passed. All 12 measurements
repeated byte-for-byte; the local Markdown references resolve. The measurement JSONL SHA-256
is `749e6b15c1ad834af54599c40bc891b139c1186ca5793b2fedd843b8bdb234bf`.
The read-only fixture's hash still matches the recorded retirement fixture:
`99c4c22c6e796c9917d091a6e2688b30229b8eb1bfa13320523b24c5dd016243`.

This research is neither a full test-suite run nor a new native visual review. No smaller
candidate was published, no save migration was added, and no visible game session was launched.
