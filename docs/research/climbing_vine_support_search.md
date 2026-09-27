# How climbing plants search for support: motion, range, and implications for the vine

Date: 2026-09-23. Code inspected at `1bf18847` on `grepping-plants`.
Scope: primary-source research and a design audit, **not an implemented solver change or
an experimentally isolated diagnosis of the player's particular sharp-angle case**.
The user has explicitly relaxed the earlier “all established stems must remain immobile”
requirement: visual continuity now takes priority, with performance measured afterwards.
This supersedes that design assumption in the earlier [live-shoot note](climbing_vine_live_shoot.md).

## 1. Separate the kinds of climber

There is no single “vine rotation algorithm.” The searching organ and the attachment
organ can be different, and the attachment mechanism matters:

- **Stem twiners**, such as common bean: the searching shoot revolves; the stem can
  subsequently wind around a support. Darwin's direct observations distinguish this
  from tendril and root climbing [S7, chapters I, IV, V].
- **Tendril climbers**, such as pea: tendrils and the shoot apex can both move, with
  developmental changes in their coordination; a tendril grasps the support. The 2024
  study tracks these anatomical landmarks separately [S3, Figures 1–6].
- **Root climbers**, such as English ivy (*Hedera helix*): stem-borne attachment roots
  and their root hairs establish adhesion to a surface. The main stem is not itself a
  succession of adhesive dots. Microscopy supports contact, form closure, chemical
  adhesion, and subsequent root-hair shape changes [S4, abstract/results/discussion].
- **Adhesive-disc tendril climbers** are another mechanism. Darwin describes the
  historical *Ampelopsis hederacea* (Virginia creeper) attaching through tendril discs;
  this is not the same organ as ivy's attachment roots [S7, chapter IV]. Do not infer
  modern Boston-ivy parameters from this historical species account.

**Design recommendation, not a botanical finding:** the current flat-wall demo is best
served by a wall-clinging phenotype with a searching young shoot and local attachment
organs. Do not combine bean-like large revolutions, pea-like grasping, and ivy-like wall
adhesion as though they were one experimentally established behavior. A separate twining
phenotype could later justify wrapping the main stem around poles.

## 2. What is actually rotating?

Circumnutation is an organ movement, not necessarily axial spinning of a rigid stem.
Bastien and Meroz model the changing magnitude and direction of curvature through
differential growth. Their 3D treatment shows that the direction of curvature follows
the direction of maximal differential growth under their assumptions. They also show
why an apex trajectory alone does not generally identify the underlying mechanism
[S1, abstract, model, results]. This is a primary theoretical/kinematic study, not an
experiment establishing an ivy-specific motor or universal oscillation parameters.

In sufficiently long growing organs, active growth is localized to a subapical zone
rather than the entire mature organ [S1, “The effects of growth”]. A complementary
shoot-tropism model combines gravity/light response with curvature-sensitive
straightening (proprioception) in a growth zone [S8, Eq. 1]. Its applicability to the
specific wall-clinging attachment behavior here is a modeling inference, not directly
validated by those papers.

**Important distinction for graphics:**

> The path a tip sweeps through time is not automatically the centerline of the final stem.

A curved growing zone can move an existing tip through a circle or ellipse while the
rest of that zone changes shape. Appending the tip's successive positions/directions as
permanent stem is a different construction. Actual winding around a pole can leave a
helical stem; free-space searching or wall-clinging need not leave a printed spring.
This distinction follows from the whole-organ kinematics in [S1], not from fitting a
smoother rendering curve over the present skeleton.

Observed/described trajectories include circular, elliptical, pendulum-like and irregular
patterns [S9, abstract]. In one pea experiment, of eight single-support plants, two
rotated clockwise, two counterclockwise, and four showed both directions [S2, §2.1].
That does **not** imply frequent direction reversal is appropriate for every species.

## 3. How large and how fast? Keep the measurements distinct

### Direct quantitative examples

