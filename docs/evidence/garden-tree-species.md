# Apple, pine and weeping willow: implementation and native review

> Historical evidence for this delivery stage. The subsequent requested opening-tree replacement
> and Test/Garden Tree retirement are documented in
> [Apple opening and Test Tree retirement](apple-opening-and-test-tree-retirement.md).

## Delivered behavior

**Grow → Type: Tree** now offers Apple Tree, Pine and Weeping Willow. Tab, click-to-plant,
per-placement seeds, age, existing wind/leaf audio and garden persistence are reused. The checked-in
tuning-tree description is an Apple Tree; settings outside `[tree.desc]` are unchanged. **Correction:**
the ordinary fresh garden uses an independent opening recipe and still starts with four legacy
Garden Trees; it was not converted to Apple Tree. See [shared growth rules](../tree-growth.md) and
its [follow-up compatibility evidence](tree-growth-rules.md).

- **Apple:** short central axis, rising/spreading scaffold limbs, curved twigs, overlapping rounded
  foliage masses at twig tips rather than leaves at every segment. Apples hang below those tips
  and participate in the existing apple lifecycle. The first native candidate was rejected as too
  tiered/flat: the final version lowers the central axis, increases scaffold ascent, enlarges and
  rounds terminal leaf sprays, and lowers fruit attachments.
- **Pine:** continuously tapered, slightly bent trunk; seeded internode spacing, 2–4 limbs per
  station, unequal lengths/azimuths, open gaps, sagging limbs with upswept tips and outer sprays.
  It is no longer the identical nine-whorl/four-limb legacy Conifer.
- **Willow:** rising scaffold crown with separately connected, curved pendant shoots. Narrow
  branch-oriented leaf sprays run along descending axes. Drop length varies and ends above the
  root plane. These are actual branches/attachments and wind joints, not painted green curtains.
- Species owns the apple policy: even a copied/edited nonzero fruit probability cannot make Pine,
  Willow or legacy Conifer grow apples.
- The same bounded compiler owns parent-before-child connections, taper, axis identity and causal
  reveal age. New profiles retain deterministic topology over age; descendants are not revealed
  before their ancestors. Axis allocation budgets also apply on snapshot restoration.
- Old `garden_tree` and `conifer` saved identities retain their original generators. Missing species
  still means legacy Garden Tree; the new planting selection defaults independently to Apple Tree.
  No saved garden migration or automatic rewriting/replacement was performed.
- Tree authoring hides recursive-only controls for axis species, retaining size, thickness, seed,
  subdivision and foliage settings. Fruit controls are absent for non-apple species. The existing
  saved-object editor and unified Save path remain the owners; no App-only sliders/save hooks.

## Native Release evidence

`RE_FLORA_TREE_SPECIES_REVIEW=apple|pine|willow|mixed` is an explicit unsaved fixture. It exercises
selection and preview construction, publishes through the normal tree service on the real terrain,
then tests age 1 → 0.45, JSON serialization/observer restore, age 0.45 → 1, GPU/CPU pose agreement,
finite posed surfaces, GPU surface/BVH and lighting readbacks. Grow is cleared after planting so the
next-placement blueprint does not obscure the evaluated tree. `RE_FLORA_TREE_SPECIES_SEED` sets the
repeatable seed (default 122). No ordinary run enters this fixture.

```sh
# Run from this worktree. Repeat for apple/pine/willow and seeds 7/122/907.
mkdir -p target/tree-species-review
RE_FLORA_TREE_SPECIES_REVIEW=apple RE_FLORA_TREE_SPECIES_SEED=122 RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/tree-species-review/apple-122.png \
  --screenshot-delay 2 --auto-exit 4

RE_FLORA_TREE_SPECIES_REVIEW=mixed RUST_LOG=info cargo run --release -- \
  --hidden --mute --screenshot player-default target/tree-species-review/mixed.png \
  --screenshot-delay 2 --auto-exit 4

RUST_LOG=info cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 120
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 180
```

