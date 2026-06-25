# Verdarium Game Direction

Status: canonical, global, continuously maintained  
Last updated: 2026-06-26

This is the single source of truth for Verdarium's game direction. Other planning docs may describe schedules, experiments, platform-specific pitches, or technical work, but they should reference this document instead of redefining the game's core direction.

## North Star

Verdarium is a relaxing, third-person voxel garden where players grow, shape, observe, and gently automate a living terrarium-like outdoor space.

The desired player feeling is:

> I shaped my little garden, changed one small thing, and the plants visibly responded.

The game should feel calm, tactile, alive, and easy to return to. It should not become a stressful survival sim, a generic crafting checklist, or a hard optimization spreadsheet.

## Core Fantasy

The player is a caretaker-tinkerer of a miniature garden world:

- They walk through the garden in third person and directly plant, trim, water, arrange, dig, smooth, and harvest.
- They tune simple local environmental conditions such as moisture, fertility, light, and soil shape.
- They place small devices that behave like approachable physical rules.
- They connect playful infrastructure such as sprinklers, drip lines, water pipes, power cables, diesel generators, kite/wind generators, mirrors, and sensors.
- They sell or fulfill requests with plant products to unlock more seeds, tools, decorative objects, house upgrades, and system pieces.
- They watch the garden become more beautiful, expressive, and personally authored over time.

This is closer to a cozy plant laboratory and personal garden than an industrial farm or passive ecosystem box.

## Garden-Scale Direction

Verdarium should grow beyond a tiny closed ecosystem box into a small garden the player can inhabit and manage. The key expansion is not bigger maps for their own sake; it is giving the player a readable place where their authored terrain, plants, devices, and home all belong together.

Good long-term garden-scale features:

- Third-person caretaking: walk to a patch, dig soil, smooth rough ground, plant seeds, harvest, and place objects with tactile feedback.
- Soil memory: terrain cells can remember simple local state such as moisture, fertility, substrate, and recent disturbance.
- Plant response: nearby grass, moss, trees, flowers, and crops become greener, denser, faster, rarer, or more animated when local conditions improve.
- Playful infrastructure: sprinklers that visibly spray, drip irrigators that pulse, water pipes that feed them, power cables that hum, and generators that visibly run.
- Toy-like power sources: a rattling diesel generator, a kite generator that dances in strong wind, tiny windmills, solar mirrors, or other charming devices.
- Cozy economy: seed packets, tool/device purchases, plant and fruit sales, small orders, and unlocks that motivate experiments without becoming market micromanagement.
- Personal home base: a small house or shed that can be upgraded cosmetically and functionally as the garden develops.

The garden may eventually feel broader than the original terrarium, but the emotional promise stays the same: small changes should produce visible local responses.

## World Construction Model

The game should use two complementary world layers:

1. **Editable collidable terrain**: the current voxel terrain remains the physical ground. It supports digging, filling, smoothing, collision, water basins, soil edits, and terrain-derived simulation fields.
2. **Surface object and rasterized detail layer**: grass, flowers, sprinklers, pipes, cables, generators, decor, shops, and house pieces can be rendered as flora instances, meshes, particles, impostors, or other non-voxel/rasterized content.

Important rules:

- Surface objects do not need voxel destructibility just because the terrain is editable.
- When terrain under an object changes, the object can resample support, settle downward, fall, tilt, or become invalid for placement through normal physics or placement rules.
- Devices should read from and write to local terrain/environment state instead of requiring the terrain system to own every gameplay object.
- This separation keeps terrain deformation powerful while letting props remain cheap, expressive, and easy to add.

## MVP Promise

The MVP must prove the game is fun before it proves the environment systems are deep.

A good MVP loop is:

1. Get or buy a seed.
2. Plant it in a small garden plot.
3. Adjust one or two local conditions: moisture, light, fertility, or soil shape.
4. See clear growth feedback within seconds.
5. Harvest flowers, cuttings, seeds, or other plant products.
6. Sell them or complete a simple order.
7. Unlock a new plant, tool, device, color variant, or decorative option.

Recommended early pacing:

- 30 seconds: the player sees a seed sprout or plant visibly respond.
- 2-3 minutes: the player completes one satisfying plant-care/harvest/sell cycle.
- 10-15 minutes: the player unlocks one new possibility.

## Experience Pillars

### 1. Visible Response

Every important system should create a readable sensory response:

- A plant changes shape, color, animation, particle output, sound, or growth rate.
- Water visibly wets soil or gathers in low places.
- Light changes the look or behavior of nearby plants.
- Devices pulse, click, glow, drip, rotate, or otherwise show that a rule fired.

If a system is mathematically interesting but invisible to the player, it is not MVP-critical.

### 2. Relaxed Agency

The player should feel responsible, not punished.

Resource imbalance should create style, variation, or opportunity rather than immediate failure. For example:

- Too much shade can create a rare mossy or dark variant.
- Drier soil can make a desert-like bloom more valuable.
- Extra water can encourage reeds, pond plants, or lush growth.

Avoid early mechanics where plants simply die because the player failed to maintain a precise balance.

### 3. Programmable, Not Intimidating

The "programmable plant box" fantasy should begin with physical rule objects, not a text-code editor.

Good MVP devices:

- Moisture sensor
- Drip irrigator or tiny sprinkler
- Water pipe or hose segment
- Simple power cable
- Sun mirror
- Shade panel
- Timer
- Wind bell
- Pollinator perch

Example rule shape:

> When soil is dry, drip water nearby.