| Source and subject | Reported measurement | Correct interpretation |
| --- | --- | --- |
| S2, pea, single thin support | Mean major-axis length **72.908 mm**; mean period **69.000 min** | About 7.3 cm sweep span in this condition, **not radius** |
| S2, pea, thin + thick supports | Mean major-axis length **91.214 mm**; mean period **66.746 min** | About 9.1 cm sweep span in this condition, **not radius** |
| S7, greenhouse common bean | Three revolutions: **2 h, 1 h 55 min, 1 h 55 min** | Historical observations from this plant/setting, not a species-wide constant |
| S6, 29 climbing species | Species' maximal searcher reach spans **0.1–2.5 m** | Free searcher reach, **not a circumnutation radius or diameter** |

S2's Table 1 reports substantial dispersion: the major-axis SDs are 43.538 mm (single
support) and 38.929 mm (double support). The study describes 17 pea subjects initially,
with eight in each analyzed condition in §2.1. Its tracked landmarks are tendrils on the
considered leaf (§4.5); the major axis is defined as maximum separation of trajectory
points (§4.6). The reported means must not become hard min/max limits or an ivy main-stem
radius. Halving a span would only give an approximate semiaxis for a suitably centered
ellipse, not a newly measured search radius. The paper uses augmented/Bayesian analyses;
we use these numbers as descriptive examples, not general-population precision claims.

S3 follows 24 pea plants (16 with a support 12 cm away, 8 without support). Circumnutation
becomes more evident from the third leaf, and velocity, sweep area and trajectory-center
distance change over development. Direction switches and circumnutation counts also
change during approach [S3, §§2.1–2.2, Table 1]. **The support's 12 cm distance is not the
rotation radius.** These observations argue against a constant-amplitude lifetime motor,
but do not prescribe one universal “increase amplitude near contact” rule.

S6 measures different mechanical architectures for different reach lengths. Reach,
flexural rigidity `EI`, tissue/material stiffness `E`, geometry and attachment strategy
must not be conflated. “Older = infinitely rigid” is not supported; even a monotonic
age-to-stiffness mapping would be a simplified artistic phenotype, not a universal law.

**What remains unknown:** the reviewed sources do not establish a general numerical
radius/period for the ivy-like wall-clinging main stem that this demo depicts. We should
not fill that gap with pea numbers. The engine's 256 voxels/world unit also does not
establish a centimeters-to-voxels conversion.

## 4. How is a support found and accepted?

A useful evidence-bounded description is **spatial exploration, encounter, organ-specific
contact response, then establishment of attachment**. This is not proof of a plant
selecting the nearest point from a complete 3D terrain map.

Pea studies report differences in movement under different support arrangements and at
different developmental stages [S2, S3]. Their authors' interpretations about perception,
information exchange, choice or trial-and-error are stronger than merely reporting the
trajectories. Those experiments do not establish universal remote sensing of exact wall
coordinates or a general-purpose path planner. Local collision knowledge in the engine
should be distinguished from what the growth controller is allowed to respond to.

For ivy, S4 proposes four attachment phases: **initial physical contact → root form
closure against the substrate → chemical adhesion → root-hair shape changes/form
closure**. Its results also distinguish substrates: secretion on glass did not guarantee
enduring attachment. “Touch an eligible voxel once, immediately become fully fixed” is
therefore a poor literal model of this mechanism.

Ivy attachment is mechanically finite. S5 reports measurable elastic behavior of
attachment roots and different failure modes by substrate. Its **34% extensibility is
for the central cylinders of attachment roots**, not permission to stretch the whole
vine 34%. We should not assign these material measurements directly to game springs.
The relevant qualitative lesson is distributed, compliant attachment—not welded stem
joints and not frictionless hinges.

## 5. Design gaps visible in the current code

These are static implementation facts and improvement hypotheses, not a new benchmark
or a completed reproduction of the reported kink.

