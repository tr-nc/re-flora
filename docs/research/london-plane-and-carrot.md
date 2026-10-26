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

Research and implementation notes follow in the carrot step.
