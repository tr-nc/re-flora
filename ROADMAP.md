# Product Roadmap

This roadmap captures planned improvements for gameplay, visuals, and performance.
Items are ordered by priority and intended implementation sequence.

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

- **Trading system**
  - Add a docked ship that enables resource exchange. (Draw the docked ship, the port, with primitives during world init time
  - Support buy/sell interactions for player inventory.

- **Terrain harvesting feedback**
  - Add particle effects at terrain-edit positions.
  - Emit particles matching voxel color.
  - Animate particles toward the player camera to indicate collection into backpack storage.

- **MacOS adaption**
  - Make sure everything works properly in a reasonable framerate in macOS
  - Make sure bootstrap_macos.sh work properly

## 3) Visual & World Expansion

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
