# Product Roadmap

This roadmap defines implementation order. The canonical product identity lives in
[Game Direction](./docs/game_direction.md), and the current milestone is specified by
[First Garden Moment](./docs/first_garden_moment.md). When this roadmap conflicts with either, those
documents take priority.

## Guiding Goals

- Complete a small, honest player experience before broadening the world or technology stack.
- Make Re: Flora understandable and desirable to players without requiring technical context.
- Keep the garden active, tactile, and low-pressure.
- Preserve the author's visual and systemic taste without turning the project into a feature list.
- Release a friction-light packaged prototype, learn from player behavior, then expand toward itch.io
  and Steam.

## Current Milestone: First Garden Moment

Work is ordered by its contribution to one 30-45 second video and one 10-15 minute playable slice.
Do not start a new rendering, simulation, world-generation, spatial-audio, voxel, automation, or
infrastructure subsystem unless it directly blocks or materially improves these artifacts.

### 1. Curated Entry and Garden Area

- Start borderless full-screen and reach the playable garden with minimal menu friction.
- Build one deliberately composed area rather than expanding procedural world breadth.
- Remove or hide debug-only UI from the player path.
- Support safe exit and a truthful return experience for the current persistence capability.

### 2. Free-form Planting

- Let the player choose natural plant positions without a visible grid.
- Give planting tactile animation, particles, and spatial sound.
- Explain substrate and spacing rejection clearly through player-facing feedback.
- Make one recognizable plant lifecycle excellent before adding a larger catalog.

### 3. Local Care and Visible Response

- Use moisture as the primary local cause-and-effect factor.
- Provide one direct watering interaction.
- Make wet soil and plant response visible and understandable without a debug inspector.
- Add at most one secondary factor when it improves the slice without increasing cognitive load.

### 4. Growth and Harvest

- Create readable growth stages with a visible response within 30 seconds of care.
- Preserve enough anticipation that the full lifecycle does not feel instantaneous or disposable.
- Make harvest physical, legible, and connected to the plant the player tended.
- Retain strong visual and spatial-audio feedback throughout the interaction.

### 5. Minimal Seed Circulation

- Return enough basic seed to prevent a soft lock.
- Let surplus harvest open exactly one new plant possibility in the first slice.
- Keep prices stable and the transaction lightweight.
- Exclude mandatory orders, deadlines, upkeep, debt, and market simulation.

### 6. Player-Facing Artifacts

- Capture the honest Garden Moment video from the playable slice.
- Publish packaged builds for the advertised platforms.
- Keep the README player-facing with a stable latest-release link.
- Maintain a separate playing guide and development guide.
- Put technical breakdowns after, not before, the player experience.

### 7. Validation and Feedback

- Validate the release-mode loop from launch through planting, care, response, harvest, Seed
  Circulation, exit, and return.
- Test packaged launches on every advertised platform.
- Ask players what they thought the game wanted them to do, what they enjoyed, and what felt like
  work or obligation.
- Track whether feedback increasingly includes desire to play and download alongside implementation
  questions.

## After the First Garden Moment Is Proven

### Sustainable Return

- Preserve garden authorship across sessions.
- Let the garden remain healthy and become slightly wilder during absence without permanent loss.
- Make return motivated by curiosity rather than a rescue checklist.

### Plant and Ecological Breadth

- Add flora only when species differ visibly in care, form, harvest, or ecological response.
- Explore companion planting, pollinators, shade, substrate, and local microclimates as
  bonus layers rather than chores.
- Add collection or journal support only when it encourages observation without checklist pressure.

### Optional Garden Tools

- Explore sprinklers and pipes as expressive, understandable care tools after direct watering is
  satisfying.
- Keep automation optional and subordinate to the pleasure of inhabiting and tending the garden.
- Add sensors, cables, generators, or larger networks only after playtests show they support active
  restoration rather than engineering work.

### Atmosphere and Place

- Expand weather, time of day, ponds, insects, wind, pollen, leaves, and spatial ambience around
  player-authored garden moments.
- Add quiet environmental history without quests or mandatory exposition.
- Consider a modest home or shed only as an atmospheric anchor, not a required upgrade ladder.

## Continuing Engineering Work

### Critical Correctness

- Fix crashes, data loss, severe visual errors, broken input, and packaged-launch failures when found.
- Preserve correctness across terrain, water, flora, audio, and renderer interactions used by the
  player-facing slice.

### Performance

- Measure before changing performance-sensitive systems.
- Use release-mode app benchmarks as authoritative evidence.
- Prioritize regressions that prevent the Garden Moment from running well on advertised hardware.
- Defer speculative optimization that does not affect the current player experience.

### Platform Support

- Keep packaged Windows, macOS, and Fedora builds launchable.
- Improve platform coverage only with an explicit validation and support plan.

## Deferred Direction

- Factory-scale automation and power logistics.
- Large shops, broad order boards, market simulation, debt, and upkeep.
- Long crafting chains and inventory-management pressure.
- Authored campaigns, relationship schedules, and mandatory quests.
- Combat, survival threats, hunger, stamina pressure, and multiplayer.
- Broad open-world scope before the small Restorative Garden is compelling.
