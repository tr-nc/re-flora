# Re: Flora Game Direction

Status: canonical, global, continuously maintained  
Last updated: 2026-08-10

This is the single source of truth for Re: Flora's game direction. Current decisions in this
document supersede older plans when they disagree. Roadmaps, demo specifications, storefront copy,
and technical plans must remain subordinate to it.

## North Star

Re: Flora is a beautiful, inhabitable 3D Restorative Garden for adults who want a deliberate break
from modern work.

The player freely plants, tends, shapes, observes, and harvests a small living garden. Every small
action should have a clear sensory consequence. The garden gradually becomes sustainable and
personal, but it never punishes the player for leaving.

The desired player feeling is:

> I spent time with my garden. It responded to me, and it became more mine.

Re: Flora should feel calm, tactile, alive, and easy to return to. It must not become a stressful
survival sim, a generic crafting checklist, an optimization spreadsheet, or a second job disguised
as a cozy game.

## The Real-World Problems

Re: Flora is not justified only by a market gap for a prettier 3D planting game. Its purpose is to
address four related player problems.

### Work Has No Clear Ending

Modern screen-based work is abstract, fragmented, and often unfinished when the workday ends. A
borderless full-screen launch creates a deliberate transition into a separate place. Fast resume,
safe exit, and no absence penalty preserve that boundary instead of adding another obligation.

### Work Often Lacks Tangible Causality

Many adult players spend their days on delayed, invisible, or collectively owned outcomes. Garden
care should provide the opposite: a concrete action, a legible local response, and a lasting change
in a space that belongs to the player.

### Relaxation Games Often Trade Agency for Calm

Some calm games are passive background objects, while many farming and life simulations add chores,
schedules, stories, and optimization pressure. Re: Flora should provide active restoration: the
player does meaningful things, but the game does not demand continued attendance.

### Players Need a Personally Authored Place

The garden should preserve the player's choices and become recognizably theirs. Placement and care
are expressive acts, not a puzzle about filling a prescribed grid efficiently.

## Player Job

> When a day of work still occupies my attention, I want to enter a bounded natural place and care
> for something concrete at low risk, so that I can recover calm, agency, and a sense of completion.

## Audience

The core audience is working adults who play on PC, value beautiful real-time worlds and tactile
systemic response, and dislike task lists, social-story obligations, and efficiency pressure.

Working men aged 20 and above are the initial research and communication persona because the project
is grounded in that lived experience. This is not an exclusion rule: the product should be defined
by the need for active restoration, not by gender-coded mechanics or aesthetics.

## Core Fantasy

The player inhabits and cares for a small garden world:

- They directly plant, trim, water, arrange, dig, smooth, observe, and harvest.
- They place plants in natural compositions rather than filling visible grid slots.
- They learn a few readable local conditions such as moisture, light, substrate, and soil shape.
- They see plants and terrain respond through growth, form, color, motion, particles, sound, yield,
  and ecological variation.
- They harvest enough surplus to try new plants without turning money into a score or obligation.
- They gradually establish a Sustainable Garden that retains their work and remains healthy while
  they are away.

This is a personal garden and an active refuge. It is not an industrial farm, a passive ecosystem
box, or a programmable factory whose main pleasure is throughput optimization.

## Experience Contract

### Full-Screen Separation

The game starts borderless full-screen by default as an intentional boundary from the desktop and
work context. This is a player-chosen ritual, not an attempt to prevent task switching.

The supporting experience should provide:

- a short path from launch to the last garden state;
- minimal front-loaded menus and exposition;
- low-distraction player UI;
- safe pause and exit at any time;
- no daily-login reward, deadline, or loss caused by absence.

The game should not behave like a desktop companion or live in a screen dock. It asks for focused
presence for a while, then lets the player leave cleanly.

### Free-form Planting

Free-form Planting is a player-facing interaction promise, not a claim that the simulation contains
no voxels, cells, spacing rules, or discrete data.

- The player should not align plants to visible squares or predefined plots.
- The chosen position and composition should feel continuous and authored.
- Substrate, available space, water, and other ecological conditions may still affect what can grow.
- Invalid placement should be explained through the world and clear feedback rather than arbitrary
  red-grid rules.

