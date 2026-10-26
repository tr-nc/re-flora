# Bees and attached nesting houses

## Player behavior

Choose **Bee house** (stable internal tool slot 23). Left-click exposed tree/pole wood within two world units to mount; right-click a house to remove it. One house per host, eight houses total. Plant flowers nearby. A house supplies eight bounded cosmetic residents: leave home, approach a nearby flower, perch, return, rest. Night prevents new trips and recalls residents. With no eligible flowers they remain home. Houses remain visible with particle rendering disabled; residents do not.

This is a roofed **solitary-bee nesting house**, not a managed honeybee hive or honey economy. See [primary-source research and its limitations](research/garden-bee-houses.md). Construction is currently free, independently of the finite plant shop. No pollination-yield bonus, breeding, stinging or species-specific husbandry simulation is claimed.

## Ownership and rendering

- `garden_bees.rs` owns monotonic house IDs, tree/post host IDs, rest support cells and horizontal facing. Invalid/duplicate hosts, out-of-world placement, unsuitable wood, nearby houses and blocked body/entrance space reject before insertion. Real post recipes and canonical tree wood, not arbitrary painted wood, identify a host. Voxel-fallback tree contacts use the first matching stable tree ID; a house never silently transfers to an overlapping tree later.
- Houses **do not stamp terrain**. Original Rust triangle models for the striped bee (four wing poses) and roofed nine-opening house use the existing cached-angle model bank and native fallback. They are excluded from the voxel-particle stream. No new texture/image/model-service asset was imported.
- Stable tree attachment uses generation-bound skin bindings and the canonical tree pose, not another wind simulation or new per-frame pose readbacks. Existing renderer-published flower IDs/poses drive perching; wind-dependent flower attachment is shared with butterflies. At most one bee claims a flower within this swarm; this is not cross-species occupancy arbitration.
- `particles/bees.rs` owns at most 64 transient residents. Houses persist; trips restart from home after load. Lost flowers cause return; lost houses retire their residents. World-update gating freezes simulation. Free-flight position/heading/wings use the existing insect presentation clock; attached models follow their host transforms.
- Terrain publication invalidates only intersected house regions. Batched bounded clearance reads run for dirty/loaded houses, not idle houses. Losing the mount, burying the body/entrance, deleting the tree, or retiring the pole ID retires the house. Pole identity still follows the existing **cap-only** validity policy, not full structural stability. Growth/rebuilding can retire a house when its old support/clearance no longer fits. There is no falling-house physics or reservation of a wind-swept clearance envelope.
- `GardenSnapshot.hives` is serde-defaulted; old saves get no houses. Metadata validates before restore; runtime bindings/residents reset. Save reconciles attachments first. Existing economy, carried soil and fence metadata remain intact.
- Existing cache/resolution/FPS control IDs remain unchanged. Cache labels now cover insects/bee houses. The stale FPS-label migration discovered during testing was fixed separately in `89d267e3`: presentation metadata now comes from the declarative schema, preserving saved values.

## Accepted evidence

`RE_FLORA_BEE_REVIEW` is an opt-in native fixture calling production APIs, **not a physical mouse-input play-through**. A seeded mature Apple, a thick pole and six daisies provide repeatable geometry. Tree contact discovery uses a bounded rest-atlas search and verifies canonical fallback ownership.

| Review | Observed result |
|---|---|
| `garden` | 2 houses / 16 residents; 15 visits, 10 returns by the 14-second checkpoint; terrain bytes unchanged by houses; duplicate/missing-host rejection |
| `load` | Actual binary save/restart retained both IDs and tree/pole attachments; trips resumed, 15 visits / 10 returns |
| `edit` | Both support cells removed: 0 houses/residents, monotonic `next_id=3` retained |
| `cap` | Pole cap removed: pole identity and its house retired; tree house retained |
| `tree-delete` | Tree removed: pole house retained; tree house retired |
| `bury` | Added Rock inside the tree house: house retired, Rock preserved, pole house retained |
| `cycle` | Day-to-night: 16 residents at home, 0 outside |
| `flowers-removed` | Six lost flower hosts detected; all residents returned |
| `particles-off` | 2 houses remain, 0 residents |
| `native` | Cache disabled at runtime; native model mode confirmed in logs; same 15 visits / 10 returns |
| `without` | Same habitat fixture without houses: 0 residents |

Inspected `target/garden-bees.png` (normal daytime scene) and `target/garden-bees-tree.png` (attachment diagnostic: leaves, flowers and fruit intentionally hidden, not default appearance). Coarse model sampling can simplify openings; appearance is a first pass, not player approval.

Focused bee/house, input, fence, snapshot, toolbar, butterfly and model-cache suites passed, including four material IDs in the house cache. `cargo fmt --check`, `cargo check`, `cargo build --release`, hidden muted smoke, native runs and log inspection passed; final native logs have no ERROR/panic/VUID. GUI generated metadata changed only with its two source labels; shader ABI structs did not change.

```sh
# Create/capture/save, then actually restart/load.
env -u WAYLAND_DISPLAY RE_FLORA_BEE_REVIEW=garden RUST_LOG=info \
  cargo run --release -- --hidden --mute --terrain-save target/garden-bees.rflterrain \
  --screenshot garden-arrival target/garden-bees.png --screenshot-delay 5 --auto-exit 16
env -u WAYLAND_DISPLAY RE_FLORA_BEE_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute --terrain-load target/garden-bees.rflterrain --auto-exit 16
# Other modes use the same command without terrain-load/save.
# tree-view also accepts --screenshot garden-arrival target/garden-bees-tree.png.
```

## Cost and limits

Five additional cached sources (four bee poses, one house), at default 128 views × 16×16 × 48 bytes, add **7.5 MiB** of surface samples. This is a standing cache cost even with no houses. The model instance capacity adds 64 bees + 8 houses; the population does not grow without bounds.

Two sequential 16-second Release runs used the same tree/pole/six-flower fixture, `--hidden --mute --perf`, and `RE_FLORA_GARDEN_PERF=1` to log **every** frame. After frame 300: no houses median/p95 **16.96 / 17.98 ms**, 761 samples; two houses/16 residents **17.02 / 17.94 ms**, 760 samples. NVIDIA RTX 3060 Ti, 4096×2560 physical window, 820×512 scene extent, default automatic presentation. These are whole-frame, presentation-inclusive results from one pair, not isolated GPU costs, statistical significance, a maximum-population test or dense-garden/60-FPS acceptance.

Residents use bounded kinematic paths, not an obstacle route planner. Houses have no separate rigid-body collider, material price or honey inventory. No native GPU-allocation failure injection or subjective interaction/art approval is claimed. Existing asynchronous flower-pose publication is reused; “no idle terrain readback” does not mean the entire application performs no GPU readbacks.
