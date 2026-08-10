# Steam Direction

The canonical global direction lives in [Game Direction](./game_direction.md). This document governs
storefront positioning and player-facing presentation; it must not redefine the game's identity or
elevate a technical system above the current product milestone.

## Core Positioning

**A beautiful full-screen voxel garden that responds to your care without becoming another job.**

Re: Flora is an active Restorative Garden. Players enter a small living place, plant freely, shape
soil, tend local conditions, harvest, and gradually make the garden sustainable and personal.

The storefront promise is:

> Enter your garden, change one small thing, and watch the living world respond.

Low-resolution rendering, voxel terrain, water, procedural plants, wind, insects, particles,
lighting, and spatial sound make that response unusually tangible. They support the promise; they
are not the pitch by themselves.

## Intended Player

The initial audience is PC-playing adults who want a deliberate break after work and appreciate
beautiful systemic worlds without schedules, relationship obligations, survival pressure, or
efficiency management.

Working men aged 20 and above are the initial communication persona, not an exclusion rule. Store
copy should lead with the shared need for active restoration rather than gender-coded features.

## Author-Led and Player-Legible

The project remains author-led: it follows the creator's taste, curiosity, and constraints instead
of accumulating a generic feature checklist. Player feedback is most useful when it reveals whether
the intended feeling reached strangers.

Listen for:

- whether viewers understand what the player does in the first few seconds;
- whether they describe the garden as a place they want to enter;
- whether action and environmental response are legible without technical explanation;
- where calm concentration turns into boredom or task pressure;
- whether people ask how to play or download, not only how the renderer works;
- which moments they want to share as clips or screenshots.

Do not automatically follow requests for combat, multiplayer, survival, large crafting systems,
relationship campaigns, or generic progression. They may fit other games but dilute the Restorative
Garden.

## Separate Audience Artifacts

Re: Flora serves three related audiences with different artifacts:

| Audience | Primary artifact | First question answered |
| --- | --- | --- |
| Viewer | 30-45 second Garden Moment video | Why do I want to enter this place? |
| Player | 10-15 minute packaged vertical slice | What can I do, and how does it feel? |
| Developer | Source repository and technical breakdowns | How was it built? |

The player-facing video and build come first in storefront presentation. Technical material remains
valuable follow-up content but should not require the viewer to understand Vulkan, path tracing,
voxel structures, or simulation architecture before wanting the game.

## Experience Pillars

### 1. Immediate Garden Causality

Show a complete Garden Moment: plant, water or reshape, observe a visible response, harvest, and open
one new possibility. A feature that cannot contribute to a clear action-and-response chain is not a
current storefront priority.

### 2. Free-form Spatial Authorship

Show plants arranged in natural curves and personal compositions without visible grid slots. The
garden should look authored by its player rather than solved for efficiency.

### 3. Calm Concentration

The player is active, not idle. Tactile planting, watering, shaping, and harvesting hold attention
without timers, daily chores, narrative demands, or fear of absence.

### 4. Low-Resolution Living Nature

Voxel terrain, downsampled presentation, intentional temporal stepping, wind, water, grass,
particles, creatures, and changing light form a coherent visual identity. The slight old-machine
feeling is intentional, not a compromise.

### 5. Peaceful, Natural, Slightly Mysterious

Avoid making the tone purely cute. Wind, loneliness, old stones, fog, fireflies, a quiet bell, or a
greenhouse fragment may create atmosphere without starting an authored story or quest chain.

## Current Storefront Proof

The current proof target is [First Garden Moment](./first_garden_moment.md), produced as honest video
and playable artifacts from the same slice.

The video should form one causal arc:

1. reveal a small garden patch;
2. plant a natural composition;
3. water directly or activate one simple watering tool;
4. show water, soil, plant motion, light, and spatial sound respond together;
5. progress to a readable bloom or fruit;
6. harvest and open one new seed possibility;
7. finish on a quiet wide view long enough for the garden to breathe.

Do not lead this video with implementation text, debug UI, performance counters, or a technology
montage. A separate breakdown can explain the engineering after desire is established.

## Strong Storefront Moments

- A freely planted curve becoming visible as it sprouts.
- Water following shaped terrain and darkening nearby soil.
- A plant changing form or color as local conditions improve.
- A harvest that remains physically connected to the cared-for plant.
- Wind moving through a garden the player personally arranged.
- Butterflies rising from a newly flowering patch.
- Dusk settling over an intact garden as the player stops working.

Atmospheric weather, creatures, and mystery are valuable when they strengthen one of these player
moments rather than compete as isolated features.

## Storefront Conversion

The public repository and store entry should provide:

- a player-facing description before build or technology details;
- the Garden Moment video near the first screen;
- a stable link to the latest packaged release rather than a version-specific asset;
- a short playing guide and honest statement of prototype scope;
- source-build and engineering material in separate contributor documentation.

The relevant success signal is not views alone. The presentation should increasingly produce player
language such as "I want to play," "this feels peaceful," and "where can I download it," alongside
technical praise.

## One-Line Pitch

> A full-screen voxel garden where you plant freely, shape a living world, and return without
> obligation.