### Gentle Consequence

Care must matter, but unusual or poor conditions should create recoverable differences rather than
punishment.

- Dry soil may slow growth or favor a different form.
- Shade may produce moss, darker color, or a rare variant.
- Extra water may encourage reeds, pond plants, or lush growth.
- Returning care should recover ordinary plants; normal absence should not cause irreversible death.

"No punishment" does not mean that every choice produces the same result. Meaning comes from
legible differences, discovery, and response.

### Sustainable Return

A player returning after days or weeks should find the garden intact. It may have grown, seeded, or
become slightly wilder, but it should not have erased prior work or accumulated a rescue checklist.

The desired return motivation is curiosity:

> I wonder what grew while I was away.

It must not be fear:

> I need to return before everything dies.

## Care and Seed Circulation

The core care loop is:

1. Receive, retain, or acquire a seed.
2. Choose a place and plant it through Free-form Planting.
3. Adjust one readable local condition such as moisture, light, or soil shape.
4. Observe clear growth and environmental response.
5. Harvest flowers, fruit, cuttings, seeds, or another recognizable product.
6. Keep enough basic seed for the garden to renew itself.
7. Use surplus value to try a new plant or garden possibility.

Seed Circulation exists to extend plant care and expression.

- Basic plants must not require repeated purchases to prevent a soft lock.
- Currency may unlock variety, but it should not pay rent, taxes, maintenance, or survival costs.
- Prices should be stable and understandable before any market variation is considered.
- There are no mandatory timed orders or daily sales targets.
- Requests, if explored later, must be optional invitations to try a plant rather than obligations.
- House upgrades, large shops, and broad device economies are not part of the first proof.

## Story and Atmosphere

Re: Flora does not need an authored plot, dialogue campaign, relationship progression, or quest log.
The Personal Garden Story comes from what the player planted, what changed, and what they remember.

Atmosphere may still contain quiet history and mystery:

- old stones half-buried in grass;
- a wind bell, small shrine, or greenhouse fragment;
- fireflies, glowing mushrooms, fog, rain, pollen, and changing light;
- natural signs that make the world feel older than the current session.

These elements should create mood and curiosity without demanding exposition or completion.

## Technology Serves the Garden Moment

Re: Flora's renderer, voxel representation, water simulation, procedural generation, ray-traced
spatial audio, lighting, and physics are important creative materials. They are not the player's
reason to care by themselves.

The current development priority is to integrate existing systems into a Garden Moment that a
stranger can understand without knowing how it is implemented. A new technology subsystem is not a
product milestone unless it directly unlocks or materially improves that player-visible moment.

Developer-facing technical explanations remain valuable, but they should follow a player-facing
experience rather than replace it.

## Current Milestone: The First Garden Moment

The next product milestone is defined in [First Garden Moment](./first_garden_moment.md).

It must produce two artifacts from the same honest gameplay slice:

1. A 30-45 second player-facing video that communicates desire, action, response, and atmosphere.
2. A friction-light 10-15 minute packaged demo in which a player completes one care, harvest, and
   Seed Circulation loop.

The slice should use one curated garden area, one clearly readable plant lifecycle, one direct
planting action, one watering interaction, visible environmental response, one harvest, and one new
possibility. It should end with enough quiet time for the garden to breathe.

This milestone takes priority over adding another broad rendering, simulation, world-generation,
audio, automation, or infrastructure subsystem.

## Experience Pillars

### 1. Visible Response

Every important system should create a readable sensory response:

- plants change shape, color, animation, particle output, sound, or growth rate;
- water visibly moves, wets soil, or gathers in low places;
- light and shade visibly affect nearby life;
- harvest feels physical and connected to the cared-for plant.

If a system is mathematically interesting but invisible or illegible to the player, it is not
critical to the current milestone.

### 2. Relaxed Agency

The player should feel responsible and expressive, not monitored or punished. The game creates
Gentle Consequences and recoverable variation instead of fragile optimal states.

### 3. Spatial Authorship

Terrain shape, plant position, local composition, and the traces of care should make one player's
garden visibly different from another's.

### 4. Calm Concentration

