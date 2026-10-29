# Unified tree generation and per-tree variation

## Contract and ownership

`app/core/vegetation/planting_settings.rs::planting_description(settings, species,
individual)` is the fresh-tree interface. Opening trees, Grow's preview, advancing
to the next planting, and the older procedural scatter command all use it. The
opening layout supplies only stable identity/position, not another variation recipe.
`tree_gen/variation.rs` resolves that recipe into an owned `TreeDesc`.

- A separate preview individual key stays stable while editing sliders or toggling
  randomness. Color, size and morphology do not reroll each other. Successful
  ordinary planting already advances the preview; rejection does not alter its
  captured descriptor.
- **Randomize new trees** off: use the authored branching seed, size mean and
  color means. On: optionally vary the branching seed and sample the numeric spreads.
- The initial four Apple trees use exactly the same recipe as later Apple planting.
  Their keys derive deterministically from the garden seed and index. Fresh layout
  version is now 6; the flower random stream is unchanged.
- Existing descriptions, growth, undo/transaction compensation and garden restore
  consume captured values, never resample current planting settings. Missing
  appearance metadata in old descriptions defaults to identity. Snapshot validation
  rejects non-finite/out-of-range appearance values.
- Fixed benchmark/test scenes and the explicitly requested legacy **tuning tree**
  remain directly authored, not fresh random planting. The layout's old descriptor
  helper is now test-only for frozen migration/geometry fixtures.

The unsaved App-only `TreeVariationConfig` and its second randomization algorithm
were removed. The individual key is ephemeral preview identity, not another copy
of saved controls. All controls use the existing declarative schema and Save path.

## Tuning UX

Search **Tree Planting** in Debug. Mean/spread controls are adjacent:

| Control | Default | Meaning |
| --- | --- | --- |
| Randomize new trees | On | Affects future trees, not existing ones |
| Shape: vary individual seed | On | Disable to keep architecture fixed while varying color/size |
| Tree scale mean | 1.00 | Relative to the selected species' native size; never compounds after loading an edited tree |
| Size variation | 15% | Maximum ± fraction of that mean |
| Leaf hue offset mean | 0° | Preserve the shared palette; negative moves its green toward yellow/red, positive toward greener hues |
| Leaf hue variation | 12° | Maximum ± hue offset |
| Leaf saturation mean / variation | 1.00 / 0.08 | Multiplier and maximum additive ± spread |
| Leaf brightness mean / variation | 1.00 / 0.08 | Multiplier and maximum additive ± spread |

Instant maturity and Apple density retain their existing saved controls. Turning
randomness off disables spread/shape controls, **not** the means. Existing global
Leaves bottom/tip color controls still affect the common palette: these new
per-tree values are offsets relative to it, not frozen absolute colors.

Recommended iteration: turn randomization off, tune one mean tree, then enable
randomness and increase spreads. For the current palette, roughly −70° produces
red foliage, −30° yellow/green, and +15° stronger green; these are examples, not
absolute color swatches independent of the base palette. Shape uses the existing
species generator and seed; this change does not add a second crown algorithm or
randomly inflate branching complexity. The planting ghost remains the existing
blue geometry preview, not a photorealistic color preview.

### Distribution choice

This is an engineering/UX choice, not a botanical variance model:

- Use a symmetric **triangular** distribution: `mean + amplitude * (U1 + U2 - 1)`.
  It favors the center, has finite support, and reaches no further than the shown
  maximum ± amplitude. Variance is `amplitude² / 6`; the UI intentionally says
  *variation*, not mathematical variance or standard deviation.
- Size multiplies the species-relative mean by `1 + deviation`. A mean of 1 with
  variation 15% means 0.85–1.15, with most trees closer to 1.
- Numeric color bounds are hue −120…60°, saturation/brightness 0.25…2. Near a bound,
  symmetric support shrinks to the nearer bound, preserving the mean rather than
  clipping samples and accumulating trees at the limit. Actual allowed spread can
  therefore be less than the displayed maximum.
- Hash streams have separate fixed domains for size/hue/saturation/brightness.
  They do not depend on mutable RNG call order, position, frame time, or tree draw
  index. The recipe is reproducible; independently created trees receive different
  keys, while fixed mode deliberately ignores that key.

### Primary-source research

