# Product Roadmap

This roadmap captures planned improvements for gameplay, visuals, and performance.
Items are ordered by priority and intended implementation sequence.

The canonical product direction lives in [docs/game_direction.md](./docs/game_direction.md). Keep this roadmap as an implementation plan that follows that direction rather than a second source of truth.

## Guiding Goals

Goals matter - even for a small cozy game that could keep evolving forever. They help guide decisions, keep development focused, and make it easier to measure progress.

- **Long-term goals**
  - Share development updates regularly on Reddit and YouTube.
  - Prioritize players first, while also creating value for other developers.
  - Release a playable version on itch.io first, then expand to Steam.

- **Short-term goals**
  - Build a minimum viable product first and make it playable.
  - Test market interest and gather feedback through Reddit and YouTube.

## 1) Critical Fixes

- No critical bugfixes are currently tracked.

## 2) High-Priority Features

- **Third-person garden loop vertical slice**
  - Turn the current terrarium feel into a small player-managed garden plot.
  - Support a short loop: buy or receive seed packet -> plant -> adjust soil/moisture -> observe response -> harvest -> sell or fulfill order -> unlock one new item.
  - Keep the first slice small enough to prove fun within 2-3 minutes.

- **Local soil state and plant response**
  - Track simple per-area soil state such as moisture and fertility.
  - Let grass/plants visibly respond through greenness, density, growth speed, yield, or particles.
  - Prioritize readable feedback over deep simulation.

- **First utility device chain**
  - Prototype a tiny sprinkler or drip irrigator that can be placed on/into soil.
  - Feed it through a simple water pipe/hose and optional power cable.
  - Show obvious activation feedback: spray, wet soil, sound, plant response.

- **Surface object layer behavior**
  - Treat grass, devices, pipes, cables, generators, decor, shops, and house pieces as non-voxel surface objects above editable terrain.
  - When terrain below them changes, resample support and let objects settle, fall, tilt, or become invalid rather than making every prop destructible.

- **Terrain harvesting feedback**
  - Add particle effects at terrain-edit positions.
  - Emit particles matching voxel color.
  - Animate particles toward the player camera to indicate collection into backpack storage.

- **MacOS adaption**
  - Make sure everything works properly in a reasonable framerate in macOS
  - Make sure bootstrap_macos.sh work properly

## 3) Visual & World Expansion

- **Cozy garden economy objects**
  - Add a tiny shop, trading platform, or order board for seeds, devices, pipes, cables, and plant/fruit sales.
  - Keep commerce lightweight and directly tied to plant experimentation.

- **Home base / shed direction**
  - Add a small player house or garden shed as a long-term upgrade anchor.
  - Prefer cosmetic and gentle utility upgrades over survival pressure.

- **Playful generators**
  - Explore toy-like power sources such as a rattling diesel generator, kite/wind generator, tiny windmill, or solar mirror.
  - Make each generator strongly animated and readable before adding many variants.

- **Reflective pond biome element**
  - Add a small pond with SSR reflections for terrain and flora.

- **Ocean presentation pass**
  - Create a more pixelized ocean look.
  - Continue visual research and prototyping.

- **Additional flora types**
  - Expand flora variety to improve biome richness.

- **Stylized cloud system**
  - Add clouds with a strong pixel-art aesthetic.

## 4) Performance Work

## 5) Low-Priority Tooling

- **Formatter enforcement**
  - Add PR formatting checks so formatting is enforced even when contributors forget to run local tools.
  - Pin formatter versions in CI for reproducible results.
  - Consider a single repo-managed formatting entrypoint so Rust and shader formatting are easier to run consistently.
