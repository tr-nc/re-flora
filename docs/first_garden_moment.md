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

The same vertical slice produces two player-facing artifacts:

1. a 30-45 second video for discovery and sharing;
2. a friction-light 10-15 minute packaged demo for direct play.

Technical breakdowns remain a separate artifact for developer audiences. New technology work is
accepted into this milestone only when it directly blocks or materially improves the player-visible
slice.

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

## Implementation Decisions

- The highest validation seam is the complete player-visible sequence from launch through planting,
  care, response, harvest, Seed Circulation, exit, and return.
- The slice uses one curated garden area. Broader procedural world variety is not required.
- The slice uses one highly readable, recognizable plant lifecycle before expanding plant count.
- Plant placement follows Free-form Planting: there is no visible placement grid, while ecological
  and spatial constraints remain valid.
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
- New renderer, water, audio, voxel, world-generation, or procedural-generation work enters the
  milestone only when it blocks the loop, breaks presentation quality, or produces a visible
  improvement in the final player-facing artifacts.

## Testing Decisions

- Tests judge external player behavior and presentation rather than the internal algorithm used to
  produce it.
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
- Lightweight player tests ask three primary questions: what did you believe the game wanted you to
  do, which moment did you most enjoy, and what felt like work or obligation?
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
- Factory automation, power networks, complex sensors, or infrastructure optimization.
- Market simulation, mandatory orders, crafting chains, debt, upkeep, or survival economics.
- Authored story campaigns, NPC relationship schedules, combat, stamina, hunger, multiplayer, and
  other systems that compete with the Restorative Garden.
- Final Steam-scale content breadth or a promise that the vertical slice is the complete game.

## Further Notes

The public video should be cut as one continuous causal arc rather than a technology montage:

1. enter or reveal a small dry garden patch;
2. plant a natural curve or composition without a grid;
3. water directly or activate a simple watering tool;
4. show water, soil, sound, and plant response as one event;
5. progress into a readable bloom or fruit state;
6. harvest and open one new seed possibility;
7. finish on a quiet wide view long enough for the garden to breathe.

The milestone is successful when a stranger can describe what the player does and why it feels
desirable without first being told how the engine works.
