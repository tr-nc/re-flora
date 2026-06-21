# Verdarium Game Direction

Status: canonical, global, continuously maintained  
Last updated: 2026-06-21

This is the single source of truth for Verdarium's game direction. Other planning docs may describe schedules, experiments, platform-specific pitches, or technical work, but they should reference this document instead of redefining the game's core direction.

## North Star

Verdarium is a relaxing, programmable voxel terrarium where players grow, shape, observe, and gently automate a living plant box.

The desired player feeling is:

> I shaped a tiny plant world, changed one small thing, and the plants visibly responded.

The game should feel calm, tactile, alive, and easy to return to. It should not become a stressful survival sim, a generic crafting checklist, or a hard optimization spreadsheet.

## Core Fantasy

The player is a caretaker-tinkerer of a miniature living world:

- They plant, trim, water, arrange, and harvest flora.
- They tune simple environmental conditions such as water, light, and soil.
- They place small devices that behave like approachable programming rules.
- They sell or fulfill requests with plant products to unlock more seeds, tools, and decorative/system pieces.
- They watch the terrarium become more beautiful, expressive, and personally authored over time.

This is closer to a cozy plant laboratory than an industrial farm.

## MVP Promise

The MVP must prove the game is fun before it proves the environment systems are deep.

A good MVP loop is:

1. Get or buy a seed.
2. Plant it in a small voxel terrarium.
3. Adjust one or two local conditions: water, light, or soil.
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
- Drip irrigator
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

- Buy seeds or starter devices.
- Sell flowers, cuttings, seeds, or specialty harvests.
- Fulfill small cozy orders.
- Unlock new plant varieties and terrarium tools.

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
- Light / shade
- Soil / substrate

These should affect:

- Growth speed
- Color
- Shape
- Yield
- Value
- Variant or mutation chance
- Ambient behavior

They should not require precise global balance.

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
3. Water/light/soil as soft local modifiers.
4. One approachable automation rule device chain.
5. A tiny order/sell loop that motivates experimentation.
6. A collection/journal reason to try variants.
7. Strong visual/audio response: particles, growth animation, rustle, drip, chime, color change.

Defer until after the MVP is fun:

- Complex global resource simulation.
- Plant death as a normal consequence.
- Text-code programming.
- Large economy simulation.
- Long crafting chains.
- Combat, survival threats, hunger, stamina pressure.
- Multiplayer.
- Broad open-world scope.

## Design Decision Filter

Before adding a feature, ask:

1. Does it make the terrarium visibly respond to the player?
2. Does it make the game more relaxing, expressive, or delightful?
3. Does it deepen the plant-box/programming fantasy without adding heavy cognitive load?
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
