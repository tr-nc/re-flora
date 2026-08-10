# First Garden Moment

Status: current product milestone  
Authority: subordinate to [Re: Flora Game Direction](./game_direction.md)  
Last updated: 2026-08-10

## Problem Statement

Re: Flora has substantial rendering, simulation, procedural-generation, spatial-audio, voxel, and
world-building technology, but its public presentation primarily communicates those technologies to
other developers. A stranger can see that the world is technically impressive without immediately
understanding what they would do, why the garden matters, or why they would want to play.

The solo developer needs a bounded milestone that turns existing technology into visible player
value, produces a more effective public demo, and creates a healthier feedback loop with players.
Continuing to polish independent technology systems does not by itself solve this communication and
gameplay-integration problem.

## Solution

Build one honest, curated Garden Moment around a complete care loop:

> enter a small garden, plant freely, change a local condition, see a beautiful response, harvest,
> open one new planting possibility, and leave without obligation.

The final vertical slice produces two player-facing artifacts:

1. a 30-45 second video for discovery and sharing;
2. a friction-light 10-15 minute packaged demo for direct play.

They are reached through three explicit delivery gates rather than one long implementation pass:

1. **Proof of Feeling:** a 15-25 second honest work-in-progress clip made mostly from existing
   capabilities. It shows Free-form Planting, direct care, a visible response, and atmosphere. It
   does not claim that harvest, Seed Circulation, or return persistence are finished.
2. **Garden Moment Video:** the complete 30-45 second causal arc, including a minimal harvest and
   exactly one new planting possibility.
3. **Playable Garden Moment:** the same loop in a 10-15 minute packaged build, with lightweight
   onboarding, safe exit, and truthful persistence and return behavior.

Each gate answers a different question. The first asks whether a stranger desires the experience;
the second asks whether the whole loop is legible and appealing; the third asks whether the game
feels good without editing or developer guidance. Failure at an earlier gate should change the
slice before more systems are added.

Technical breakdowns remain a separate artifact for developer audiences. New technology work is
accepted into this milestone only when it directly blocks or materially improves a named shot or
player action in the slice.

## User Stories

1. As a working adult, I want the game to open into a beautiful full-screen garden, so that entering
   it feels separate from work and the desktop.
2. As a returning player, I want to resume near my garden without front-loaded exposition, so that I
   can recover the feeling of place quickly.
3. As a tired player, I want to understand the first useful action through the world and controls,
   so that starting does not feel like reading another work document.
4. As a gardener, I want to choose a natural position for each plant without visible grid slots, so
   that the composition feels authored by me.
5. As a gardener, I want substrate or space limitations to be explained clearly, so that ecological
   rules feel understandable rather than arbitrary.
6. As a player, I want planting to have tactile visual and spatial-audio feedback, so that placing a
   seed feels physical and satisfying.
7. As a player, I want one recognizable plant to have clearly different growth stages, so that I can
   understand what changed without opening a statistics panel.
8. As a caretaker, I want to change one local condition such as moisture, so that I can form a clear
   cause-and-effect relationship with the plant.
9. As a caretaker, I want water to move and visibly affect soil, so that simulation reads as garden
   behavior rather than background technology.
10. As a relaxed player, I want unusual conditions to change growth rather than destroy my work, so
    that experimentation remains meaningful but safe.
11. As a player, I want to see a response within 30 seconds of care, so that I know the garden is
    listening to me.
12. As a player, I want growth to retain enough duration and anticipation, so that the experience
    does not feel like a mobile reward dispenser.
13. As a gardener, I want a harvest to remain visibly connected to the plant I cared for, so that it
    feels like an outcome rather than a generic resource pickup.
14. As a player, I want basic plants to return enough seed to continue, so that I cannot be locked
    out of the core garden loop.
15. As an experimenting gardener, I want surplus harvest to open one new plant possibility, so that
    the small economy expands expression instead of becoming a score.
16. As a player, I want to pause and leave at any point without loss, so that the game respects the
    boundary between the garden and real life.
17. As a returning player, I want my garden to remain healthy and possibly grow wilder while I am
    away, so that return is motivated by curiosity rather than fear.
18. As a viewer, I want the public video to show action, response, and atmosphere before technical
    terminology, so that I can decide whether I want to enter the garden.
19. As a potential player, I want a direct packaged download near the first public description, so
    that trying the game does not require compiling the engine.
20. As a technical follower, I want implementation breakdowns to remain available separately, so
    that the project can share engineering knowledge without making it the player pitch.
21. As the solo developer, I want a narrow definition of done, so that a new rendering or simulation
    problem cannot silently replace completion of the player-facing demo.