| Current implementation | Why it may look artificial | Proposed direction |
| --- | --- | --- |
| `growth.rs::probe`: phase-driven upward/radial vector produces the next 2-voxel segment | Search motion is strongly encoded into permanent centerline shape | Drive a growing zone's preferred curvature; extend from its continuous tangent |
| `advance`: 24 phases per turn, one phase per ready ordinary growth attempt | Rotation period is tied to the growth-attempt rate and discrete steps | Continuous simulation-time oscillator, separately defined from elongation |
| `probe`: frame rebuilt from the node's exposed-face normal | A voxel-face normal can change abruptly at a corner | Carry a continuous material/tangent frame; use the surface normal as a constraint/cue |
| `attach_tip` calls `freeze_path` | Attachment freezes the whole previous path; older geometry cannot share bending | Continuous bending through attachments; local support constraints with finite compliance |
| `shoot.rs`: independent rest-direction targets plus gravity/search; no adjacent-segment bending energy | Length preservation does not ensure smooth junctions or curvature | Couple neighboring segments, including across attachment nodes |
| `shoot.rs`: active deformation ends at `Node.fixed` | Effective growing/moving zone is reset by attachment topology | Separate active growth-zone physiology, passive mechanics and attachment state |
| Spacing threshold plus one contact establishes attachment | Acceptance is instantaneous and essentially geometric | Candidate contact, alignment/contact persistence, strengthening, established support |
| Air budget is `clamp(3 × spacing, 12, 64)` | Adhesion density also changes reach; no independent motion amplitude model | Separate bounded extension budget, growth-zone length, search envelope and adhesion policy |

With the user's saved 10 ready attempts/s, 24 phases nominally take **2.4 seconds** under
continuous ready ordinary search. That is a code-derived gameplay period, not a biological
measurement. Faster playback is legitimate; locking rotation/elongation together and
leaving a near-identical wave per cycle is the more important design issue.

**Do not misdiagnose a phase reset:** `attach_tip` currently does not reset `Tip.phase`.
The missing continuity lies in the direction/frame/bending/fixation design, not a proven
“phase resets to zero at each anchor” bug. Similarly, a two-voxel step is not a search radius.

## 6. Recommended visual-first experiment

### First priority: continuous body and a genuine moving growth zone

1. Keep one main vine and stable node identities. Preserve pruning/regrowth as a deliberate
   gameplay rule; it is not a claim that real unsupported tissue instantly disappears.
2. Give the stem stretch/length constraints and **coupled bending resistance**, including
   across attachment locations. Use finite stiffness and damping; do not turn the whole
   vine into either a rigid wire or a limp rope.
3. Distinguish passive elastic response of older stem from active curvature change in the
   growing zone. Let older spans redistribute some bending. Age/tissue parameters should
   vary smoothly, rather than jumping to infinity on attachment.
4. Advance exploration phase with simulation time. Apply it to preferred curvature over
   a finite growing zone, not directly as a sequence of newly printed direction vectors.
   Preserve heading, curvature state and phase across contact transitions.
5. Choose an elliptical/asymmetric search envelope with gradual developmental changes.
   Seed differences should be coherent (period, aspect ratio, preferred direction, slow
   modulation), not independent frame-to-frame noise. Default handedness can remain
   stable; species-appropriate reversals are optional rather than mandatory randomness.

### Second priority: wall-clinging contact behavior

6. Let a close encounter initiate a bounded contact/attachment process, not instantaneous
   freezing. Body orientation must be feasible before attachment strengthens; if it cannot
   turn smoothly without penetrating terrain, continue searching or stop rather than kink.
7. For the proposed wall-clinging phenotype, consider short visible attachment roots or
   small contact clusters behind the apex. These are attachment organs, **not renewed main
   shoot branching**. They can connect a gently offset stem to the surface instead of
   forcing the main centerline to strike every anchor directly.
8. Use restrained near-wall exploration and a larger permitted envelope over gaps as an
   explicit artistic policy, not a universal biological law. Numerical reach bounds and
   authoritative collision checks remain mandatory; no attraction through unseen walls.

Do not first solve this by merely enlarging the rotation radius, increasing flexibility,
adding random turns, or drawing a smooth spline over an unchanged kinked collision body.
For a continuum-inspired alternative one can evolve a preferred-curvature field and solve
an elastic rod's pose; the sources do **not** select XPBD versus another numerical solver
for us. A bounded discrete rod with bend constraints is a reasonable first experiment,
not a claim of faithfully simulating plant physiology.

### What to compare before optimizing

Use a saved Debug A/B checkbox: unchecked = current implementation, checked = experimental
continuous-body model. This is a recommendation for future implementation; no checkbox or
model change is introduced by this research commit.

