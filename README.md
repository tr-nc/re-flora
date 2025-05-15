# Re: Flora

_Re: Flora is currently in early & actively development. Features and timelines are subject to change as the project evolves._

## Overview

Voxel Garden Island is an experimental relaxation game that allows players to design and nurture their own island paradise. Using vibrant voxel rendering, players can cultivate a diverse ecosystem of plants, shape terrain, and create a personal sanctuary. The game emphasizes creativity and tranquility with no failure states, focusing instead on the joy of watching your garden evolve.

This project aims to give players:

> A meditative voxel-based gardening experience where players cultivate their own island ecosystem.

### Core Features

- **Intuitive Planting System**: Easily select, place, and nurture various plant species
- **Dynamic Ecosystem**: Watch plants grow, spread, and interact based on environmental conditions
- **Day/Night & Seasonal Cycles**: Experience visual changes and different growth patterns
- **Relaxing Atmosphere**: Meditative audio, gentle animations, and a stress-free experience

### Botanical Reality

We're integrating elements of real-world botany, including:

- Realistic growth cycles (accelerated but proportional)
- Environmental preferences (light, soil, water needs)
- Seasonal behaviors and adaptations
- Educational elements about plant varieties

### Mini-Objectives

While Voxel Garden Island has no mandatory goals, players can engage with optional objectives:

- Themed garden challenges
- Botanical collection completion
- Ecosystem balance achievements
- Seasonal photography contests

### Inspiration

This project draws inspiration from:

- The meditative aspects of gardening
- Voxel art aesthetics and capabilities
- Games focused on creativity and expression
- The natural world's beauty and complexity

## Get ready for development!

### Rust Setup

Simply use the stable channel of the latest version of rust.

<!--
- Use nightly build for [portable simd](https://github.com/rust-lang/portable-simd)

```shell
rustup default nightly
``` -->

### VSCode Plugins

| Name               | Usage                           |
| ------------------ | ------------------------------- |
| shader lint        | Shader Linting                  |
| rust-analyzer      | For rust linting, formatting... |
| to be continued... | ...                             |

Notes:

- don't use glslx for vulkan styled shaders

## References

### Tech

#### Vulkan Related

[Descriptor set - Vulkan's guide](https://vkguide.dev/docs/chapter-4/descriptors/#binding-descriptors)

[Descriptor set - Nvidia's guide](https://developer.nvidia.com/vulkan-shader-resource-binding)

#### Rust Related

https://refactoring.guru/refactoring/smells

https://rust-unofficial.github.io/patterns/patterns/structural/small-crates.html

### Inspirations

[Ray Traced Reverb, Wind and Sound Occlusion (Path Traced Voxel Project)](https://www.youtube.com/watch?v=UHzeQZD9t2s)

[Exploring an Infinite Voxel Forest](https://www.youtube.com/watch?v=1wufuXY3l1o)

[Animated Voxel Trees - Detail Enhancement Preview](https://www.youtube.com/watch?v=BObFTsNeeGc)

[Voxel Water Physics](https://www.youtube.com/watch?v=1R5WFZk86kE)

[Ray tracing Sound in a voxel world](https://www.youtube.com/watch?v=of3HwxfAoQU)

[Rigid Body Physics](https://www.youtube.com/watch?v=byP6cA71Cgw)

[Other Optimization Techs](https://www.youtube.com/watch?v=PYu1iwjAxWM)

- Grass
  [How I added particles!](https://www.youtube.com/watch?v=rf9Piwp91pE)

  [Animated Voxel Grass](https://www.youtube.com/watch?v=dGZDXaEXTOI)

[CG tutorials](https://www.scratchapixel.com/)

### Ray Tracing

[Guide](https://www.khronos.org/blog/ray-tracing-in-vulkan/)

[Shading doc](https://github.com/KhronosGroup/GLSL/blob/main/extensions/ext/GLSL_EXT_ray_query.txt/)

[Ray Tracing Pipeline vs Ray Query](https://tellusim.com/rt-perf/)

[Best Practices 1](https://developer.nvidia.com/blog/rtx-best-practices/)

[Best Practices 2](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)

[Another View on the Classic Ray-AABB Intersection Algorithm for BVH Traversal](https://medium.com/@bromanz/another-view-on-the-classic-ray-aabb-intersection-algorithm-for-bvh-traversal-41125138b525)

[A guide to fast voxel ray tracing using sparse 64-trees](https://dubiousconst282.github.io/2024/10/03/voxel-ray-tracing/)

[Voxel RT different voxel format benchmark](https://github.com/dubiousconst282/VoxelRT)

[Reddit discussion](https://www.reddit.com/r/VoxelGameDev/comments/1fzimke/a_guide_to_fast_voxel_ray_tracing_using_sparse/)

# Special Thanks To

[Implementation of ash with egui](https://github.com/adrien-ben/egui-ash-renderer)

[Synchronization Tutorial](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

[Official Synchronization Examples](https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples)

[Command Buffer Life Cycle](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#commandbuffers-lifecycle)

[Gamma Correction](https://www.cambridgeincolour.com/tutorials/gamma-correction.htm)

[Gamma Correction, SRGB color space](https://observablehq.com/@sebastien/srgb-rgb-gamma)
