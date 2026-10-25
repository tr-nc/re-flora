# Shared tree growth and species rules

This is a behavior-preserving generation refactor, not a new botanical simulation. The public
interface remains `Tree::new(mature_desc.at_age(age))`; planting, preview, snapshot restore, poses,
collision and rendering do not need species-specific construction paths.

## Ownership

```text
saved TreeDesc + individual seed + age
                |
       species.rs: one species definition
       architecture / foliage / fruit / authoring defaults
                |
       growth::generate(aged description)
          |                   |
   architectural axes    legacy recursive adapter
   spreading / layered   (existing branch_skeleton)
   legacy Conifer
          \                   /
       full skeleton + age-selected wood/radii + foliage sites
                |
       tree.rs: common geometry and leaf-spray construction
                |
       existing publication / pose / wind / snapshot paths
```

- **`src/tree_gen/species.rs`** registers each persistent species once. Label, fruit policy,
  authoring defaults and growth rule come from that definition. `uses_axis_model` derives from
  the same resolved rule used by generation, so GUI capability and snapshot-budget checks cannot
  independently choose another generator. The old Conifer flag resolves its legacy adapter here.
- **`src/tree_gen/growth/rules.rs`** contains typed architectural recipes: crown height/width,
  scaffold ascent or sag, branch count, rising versus pendant shoots, and tip-only versus along-axis
  foliage. These are internal, code-declared recipes, **not a new external config format or GUI**.
  Existing editable dimensions, density, seed, etc. continue to live in the saved `TreeDesc`.
- **`src/tree_gen/axis/garden.rs`** implements spreading and layered crown algorithms. Spreading
  crowns can use rising or pendant shoot rules; there is no `willow: bool` or species-name switch.
  Different curves remain small, intentional algorithm choices, not copies of a whole tree system.
- **`src/tree_gen/axis.rs`** compiles complete axes. It derives branch order from the parent instead
  of trusting a caller-supplied number, checks endpoint attachment and point/radius/age lengths,
  preserves taper samples, assigns stable axis IDs, enforces the segment budget, and clamps child
  reveal times to their parent's. Sampling depth is not branching depth.
- **`src/tree_gen/growth.rs`** is the single internal generation interface. It resolves architecture,
  age visibility, wood radii and foliage attachment policy, including the visible leader's young
  crown. It returns full branch identity even for wood not yet visible. `tree.rs` no longer chooses
  a species, checks legacy-versus-axis topology, or special-cases Apple foliage.
- **`src/tree_gen/axis/legacy_conifer.rs`** isolates the frozen legacy axis layout. Its arithmetic,
  RNG draw order and topology remain unchanged; recursive trees retain the existing generator.

## Invariants

1. A fixed authored description and seed reconstruct the same individual, including after save/load.
   Age scales/reveals that individual rather than resampling its architecture.
2. Every architectural side axis begins at its parent's endpoint. Segment parents precede children;
   a child is not revealed before its parent. Consecutive samples share radii and axis identity.
3. Leaf sites are generated only on visible wood. Apple masses attach at twig tips; Pine/Willow
   sprays follow shoot axes. Their distinction is a rule, not a renderer species check.
4. Species profiles provide defaults when switching species. Matching-species live authoring is
   preserved (apart from the existing normalization that new species always use axes).
5. Fruit policy stays authoritative even when an incompatible nonzero apple probability is copied
   into a non-apple description. No new fruit lifecycle was introduced.
6. Existing serialized identity, description fields, empty axis metadata for recursive trees and
   Conifer A/B semantics remain compatible. No save migration or silent shape replacement occurs.

## Extending the system

For a species expressible with current architecture:

1. Add its persistent identity and definition in `species.rs`, selecting an architectural recipe,
   foliage policy, fruit policy and switching defaults. Never repurpose an existing saved identity.
2. Declare the recipe in `growth/rules.rs`, reusing spreading/layered and rising/pendant mechanisms.
3. Add the species to the intended planting catalog and exercise the public `Tree` interface across
   seeds, ages and round trips. Verify native planting, restoration and appearance.

Only a genuinely different branch behavior should add a new internal architecture/shoot adapter.
The geometry builder, renderer and publication code should not grow species-name branches. Fixed
sampling/implementation choices need not all become configurable, and this does not introduce a
plugin registry or runtime scripting system.

## Explicit limits

- Growth is still deterministic procedural architecture plus age scaling/reveal. It does not react
  to sunlight competition, pruning history, obstacles or water by deciding where new buds emerge.
- Legacy generators are intentionally retained behind the same interface, not rewritten to look
  like the new species. Unifying the interface does not erase their saved appearance.
- **The ordinary fresh garden still establishes four legacy Garden Trees.** Its opening recipe in
  `src/garden_layout.rs` is independent of `[tree.desc]`. That saved tuning description and the new
  planting selection default to Apple Tree; they do not replace the opening trees. Changing the
  opening layout is a separate player-visible change.

Validation and the same-machine compatibility comparison are recorded in
[`evidence/tree-growth-rules.md`](evidence/tree-growth-rules.md).
