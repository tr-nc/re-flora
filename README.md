# Re: Flora

_Voxel Garden Island is currently in early development. Features and timelines are subject to change as the project evolves._

# Voxel Garden Island

A meditative voxel-based gardening experience where players cultivate their own island ecosystem.

## Project Overview

Voxel Garden Island is an experimental relaxation game that allows players to design and nurture their own island paradise. Using vibrant voxel rendering, players can cultivate a diverse ecosystem of plants, shape terrain, and create a personal sanctuary. The game emphasizes creativity and tranquility with no failure states, focusing instead on the joy of watching your garden evolve.

## Core Features

- **Intuitive Planting System**: Easily select, place, and nurture various plant species
- **Dynamic Ecosystem**: Watch plants grow, spread, and interact based on environmental conditions
- **Day/Night & Seasonal Cycles**: Experience visual changes and different growth patterns
- **Relaxing Atmosphere**: Meditative audio, gentle animations, and a stress-free experience

## Visual Style

The game features a distinctive voxel aesthetic with:

- Vibrant color contrasts (bright reds, lush greens)
- Multi-layered voxel elements creating rich textures
- Subtle animations like swaying plants and flowing water
- Dynamic lighting that changes with time of day and weather

## Development Roadmap

### Phase 1: Core Mechanics (Q3 2025)

- [x] Basic island generation
- [ ] Fundamental planting system
- [ ] Simple plant growth cycles
- [ ] Day/night cycle implementation
- [ ] Basic camera controls and UI

### Phase 2: Ecosystem Expansion (Q4 2025)

- [ ] Advanced plant behaviors and interactions
- [ ] Weather systems and effects
- [ ] Wildlife introduction (butterflies, birds)
- [ ] Expanded plant variety (20+ species)
- [ ] Basic photo mode

### Phase 3: Player Creativity Tools (Q1 2026)

- [ ] Terrain modification tools
- [ ] Garden decoration items
- [ ] Path creation system
- [ ] Water feature placement
- [ ] Enhanced photo and sharing capabilities

### Phase 4: Advanced Features (Q2 2026)

- [ ] Seasonal events and special plants
- [ ] Plant hybridization system
- [ ] Community challenges and sharing
- [ ] Ecosystem achievements and collections
- [ ] Enhanced audio landscape

## Botanical Reality

We're integrating elements of real-world botany, including:

- Realistic growth cycles (accelerated but proportional)
- Environmental preferences (light, soil, water needs)
- Seasonal behaviors and adaptations
- Educational elements about plant varieties

## Mini-Objectives

While Voxel Garden Island has no mandatory goals, players can engage with optional objectives:

- Themed garden challenges
- Botanical collection completion
- Ecosystem balance achievements
- Seasonal photography contests

## Technical Implementation

The project is being developed using:

- [Game Engine of Choice] with custom voxel rendering
- Optimized instancing for performance with dense vegetation
- LOD system for distance rendering
- Procedural generation with player customization

## Inspiration

This project draws inspiration from:

- The meditative aspects of gardening
- Voxel art aesthetics and capabilities
- Games focused on creativity and expression
- The natural world's beauty and complexity

## Get Involved

We welcome feedback and suggestions! If you're interested in contributing or testing:

- Join our Discord: [Discord Link]
- Follow development updates: [Dev Blog/Twitter]
- Wishlist on Steam: [Steam Page Link when available]

## Setup (deprecated)

- Use nightly build for [portable simd](https://github.com/rust-lang/portable-simd)

```shell
rustup default nightly
```

## Future Plans

1. **Water Physics**

- Ocean Rendering: Implement approximate methods instead of full simulation to maintain performance.

2. **Rendering Pipeline Enhancements**

- Add support for dynamic lighting.
- Optimize performance.
- Reduce variance in SVGF filter output to improve rendering quality.

---

# Vscode setup

## Plugins

shader lint: GLSL Lint

don't use glslx for vulkan styled shaders

## References

[Descriptor set - Vulkan's guide](https://vkguide.dev/docs/chapter-4/descriptors/#binding-descriptors)

[Descriptor set - Nvidia's guide](https://developer.nvidia.com/vulkan-shader-resource-binding)

https://rust-unofficial.github.io/patterns/idioms/coercion-arguments.html

https://rust-unofficial.github.io/patterns/patterns/structural/compose-structs.html

https://refactoring.guru/refactoring/smells

https://rust-unofficial.github.io/patterns/patterns/structural/small-crates.html

https://crates.io/crates/egui-ash-renderer

### Code

- GUI Development in Rust: [egui.rs](https://www.egui.rs/)

### Ideas & Inspirations

1. **Voxel Water Physics**:
   [Voxel Water Physics](https://www.youtube.com/watch?v=1R5WFZk86kE)

2. **Ray Tracing Sound in a Voxel World**:
   [Ray tracing Sound in a voxel world](https://www.youtube.com/watch?v=of3HwxfAoQU)

3. **Rigid Body Physics**:
   [Rigid Body Physics](https://www.youtube.com/watch?v=byP6cA71Cgw)

4. **Other Optimization Techniques**:
   [Other Optimization Techs](https://www.youtube.com/watch?v=PYu1iwjAxWM)

# Special Thanks To

[Implementation of ash with egui](https://github.com/adrien-ben/egui-ash-renderer)

[Synchronization Tutorial](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

[Official Synchronization Examples](https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples)

[Command Buffer Life Cycle](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#commandbuffers-lifecycle)

[Gamma Correction](https://www.cambridgeincolour.com/tutorials/gamma-correction.htm)

[Gamma Correction, SRGB color space](https://observablehq.com/@sebastien/srgb-rgb-gamma)
