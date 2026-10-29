# Ginkgo — botanical research and implementation contract

## Primary references

- [NC State Extension, Ginkgo biloba](https://plants.ces.ncsu.edu/plants/ginkgo-biloba/), full page read: deciduous tree native to China; typically 50–80 ft tall, 30–40 ft wide; pyramidal form, distinctive fan-shaped leaves and golden autumn foliage. Female trees bear fleshy seeds; planted male trees are often preferred. Full sun and drainage matter in cultivation.
- [RHS, Ginkgo biloba](https://www.rhs.org.uk/plants/7990/ginkgo-biloba/details/), full page read: conical when young, irregular/spreading with age; fan-shaped, often bilobed leaves up to 12 cm wide; clear yellow autumn foliage, green spring/summer foliage, upright habit. This supports a recognisable autumn art direction, not year-round botanical yellow leaves.
- [Arnold Arboretum, Ginkgo](https://arboretum.harvard.edu/plant-bios/ginkgo/), search-index passage only (direct fetch returned HTTP 403): short/spur shoots, fan-shaped leaves (often bilobed on long shoots), gray fissured bark. Do not treat this as a full-page review.
- [Kew, Ginkgo tree](https://www.kew.org/plants/ginkgo-tree), search-index passage only (direct fetch HTTP 403): fan-shaped, often lobed leaves, golden-yellow autumn color; corroboration only.

These descriptions are not fitted growth equations. Tree dimensions, branching constants, palette and shop price below are game/art decisions, not claims of scientific simulation. No external photo/model/texture or AI-generated asset is imported.

## Existing ownership and constraints

- `src/tree_gen/species.rs` owns persistent snake-case identity, selectable species, architecture and switching defaults. `PLANTABLE` feeds Grow and the shop; `config/garden-shop.toml` must supply every offered sapling's price.
- `growth/rules.rs` and `axis/garden.rs` describe and compile connected axes. Growth reveals this stable topology; pose, wood ownership and collision share it. Do not introduce another generator/render path or change existing saved species geometry.
- `tree.rs` samples branch-oriented voxel foliage sprays (including plane's palmate mask). These are stylized leaf masses, not individually modeled botanical leaves. Ginkgo should use flattened, bilobed fan sprays on ascending shoots, not plane lobes or pine whorls.
- Per-tree `TreeAppearance` is a saved user variation relative to the shared palette. Variation resolution overwrites its offsets, so a species' golden base must **not** be smuggled into these user offsets or a global leaf-color setting. Carry a species-derived base palette through the existing leaf publication; apply saved variation afterward, consistently in both draw paths and restore.
- Wood already carries a bark style through its owned surface bindings. Add gray-brown vertically broken bark in this existing path, with rest-space coordinates, not a new material or global cherry-wood recolor.

## Intended behavior

1. Append persistent `ginkgo` / display `Ginkgo` to the existing three plantable trees. Existing species names, defaults and saved identities remain unchanged. New saplings cost 35 coins (same tier as plane).
2. Upright leader with unequal ascending scaffolds; a tapered upper crown rather than a broad plane dome or repeated pine whorls. Reuse the spreading compiler with an explicit crown-taper parameter, neutral for existing species. Keep deterministic seeds, taper/connectivity, age topology and branch-bound wind.
3. Flattened fan-shaped foliage sprays with a shallow central notch, warm gold palette and gray-brown bark. This is a fixed autumn presentation, not a seasonal simulation. Existing user color variation remains usable relative to that palette. No apples, seed harvest or reproductive simulation.
4. New species participates in the ordinary preview, planting preferences, finite shop stock, growth, serialization and transactional restore. No save schema migration or new Debug controls needed.
5. Validate multiple seeds and ages, foliage shape/bounds, no fruit, registry/shop consistency, independent palette and save/load. Use native Release species-review runs for actual preview/publication, age rebuild, serialized restore and GPU checks; inspect actual screenshots and logs. Build Release for handoff.

## Scope limits

No four-season phenology, sex selection, edible seeds, detailed vein mesh, new audio/image resource, or dense-forest performance claim. Decorative falling leaves retain the existing shared particle system. Golden **voxel sprays** communicate the fan motif at this game's scale; they are not a claim that each rendered voxel is a complete anatomical leaf.