22. As the solo developer, I want audience feedback to reveal whether people desire the garden, so
    that comments about playing, feeling, and downloading complement technical praise.
23. As a viewer, I want the camera to place me near the plants rather than above an editor canvas,
    so that I imagine inhabiting the garden instead of authoring a technical scene.
24. As a first-time player, I want the seed, placement preview, tool motion, soil response, and
    sound to make planting self-explanatory, so that Free-form Planting does not feel like a
    level-editor brush.
25. As a viewer, I want the first short clip to state honestly that it is work in progress, so that
    an unfinished economy or save system is not mistaken for a broken promise.
26. As the solo developer, I want the first test to reuse mature visual and plant assets, so that I
    can learn whether the experience works before creating another large content system.
27. As the solo developer, I want every milestone task tied to a named shot or player action, so
    that attractive technical investigations cannot silently replace delivery.
28. As a playtester, I want the playable demo to teach itself with minimal interface and no verbal
    briefing, so that my behavior reveals whether the experience is actually understandable.
29. As a returning player, I want the game to distinguish saved garden state from presentation-only
    progress, so that I am never told my garden will persist before it truly does.

## Implementation Decisions

- The highest validation seam is the complete player-visible sequence from launch through planting,
  care, response, harvest, Seed Circulation, exit, and return. The three delivery gates validate
  progressively larger portions of this same seam; they are not three separate prototypes.
- Gate 1, Proof of Feeling, is deliberately allowed to omit harvest, economy, onboarding, and
  persistence. It must be labeled work in progress and must use real in-engine interactions rather
  than fabricated outcomes.
- Gate 2, Garden Moment Video, requires the complete visible care and reward arc. Any compressed
  time must be honest editing of working behavior that remains part of the playable slice.
- Gate 3, Playable Garden Moment, requires the same loop to work without edits or developer
  explanation. Garden persistence and return claims enter public messaging only at this gate.
- The slice uses one curated garden area. Broader procedural world variety is not required.
- The default first lifecycle is Purple Allium. Its harvest is a cut flower whose small surplus
  value opens Lavender as the single new planting possibility. This pairing reuses existing strong
  visual material and remains the default unless early viewer tests show that an edible crop is
  essential to understanding the fantasy.
- Tomato or strawberry is the fallback first crop when testing shows that flowers do not communicate
  harvest clearly enough. Building both routes before that evidence is out of scope.
- Plant placement follows Free-form Planting: there is no visible placement grid, while ecological
  and spatial constraints remain valid.
- The primary presentation camera is near ground level and inhabits the garden. An orbiting view may
  be used only for the final composition reveal, not as the main interaction language.
- A complete player avatar is not required. Planting must still include a world-space seed or bulb,
  readable placement preview, tool or hand cue, and synchronized soil and audio feedback so the
  interaction does not resemble a developer brush.
- One local condition, preferably moisture, is sufficient for the primary cause-and-effect chain. A
  second condition is included only if it materially improves the slice without obscuring it.
- Watering is a direct, tactile interaction. Existing simulation and irrigation technology may be
  used, but infrastructure construction is not the core task.
- The harvest result supports basic self-renewal plus one small opportunity to try a new plant.
- There are no mandatory orders, timed requests, maintenance costs, market fluctuations, or house
  upgrade requirements in the slice.
- Growth provides a visible response within 30 seconds, while the complete lifecycle retains enough
  anticipation to avoid feeling instantaneous and disposable.
- The video may compress elapsed time through honest editing or time progression; it must not depict
  behavior unavailable in the packaged demo.
- The player-facing presentation avoids debug panels, performance counters, implementation nouns,
  and source-build instructions before communicating the experience.
- The packaged demo is directly downloadable for supported platforms and does not require a player
  to install a Rust or shader-development toolchain.
- The video and packaged demo come from the same vertical slice, so promotional work also validates
  the product.
- Technical videos and open-source explanations remain separate follow-up material.
- Every accepted task must improve a named video shot or a named action in the 10-15 minute player
  path. Correctness work is also accepted when a defect blocks packaging, loses player work, or
  makes the demonstrated behavior untruthful.
- New renderer, water, audio, voxel, world-generation, or procedural-generation work enters the
  milestone only when it blocks the loop, breaks presentation quality, or produces a visible
  improvement in the final player-facing artifacts.

## Testing Decisions

- Tests judge external player behavior and presentation rather than the internal algorithm used to
  produce it.
- Gate 1 is tested first with 3-5 working adults who are not graphics-engine specialists. They see
  the clip without a technical preface and answer: what did you think the game wanted you to do,
  which moment was best, what felt like work or an editor, and would you reopen this garden tomorrow
  and why?
