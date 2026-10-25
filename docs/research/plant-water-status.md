# Reversible water-deficit presentation

> Historical implementation notes. The immediate soil-to-wilt mapping below has
> been superseded by [root-fed plant water storage](plant-hydration-storage.md).
> Current grass/flower growth and wilt read persistent plant hydration.

## Reference and scope

Primary source, retrieved 2026-10-25: Oregon State University Extension,
[Environmental factors affecting plant growth — Water and humidity](https://extension.oregonstate.edu/catalog/em-9903-environmental-factors-affecting-plant-growth).
Water provides cell turgor (firmness); when roots cannot replenish water transported to leaves, turgor is lost and leaves go limp. This supports **loss of upright posture**, not treating all short-term dryness as dead/brown tissue.

Additional discovery: University of Minnesota Extension,
[Water-saving strategies for home lawns](https://extension.umn.edu/garden-and-home/yard-and-garden/lawns-and-landscapes-in-minnesota/water-saving-strategies-for-home-lawns), describes folded/curled blades and persistent footprints as watering cues in search excerpts. Direct UMN/UMass page retrieval returned HTTP 403; those excerpts are not used to justify exact deformation parameters.

The implementation is a stylized, recoverable visual response, not a calibrated botanical or hydraulic simulation. It covers both ordinary grasses and all registered modeled flowers. Tree leaves, trunks and apples are excluded. No mortality, permanent yellowing, independent plant hydration reserve, offline decay, or recovery timer is introduced. It does not yet model individual leaf rolling or independently rotating flower heads.

## Authoritative state and geometry

`shader/slang/plant_water_status.slang` derives water deficit from the existing root-support voxel's two-bit moisture, only for valid soil/sand and surface flora. Moisture levels 0/1/2/3 produce deficits 1, 4/9, 1/9, 0. Unsupported material/out-of-range support is neutral rather than fabricated drought. Existing growth curves and grass material wetness remain separate; even the growth override does not overwrite actual soil water status.

The same pure rooted offset is used by grass and flower poses. The root has zero displacement; quadratic height weighting bows upper tissue sideways and down. Direction is stable per root seed, not frame or draw index. At maximum deficit, tip displacement is 18% height sideways and 16% down (artistic parameters, not source-derived biological constants).

Grass geometry and lighting preparation receive the same sampled deficit without extra atlas samples. Flowers add wilt to the shared pose used by stem bands, flower heads and butterfly landing frames; the combined wind/wilt displacement stays within the existing 25%-height CPU culling budget. Tree/fruit pose entry points explicitly supply zero deficit. Water returning to level 3 removes the offset immediately on the next pose evaluation. The saved soil atlas already owns persistence, so no new snapshot or settings schema is required.

## Validation and limits

- `cargo fmt --check`, `cargo check`, `cargo test flora::species`, `cargo build --release`: pass.
- Production Slang CPU test `shader/tests/plant_water_status_test.slang`: passes. All surface species/four moisture levels, monotonic/bounded stress, rooted base, finite/downward/bounded motion over 64 seeds, exact wet recovery, invalid supports and tree/apple exclusion.
- `python scripts/run_slang_tests.py`: new water test passes, but full suite stops at `sprinkler_contact_contracts` (`failures=1`, not investigated or fixed here).
- Full Rust suite: 1,702 pass / 9 fail / 14 ignored, plus 4 passing library tests. The nine failures match the preceding toolbar validation: two configuration-default assertions and seven raster-tree tests. No full-suite green claim.
- Native hidden muted Release review plants all modeled flower species and a grass patch, writes real soil moisture, verifies GPU atlas readback, and asserts moisture transitions do not change instance growth/lifetime data. `water-cycle` visits wet -> dry -> wet at frames 0/48/96; all pass, no ERROR/panic/VUID.
- Viewed screenshots `target/plant-water-dry.png` and `target/plant-water-wet.png`. The dry pose has bowed stems/lowered head positions and slumped grass; appearance strength remains for player review. These captures use the growth override to isolate posture from growth reduction, and retain live wind, so they are not pixel-identical deterministic image tests. No performance acceptance is claimed.

```sh
RUST_LOG=info RE_FLORA_FLOWER_MODEL_REVIEW=water-cycle cargo run --release -- \
  --hidden --mute --auto-exit 5
RE_FLORA_FLOWER_MODEL_REVIEW=water-dry cargo run --release -- \
  --hidden --mute --screenshot player-default target/plant-water-dry.png \
  --screenshot-delay 2 --auto-exit 3
# Substitute water-wet and the corresponding output filename for the wet capture.
```

Review modes are opt-in, unsaved test scenes. Ordinary gameplay only reads existing water state; no visible game was automatically launched.