Start with the user's saved seed 3500, counterclockwise, recessed wall, then repeat on
flat wall, outward ledge and slope. Compare **both** apex trajectories through time and
simultaneous stem shapes. Track several points behind the apex, not only the tip [S1].

Acceptance observations should include smooth heading at birth and attachment, no abrupt
curvature spike at an anchor, small responsive older-stem motion, controlled tip sweeps,
and no persistent uniform free-space spring merely from searching. Keep hard checks for
finite coordinates, length, whole-segment/swept collision, stable identities, one main tip,
unknown/stale-terrain transactions and downstream pruning.

The old assertions that every established node/waiting stump is position-identical
cannot simply be reused for a movable model. Define the revised motion bounds explicitly;
retain bud identity and repair-gated restart behavior. If a cut base can move, its stored
restart geometry/support reference must remain meaningful—do not accidentally regrow
through a missing wall or silently invalidate an absolute-world-space saved cut step.

Choose artistic period/envelope/compliance values through visible comparison, then measure
release-mode costs. Existing export spikes and earlier failed performance acceptance are
not erased by this proposal. No performance claim or visual approval is made here.

## Primary sources and verification notes

- **S1.** Bastien & Meroz (2016), *The Kinematics of Plant Nutation Reveals a Simple
  Relation between Curvature and the Orientation of Differential Growth*.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC5140061/ — 3D model, growth-zone discussion,
  circle/ellipse examples, and limits of apex-only inference.
- **S2.** *Decision-Making Underlying Support-Searching in Pea Plants* (2023).
  https://pmc.ncbi.nlm.nih.gov/articles/PMC10143786/ — §2.1 handedness; Table 1 exact
  quantitative examples; §§4.5–4.6 landmarks and definitions. Interpretive language is
  not adopted as proof of cognition or omniscient support localization.
- **S3.** Guerra et al. (2024), *Ascent and Attachment in Pea Plants: A Matter of Iteration*.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC11124904/ — developmental differences,
  support/no-support comparison, sample sizes. Supplementary Videos S1/S2 are listed by
  the paper but were **not viewed** in this research pass; no video-derived claim is made.
- **S4.** Melzer et al. (2010), *The attachment strategy of English ivy: a complex
  mechanism acting on several hierarchical levels*.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC2894893/ — attachment phases and microscopy.
- **S5.** Steinbrecher et al. (2012), *Structure, attachment properties, and ecological
  importance of the attachment system of English ivy (Hedera helix)*.
  https://pmc.ncbi.nlm.nih.gov/articles/PMC3245459/ — root/root-cylinder mechanics and
  substrate-dependent failure; these are not stem material constants.
- **S6.** *Mind the Gap: Reach and Mechanical Diversity of Searcher Shoots in Climbing
  Plants* (2022).
  https://www.frontiersin.org/journals/forests-and-global-change/articles/10.3389/ffgc.2022.836247/full
  — primary 29-species mechanical/architectural study; reach is not sweep radius.
- **S7.** Darwin, *The Movements and Habits of Climbing Plants*, historical direct
  observations. https://www.gutenberg.org/files/2485/2485-h/2485-h.htm
  — common-bean revolution timings and distinctions among attachment organs. Old
  nomenclature and negative observations are not treated as modern universal taxonomy
  or proof that root climbers cannot circumnutate.
- **S8.** *A Unified Model of Shoot Tropism in Plants: Photo-, Gravi- and Propio-ception*
  (2015). https://pmc.ncbi.nlm.nih.gov/articles/PMC4332863/ — growth-zone curvature model;
  not direct validation of wall-clinging search behavior.
- **S9.** *Can Plants Move Like Animals? A Three-Dimensional Stereovision Analysis of
  Movement in Plants* (2021). https://pmc.ncbi.nlm.nih.gov/articles/PMC8300309/
  — 3D trajectory methodology and movement-shape vocabulary, not proof that plants
  possess animal-like intentions.

Source passages were checked in fetched full texts rather than adopting search-provider
summaries as evidence. MDPI access for S3 failed; its PMC full text was used instead.
A modern Parthenocissus PDF returned 404, and a 2026 support-sensing preprint DOI returned
429; neither is used to support findings here. No biological universal radius, molecular
oscillator mechanism, or calibrated game-scale conversion was established by this pass.