Final single-tree native results (published, quantized leaf instances, not an FPS benchmark):

| Species | Seed | Segments | Leaf instances | Apple sites |
|---|---:|---:|---:|---:|
| Apple | 7 | 250 | 31,402 | 34 |
| Apple | 122 | 250 | 31,329 | 27 |
| Apple | 907 | 250 | 31,569 | 28 |
| Pine | 7 | 428 | 18,628 | 0 |
| Pine | 122 | 460 | 20,098 | 0 |
| Pine | 907 | 476 | 20,906 | 0 |
| Willow | 7 | 448 | 12,580 | 0 |
| Willow | 122 | 448 | 12,605 | 0 |
| Willow | 907 | 448 | 12,650 | 0 |

All nine runs passed their `[TREE_SPECIES_REVIEW]` contracts and normal shutdown, with no
ERROR/panic/VUID in their captured logs. The final mixed run also passed, with seeds 122/123/124,
then normal `failures=0` exit. The raster smoke passed deletion/replacement, age/stiffness sweeps,
surface validation and legacy Conifer A/B plus snapshot restoration. It logged one attempted
removal of an already-absent tree in its deletion path; no ERROR/panic/VUID. Final ordinary startup
and its inspected per-worktree latest log were clean, `failures=0`.

An initial **invalid mixed fixture** placed its third root at x=2.25, outside the default
2-unit-wide atlas, and aborted during terrain publication. The fixture now derives interior
positions from world dimensions and was rerun successfully twice. This is not evidence that
arbitrary out-of-world internal `TreePlacement::World` calls are supported.

Artifacts under `target/tree-species-review/`:

- `{apple,pine,willow}-{7,122,907}.png`: viewed native captures; `mixed.png`: final mixed garden.
- `*-first-pass.png`: superseded candidate captures (include the then-visible preview blueprint).
- `seed-gallery.png`: nine current native crops; columns Apple/Pine/Willow, rows 7/122/907.
- `overview.png`: seed-122 crops side by side. Contact sheets use ImageMagick, crop
  `760x740+380+50`, resize to `390x380`, add text labels and margins; no recoloring or synthesized
  foliage. Original full frames remain beside them.

Temporary command logs: `/tmp/re-flora-tree-<species>-<seed>.log`, `...-mixed.log`,
`...-raster-smoke.log`, `...-final-smoke.log`, `...-final-latest.log`. The durable app logs are under
this worktree's `target/re-flora-logs`.

## Automated validation

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- `cargo test tree_gen`: **43 passed**, including new all-species/seed/age connectedness,
  deterministic topology/foliage bindings, finite taper, bounded geometry, descending willow vs
  ascending apple/pine shoots, and serialization/legacy-architecture checks.
- `cargo test vegetation::`: **43 passed**, including mixed legacy/new-species snapshot reconstruction,
  identical restored leaf positions, transactional publication guards and species-enforced fruit policy.
- `cargo test every_tree_architecture` and `cargo test lateral_wood_stays_excluded`: **1 + 1 passed**;
  root-axis rest collision policy includes all registered species.
- `cargo test plant_catalog`, `cargo test grow_panel`, `cargo test player_tools`: **2 + 3 + 19 passed**.
- `cargo test tree_gui`: **3 passed**, including untouched saved authoring for every species.
- `cargo test every_serialized_custom_leaf`, `cargo test every_declared_generic_setting`:
  **1 + 1 passed**. Added typed alternate-species policy to the existing generic save test.
- `cargo test gui_config_loader`: **56 passed**; `cargo test ui_text`: **4 passed**.

No full-suite rerun or performance acceptance is claimed. The known unrelated full-suite issues
from earlier work were not changed. No shader/generated-file/image/audio resource imports or
new fruit renderer were needed. Foliage still uses the existing voxel-leaf geometry and shared
saved palette: literal needle meshes, unique bark/leaf palettes, pine cones, willow catkins and
additional edible fruits remain outside this iteration. Final aesthetic approval belongs to the
user; no visible game was automatically launched.