This lets players feel clever without requiring them to learn syntax.

### 4. Small Economy, Not Generic Commerce

Buying and selling should support plant care, collection, and experimentation.

Good economy uses:

- Buy seeds, seed packets, tools, starter devices, pipes, and cables from a tiny shop or trading platform.
- Sell flowers, cuttings, fruits, seeds, or specialty harvests for coins.
- Fulfill small cozy orders.
- Unlock new plant varieties, garden tools, device toys, decorative pieces, and house upgrades.

Avoid generic market simulation, trade-route management, or long crafting chains before the plant loop is already fun.

### 5. Cozy Mystery and Beauty

The world should be relaxing, but not bland. It can include quiet mystery:

- Old stones half-buried in grass.
- A small shrine, wind bell, or broken greenhouse fragment.
- Fireflies, glowing mushrooms, fog, rain, pollen, and low-resolution water highlights.
- Small signs that the terrarium has a history beyond the player's tools.

## Environmental Response Direction

Verdarium should prioritize local, readable plant responses over global resource balancing.

Implement only a few readable local factors first:

- Water / moisture
- Fertility / nutrients
- Light / shade
- Soil / substrate and shape

These should affect:

- Growth speed
- Color and greenness
- Shape and density
- Yield
- Value
- Variant or mutation chance
- Ambient behavior

They should not require precise global balance.

Example readable interaction:

> A sprinkler is placed in soil, connected to water and power, then visibly sprays nearby ground. The surrounding soil darkens with moisture, nearby grass becomes greener and denser, and plants in range grow faster or produce a more valuable harvest.

After the plant loop is fun, environmental depth should grow as a reward layer:

- Plant diversity bonuses
- Companion planting effects
- Pollinator attraction
- Stable moisture/light microclimates
- Rare seeds from well-composed habitats
- Decorative journal discoveries

The key rule: good care grants bonuses; unusual conditions create different outcomes, not chores.

## MVP Feature Cut Line

Prioritize for MVP:

1. Fast, juicy plant growth and harvest feedback.
2. A small set of readable flora with distinct conditions and outputs.
3. Moisture/fertility/light/soil shape as soft local modifiers.
4. One approachable automation chain, such as water source -> pipe -> sprinkler -> greener plants.
5. A tiny order/sell loop that motivates experimentation.
6. A collection/journal reason to try variants.
7. Strong visual/audio response: particles, growth animation, rustle, drip, spray, chime, color change.

Defer until after the MVP is fun:

- Complex global resource simulation.
- Plant death as a normal consequence.
- Text-code programming.
- Large economy simulation.
- Long crafting chains.
- Factory-scale logistics or pressure-heavy automation.
- Combat, survival threats, hunger, stamina pressure.
- Multiplayer.
- Broad open-world scope.

## Design Decision Filter

Before adding a feature, ask:

1. Does it make the garden visibly respond to the player?
2. Does it make the game more relaxing, expressive, or delightful?
3. Does it deepen the garden/programming fantasy without adding heavy cognitive load?
4. Can it create a good 10-second GIF, screenshot, or player story?
5. Can it be tested in a small vertical slice?
6. Does it support the MVP loop directly?
7. If it adds pressure, is that pressure optional, forgiving, or framed as discovery?

If the answer is mostly no, defer it.

## Reference: Scott Rogers

The Chinese book the project is referencing is **《通关！游戏设计之道》** by **Scott Rogers**. Its English title is **_Level Up! The Guide to Great Video Game Design_**.

Useful Verdarium takeaways, adapted from Rogers' practical design framing:

### Establish the Three Cs early

Rogers emphasizes the importance of the **Three Cs**: **Character, Camera, and Control**. For Verdarium this means:

- Character: define the player fantasy as caretaker-tinkerer, not warrior, tycoon, or survivalist.
- Camera: make the small terrarium readable, cozy, and pleasant to inspect.
- Control: make planting, watering, trimming, harvesting, and placing devices immediately tactile and predictable.

Changing these late risks reworking the whole game feel.

### Gameplay first, content second

Do not produce many plants, devices, or economy items before one tiny loop is already enjoyable. A single plant that responds beautifully is more valuable than twenty plants that only differ in data.

### Readability beats hidden complexity

The player should understand what happened and why. Prefer clear animation, color, sound, icons, and local feedback over invisible formulas.

### Teach through play

The first tutorial should be a small guided action chain, not a manual:

1. Plant this seed.
2. Water the dry soil.
3. Watch it sprout.
4. Move the sun mirror.
5. Harvest the flower.
6. Sell or fulfill one request.

### Build around memorable moments

Every major system should create moments players can remember or share: a bloom opening, a sensor triggering a drip line, butterflies rising from flowers, a rare color appearing at dusk.

## Relationship to Other Docs

- `ROADMAP.md` should describe implementation order, not redefine the game's identity.
- `docs/steam_direction.md` may describe Steam/storefront positioning, but it should remain subordinate to this global direction.
- Technical progress docs should explain implementation details and validation, not create separate product goals.

When direction changes, update this document first, then adjust roadmap or task docs to match.

## References

- Scott Rogers, _Level Up! The Guide to Great Video Game Design_, Wiley.
- Chinese edition: 《通关！游戏设计之道》, Scott Rogers.
- Wiley listing: <https://www.wiley.com/en-us/Level+Up!+The+Guide+to+Great+Video+Game+Design%2C+3rd+Edition-p-9781394298761>
- Google Books listing: <https://books.google.com/books/about/Level_Up.html?id=8w_ETFmHrewC>