1. [Blender 3.3 Random Value node](https://docs.blender.org/manual/en/3.3/modeling/geometry_nodes/utilities/random_value.html):
   distinguishes an element **ID** from a **Seed**, documents different streams for
   the same ID, and bounded Min/Max inputs. This supports explicit per-individual
   identity and independent channels rather than consuming one ambient RNG stream.
2. [Blender Color Ramp widget](https://docs.blender.org/manual/en/latest/interface/controls/templates/color_ramp.html):
   documents color stops and RGB versus HSV/HSL interpolation, including saturation
   retention between hues. For this repository an HSV **offset**, not a replacement
   palette/ramp, preserves the existing height gradient with fewer controls. This
   is a design inference, not a claim that Blender prescribes our distribution.

Both official pages were fetched directly during this change. An attempted
SpeedTree randomization page did not exist; it is not used as supporting evidence.

## Rendering and persistence

`TreeDesc.appearance` stores resolved hue/saturation/brightness. The transactional
leaf publication, compensation and restore paths copy it into `TreeLeavesInstance`.
The renderer transforms the existing compact RGB10 color tables on the CPU, in
sRGB HSV and back to linear. Near and billboard LOD share the same leaf draw path
and transformed table. The neutral path returns the original table exactly.

No shader ABI, extra descriptor or per-vertex random evaluation is introduced;
the leaf instance stays **8 bytes**. Color is material/albedo input, so normal
lighting still applies. Bark, apples and the separately authored autumn falling-
leaf particle palette are unchanged. Size retains the existing tree-size semantics:
branch lengths/radii follow it, while voxel leaf-detail size remains a separate
species parameter, not continuous scaling of every individual leaf voxel.

The CPU table conversion is extra per-draw work for non-neutral trees; no aggregate
performance improvement or dense-forest budget acceptance is claimed.

## Validation

- `cargo check`, `cargo build --release`, deterministic variation tests, neutral
  legacy decode, per-tree palette tests, and shared opening/Grow tests pass.
- Full serial Rust suite before the final percent-label/UI-test additions:
  **1,756 passed, 9 failed, 15 ignored**, plus four collision tests. The same two
  config-default and seven raster-tree baseline failures remain; they were not
  hidden by changing unrelated defaults. Subsequent focused tests pass for six
  distribution/ownership cases, three opening cases, real slider edits, real
  checkbox clicks off/on followed by Save/reload, and old-file migration without
  rewriting the file. The generic all-settings persistence tests also passed.
- Real hidden/muted Release fixture (`RE_FLORA_TREE_VARIATION_REVIEW=1`) plants
  three individually colored trees through preview capture and normal placement,
  checks stable slider refresh, fixed-mode next preview, existing-tree isolation,
  sapling maturation, JSON snapshot round trip, restoration and published palette
  ownership. Log reports `verified=true`; native screenshot inspected:
  `target/tree-variation/individuals.png`. This explicit fixture replaces scene
  trees; do not use it with a valuable loaded world or request a garden save unless
  deliberately saving the fixture.
- Fresh-world native runs separately verify all four opening trees: random mode
  produces distinct seeds/sizes/appearance; fixed mode produces four seed-122,
  size-6.380, identity-palette trees. Logs:
  `target/tree-variation/opening-{random,fixed}.log` (fixed uses an isolated config).
- Debug search screenshot inspected: `target/tree-variation/controls.png`, with
  all 12 declarations in one group, means adjacent to spreads, and normal scroll
  behavior. No visible game was launched automatically.
- Hidden runtime logs contain no ERROR/VUID/panic and successful shutdown. Full
  formatting remains blocked by unrelated existing differences; changed modules
  are formatted. Shader structs are unchanged; the GUI fields are build-generated.

Reproduce ordinary smoke and native fixture after building Release:

```sh
cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_TREE_VARIATION_REVIEW=1 RUST_LOG=info \
  cargo run --release -- --hidden --mute --screenshot garden-overview \
  target/tree-variation/individuals.png --screenshot-delay 1 --auto-exit 3
cargo run --release -- --tail-latest-log 120
```

Still requires user visual approval of the default spread, species silhouettes,
and desired palette. Tests establish stable ownership and rendering integration,
not that these artistic defaults are universally preferable.