- Gate 1 passes when viewers can describe planting, care, and natural response; at least some
  express desire to inhabit or play the garden; and editor-like confusion is specific enough to
  guide the next interaction pass. View count alone is not the acceptance criterion.
- Gate 2 is reviewed shot by shot for one continuous causal chain: chosen position, physical action,
  environmental response, growth, harvest, and new possibility. A beautiful montage without this
  chain does not pass.
- The complete slice is validated in a release-mode packaged build on every advertised platform.
- Launch validation confirms a short path into the garden, borderless full-screen default behavior,
  safe exit, persistence, and successful resume.
- Interaction validation covers successful Free-form Planting, understandable rejection feedback,
  visible watering, local environmental response, plant growth, harvest, seed self-renewal, and one
  new planting possibility.
- Return validation covers a representative long absence: the garden retains prior work, does not
  create a rescue checklist, and may show non-destructive growth or wildness.
- Capture validation confirms that every shot in the public video depicts behavior available in the
  packaged demo and contains no debug-only UI.
- Gate 3 playtests begin without verbal instructions. In addition to the four Gate 1 questions,
  observation records where players pause, misread a tool, search for a grid, or treat the garden as
  a task list.
- A successful player-facing presentation produces comments and questions about playing, feeling,
  planting, and downloading. Technical questions remain welcome but should no longer be the only
  legible response.
- Existing deterministic tests remain appropriate for pure planting, growth, moisture, persistence,
  and economy rules. Visual feel and the complete garden loop require real app and player tests.

## Out of Scope

- A broad renderer, path tracer, DDGI, water-simulation, spatial-audio, voxel, or world-generation
  redesign that is not required by the slice.
- Large procedural worlds, multiple biomes, or an open-world progression structure.
- A large plant catalog before one plant lifecycle is satisfying and readable.
- Building both a flower-first loop and an edible-crop-first loop before audience evidence selects
  one.
- A full-body player avatar or a broad character-animation system.
- Factory automation, power networks, complex sensors, or infrastructure optimization.
- Market simulation, mandatory orders, crafting chains, debt, upkeep, or survival economics.
- Authored story campaigns, NPC relationship schedules, combat, stamina, hunger, multiplayer, and
  other systems that compete with the Restorative Garden.
- Final Steam-scale content breadth or a promise that the vertical slice is the complete game.
- New path-tracing modes, broad water-simulation rewrites, new procedural-world systems, new audio
  propagation models, or large performance projects unless a measured defect blocks a named shot,
  player action, supported machine, or packaged launch.

## Further Notes

The target Garden Moment video is cut as one continuous causal arc rather than a technology montage:

1. **0-3 seconds:** reveal a small dry garden patch and a seed or bulb in the player's possession.
2. **3-8 seconds:** place three or four plants in a natural, gridless composition.
3. **8-15 seconds:** water directly; connect moving water, wet soil, tool motion, and spatial sound.
4. **15-23 seconds:** show readable growth or bloom with wind and a restrained pollinator or ambient
   detail.
5. **23-29 seconds:** harvest one mature flower with a physical, plant-connected action.
6. **29-34 seconds:** turn the small surplus into exactly one newly available seed, preferably
   Lavender after Purple Allium.
7. **34-42 seconds:** move into or around the authored garden, let the interface recede, and end on
   the title after enough stillness for the place to breathe.

The video excludes terrain-destruction montages, pipe-network construction, debug panels,
performance numbers, technical captions, large plant catalogs, and procedural map flyovers. Those
subjects may appear later in separate developer-facing material.

The implementation order follows the gates:

1. compose the garden and capture Proof of Feeling using the closest honest version of shots 1-4;
2. test whether viewers desire the experience and whether planting reads as play rather than an
   editor;
3. fix only the interaction and presentation problems revealed by that test;
4. implement the minimal harvest and Purple Allium-to-Lavender Seed Circulation needed for shots
   5-6;
5. capture and test the complete Garden Moment Video;
6. add onboarding, packaging, safe exit, garden persistence, and truthful return behavior;
7. run unbriefed 10-15 minute playtests before presenting the slice as a downloadable demo.

Planning baseline as of 2026-08-10: the project already has strong world rendering, lighting, wind,
water, spatial audio, growth and moisture factors, planting/tool foundations, several usable plant
species, and a fruit lifecycle experiment. The player-facing harvest, currency and unlock loop are
not yet complete, and persistence currently does not cover the full authored garden. Gate planning
must treat those as real product gaps rather than implying that renderer completeness makes the demo
complete.

The milestone is successful when a stranger can describe what the player does and why it feels
desirable without first being told how the engine works.