The game is not passive wallpaper. Planting, watering, shaping, and harvesting require presence, but
avoid the cognitive load of schedules, long recipes, inventory sorting, and task management.

### 5. Natural Beauty With Restraint

The world should be beautiful, tactile, and slightly mysterious rather than purely cute. Visual and
audio richness should support a coherent garden mood rather than compete as disconnected effects.

## Pacing

Use multiple response times instead of compressing the whole game into a mobile-style reward loop.

- Within 30 seconds, the player should see that a plant or patch responds to care.
- Within a 10-15 minute first demo, the player should complete one understandable care and harvest
  loop and open one new possibility.
- Across later sessions, the garden should preserve authorship, mature, and become sustainable.
- A promotional video may compress time, but it must depict honest game behavior.

## World Construction Model

The game uses two complementary world layers:

1. **Editable collidable terrain**: voxel terrain is the physical ground for digging, filling,
   smoothing, collision, water basins, soil edits, and terrain-derived simulation fields.
2. **Surface objects and rasterized detail**: plants, water tools, pipes, decor, and other props may
   use flora instances, meshes, particles, impostors, or other non-voxel representations.

Important rules:

- Surface objects do not require voxel destructibility just because terrain is editable.
- Objects should react understandably when supporting terrain changes: settle, fall, tilt, move, or
  become invalid through physical placement rules.
- Gameplay objects may read and write local terrain state without making terrain own every system.
- Internal representation choices must not compromise Free-form Planting or readable response.

## Feature Cut Line

Prioritize for the First Garden Moment:

1. One polished Free-form Planting interaction.
2. One recognizable plant with readable growth and harvest states.
3. Moisture and one other local factor as soft, visible modifiers.
4. One tactile watering interaction.
5. One minimal Seed Circulation transaction that opens a new plant possibility.
6. Strong visual and spatial-audio response.
7. A curated area, direct packaged download, clean launch, and safe return.

Consider only after the core moment is proven:

- optional sprinklers, pipes, sensors, and gentle automation;
- more plants and ecological relationships;
- collection or journal support without checklist pressure;
- decorative and home-base expansion;
- optional requests that encourage experiments without deadlines;
- broader weather, time, and garden-scale variation.

Defer:

- complex global resource simulation;
- normal plant death or absence decay;
- factory-scale logistics and pressure-heavy automation;
- market simulation, debt, maintenance costs, or long crafting chains;
- authored story campaigns, mandatory quests, and relationship schedules;
- combat, survival threats, hunger, stamina pressure, and multiplayer;
- broad open-world scope before the small garden is compelling.

## Design Decision Filter

Before adding or polishing a feature, ask:

1. Does it make a Garden Moment clearer, more tactile, or more beautiful?
2. Does it strengthen active restoration rather than create another obligation?
3. Does it deepen spatial authorship or readable environmental response?
4. Can a player understand its value without a technical explanation?
5. Does it materially improve the 30-45 second video or 10-15 minute playable slice?
6. If it adds pressure, is that pressure optional, forgiving, and framed as discovery?
7. Is it more important than completing the current player-facing loop?

If the answer is mostly no, defer it.

## Relationship to Other Docs

- `CONTEXT.md` owns canonical product and rendering terms, not plans.
- [First Garden Moment](./first_garden_moment.md) specifies the current demo milestone.
- `ROADMAP.md` describes implementation order and must follow this document.
- `docs/steam_direction.md` describes storefront presentation and must follow this document.
- Technical progress and research docs record implementation evidence; they do not create product
  priorities.

When direction changes, update this document first, then reconcile all subordinate documents.

## Reference: Scott Rogers

The project references Scott Rogers' *Level Up! The Guide to Great Video Game Design* (Chinese
edition: *《通关！游戏设计之道》*).

Relevant principles for Re: Flora:

- Establish character, camera, and control around embodied garden care before producing breadth.
- Prove one enjoyable action-and-response loop before adding many plants or devices.
- Prefer readable feedback over hidden complexity.
- Teach through direct planting, watering, observation, harvest, and reinvestment rather than a
  manual or story sequence.
- Build memorable moments that players want to experience and share.
