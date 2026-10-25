# Shared tree growth and species rules

The shared module originated as a behavior-preserving refactor, not a botanical simulation.
The later requested Test/Garden Tree retirement is an intentional compatibility migration (below).
The public interface remains `Tree::new(mature_desc.at_age(age))`; planting, preview, snapshot restore, poses,
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
  independently choose another living generator. The old Conifer flag resolves its legacy adapter
  here. `RetiredGardenTree` is only a wire marker, resolving to Apple rather than its own profile.
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
  RNG draw order and topology remain unchanged; recursive Conifer retains the shared generator.
- **`src/tree_gen/compatibility.rs`** freezes historical missing-field defaults and normalizes retired
  descriptions to Apple before age scaling. Snapshot migration uses the existing recursive adapter
  with exact saved fields solely to identify old baked wood; this is not a selectable old morphology.

## Invariants

1. A fixed current-species description and seed reconstruct the same individual, including after save/load.
   Age scales/reveals that individual rather than resampling its architecture.
2. Every architectural side axis begins at its parent's endpoint. Segment parents precede children;
   a child is not revealed before its parent. Consecutive samples share radii and axis identity.
3. Leaf sites are generated only on visible wood. Apple masses attach at twig tips; Pine/Willow
   sprays follow shoot axes. Their distinction is a rule, not a renderer species check.
4. Species profiles provide defaults when switching species. Matching-species live authoring is
   preserved (apart from the existing normalization that new species always use axes).
5. Fruit policy stays authoritative even when an incompatible nonzero apple probability is copied
   into a non-apple description. No new fruit lifecycle was introduced.
6. Conifer A/B, Pine, Willow and existing Apple authoring remain unchanged. `garden_tree` and missing
   species decode as retired metadata. Their replacement keeps ID, position, size, seed and global
   age/fruit phase, but deliberately adopts current Apple authoring and morphology. GUI loading
   normalizes the tuning description; garden loading replaces baked wood through the normal tree
   transaction before completing restoration. Both old and replacement allocation budgets are
   checked. Loading never overwrites the source file; saving writes the current Apple identity.

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
- Legacy Conifer's two modes and the shared recursive machinery used by other vegetation remain.
  Test/Garden Tree no longer has an independent species profile or normal generation entry point;
  only historical decoding and old-wood reconstruction are retained for safe migration.
- **The ordinary fresh garden now establishes four current Apple Trees at the original anchors.**
  Its compact size/leaf-spray fitting in `src/garden_layout.rs` remains independent of `[tree.desc]`.
  This is a subsequent player-visible recipe change, not part of the earlier invariant refactor.
- Retired trees' hand-edited wood is replaced with Apple geometry. Unretired trees take the normal
  saved-atlas restoration path; overlapping trees follow existing replacement transaction semantics.

Validation and the same-machine compatibility comparison are recorded in
[`evidence/tree-growth-rules.md`](evidence/tree-growth-rules.md). The later opening and retirement
checks are in [`evidence/apple-opening-and-test-tree-retirement.md`](evidence/apple-opening-and-test-tree-retirement.md).
