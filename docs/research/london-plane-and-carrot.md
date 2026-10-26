# London plane and carrot — botanical references and art translation

## London plane (法国梧桐)

The Chinese common name is used here for London plane, **Platanus × hispanica**
(synonym **P. × acerifolia**), not Chinese parasol tree (*Firmiana simplex*).
This is the common hybrid plane of city avenues, not a claim that the genus
originated in France.

Primary horticultural references (retrieved during implementation):

1. [NC State Extension: Platanus × acerifolia](https://plants.ces.ncsu.edu/plants/platanus-x-acerifolia/).
   A hybrid of American sycamore and Oriental plane; pyramidal when young,
   open/spreading with age; mature 70–100 ft height and 60–75 ft spread;
   exfoliating light brown bark revealing creamy olive; alternate 3–5-lobed
   leaves; usually paired spherical fruit.
2. [Royal Horticultural Society: Platanus × hispanica](https://www.rhs.org.uk/plants/96764/platanus-%C3%97-hispanica/details).
   Confirms synonyms, large deciduous stature, flaking grey/cream bark and
   maple-like palmately lobed leaves up to 20 cm wide; maturity 20–50 years.

These are botanical descriptions, **not fitted growth equations**. There is no
claim that a paper validates the game's numerical branching constants.

### Implementation decisions

- Selectable third tree becomes London Plane; new shop saplings follow it.
  Old willow trees remain decode-only legacy geometry so existing saved wood
  ownership/collision does not become orphaned. Old unplanted willow stock
  transfers to plane stock on load, without loss or duplication.
- Clear lower trunk; unequal ascending scaffold stations; broad, open rounded
  crown, upward lateral shoots; young trees reveal a leader before later branches.
  Unpollarded garden form, deliberately not a pollarded street-tree knob.
- Shared axis compiler owns connectivity/taper/age/wind attachment. Numeric
  architecture is in `src/tree_gen/growth/rules.rs`, not renderer switches.
- Flattened five-lobed **voxel foliage sprays** suggest palmate foliage at this
  scale. These are stylized leaf masses, not individually modeled botanical
  leaves, and not the willow's thin descending curtains.
- Cream/olive/brown exfoliating bark uses fixed rest-space voxel patches and the
  ordinary lighting path. Species appearance is carried with owned surface
  bindings, including fragment-cache misses; it cannot recolor other trees or
  swim during wind deformation.
- No apple fruit on plane. Plane seed balls/seasonal shedding, true secondary
  growth and physiological competition are outside this change.
- No reference photograph, texture, external model or AI-generated resource was
  imported. Geometry and bark pattern are original code; links are research only.

## Carrot

### Primary references

1. [NC State Extension: Daucus carota subsp. sativus](https://plants.ces.ncsu.edu/plants/daucus-carota-subsp-sativus/).
   Cultivated carrots have feathery green leaves and a thick, fleshy storage
   taproot. They are biennial, but harvested as a first-year vegetable. Direct
   sowing avoids damaged roots; maturity is roughly 70–120 days, cultivar and
   environment dependent. The shoulder can rise above soil at harvest.
2. [RHS: How to grow carrots](https://www.rhs.org.uk/vegetables/carrots/grow-your-own).
   Confirms direct sowing, thinning to 5–7.5 cm, and harvesting full-size roots
   around 12–16 weeks. Young roots can also be eaten. Stony/heavy soil can deform
   roots; containers need sufficient depth. These are gardening observations,
   not justification for numerical game growth constants.

The University of Minnesota page was also attempted, but HTTP 403 prevented
reading it; no claim here relies on that unavailable page.

### Modeling and play translation

- Orange tapered storage root with a rounded shoulder; most of the root is
  below the planting plane. Seven spreading, divided leaves form a basal
  rosette. Broken transverse root scars and two green tones supply restrained
  detail without imported imagery. The first-year model has no flower/umbel.
- Original deterministic low-poly recipe: `assets/models/carrot-source.mjs`.
  It uses the existing published mesh/palette and pixel-surface pipeline, not
  a separate sprite renderer or terrain-voxel plant. The catalog distinguishes
  `root_crop` from `flower`; nonflower crops do not offer nectar landing sites.
- First in-game capture showed disconnected, disappearing leaf stalks at the
  normal 20px cache budget. A second iteration thickens petioles/leaflets and
  joins the crown so the pixel silhouette remains readable. Inspect
  `target/liveliness-carrots-full.png`; this is an actual game frame, not a web
  preview. Fine botanical leaflet structure is intentionally simplified.
- Grow → Flora → Carrot. Finite play buys seeds, not transplant seedlings.
  Growth follows the existing accelerated flora tick and hydration presentation,
  not real agricultural days. CPU-owned authored growth now advances as well as
  grass, including chunks without grass, and is preserved across publication
  and reload. This also allows existing seed-grown model flowers to advance.
- Harvest or Vacuum transfers a mature root exactly once into saved crop stock;
  immature carrots stay planted. Shop sells carrots for the configured price.
  This mature-only rule is a gameplay simplification (real baby carrots are
  edible), not a biological claim. Harvested carrots are produce, not magically
  reusable seedlings.
- No nutrient physiology, cultivar selection, root forking, second-year
  flowering, seed reproduction or carrot-specific disease model is claimed.
  Soil edits use the existing plant/terrain rules; this is not a new root physics
  simulation. Growth is a shared uniform scale, not developmental leaf emergence.
- No external photo, texture, mesh or AI image/3D-generation service was used.
  Reproducible generation and provenance are in
  [the asset bundle](../../assets/models/README.md#carrot-root-crop).
