# 🌱 Re: Flora 🏝️

> **Note:** _Re: Flora is currently in early & active development. Features and timelines are subject to change as the project evolves._

## 🎮 Overview

**Re: Flora** is an experimental relaxation game that allows players to design and nurture their own island paradise. Using vibrant voxel rendering, players can cultivate a diverse ecosystem of plants, shape terrain, and create a personal sanctuary. The game emphasizes creativity and tranquility with no failure states, focusing instead on the joy of watching your garden evolve.

This project aims to give players:
> A meditative voxel-based gardening experience where players cultivate their own island ecosystem.

## ✨ Features

### Core Gamepay

- **Intuitive Planting System**: Easily select, place, and nurture various plant species.

- **Dynamic Ecosystem**: Watch plants grow, spread, and interact based on environmental conditions.

- **Day/Night & Seasonal Cycles**: Experience visual changes and different growth patterns.

- **Relaxing Atmosphere**: Meditative audio, gentle animations, and a stress-free experience.

### Botanical Reality

We're integrating elements of real-world botany, including:

- Realistic growth cycles (accelerated but proportional).
- Environmental preferences (light, soil, water needs).
- Seasonal behaviors and adaptations.
- Educational elements about plant varieties.

### Mini-Objectives

While **Re: Flora** has no mandatory goals, players can engage with optional objectives:

- Themed garden challenges.
- Botanical collection completion.
- Ecosystem balance achievements.
- Seasonal photography contests.

## 🎨 Inspiration

This project draws inspiration from:

- The meditative aspects of gardening.
- Voxel art aesthetics and capabilities.
- Games focused on creativity and expression.
- The natural world's beauty and complexity.

## 🛠️ Getting Started with Development

### Rust Setup

Ensure you are using the latest stable version of Rust.

```sh
rustup update stable
```

### Recommended VSCode Extensions

| Name | Usage |
| :--- | :--- |
| `shader-lint` | For GLSL/HLSL shader linting. |
| `rust-analyzer` | Provides language support for Rust (linting, formatting, etc.). |
| _to be continued..._ | _..._ |

> **Note:** Do not use `glslx` for Vulkan-style shaders.

---

## 📚 Resources & References

### Vulkan

- [Descriptor set - Vulkan Guide](https://vkguide.dev/docs/chapter-4/descriptors/#binding-descriptors)
- [Descriptor set - Nvidia's Guide](https://developer.nvidia.com/vulkan-shader-resource-binding)
- [Vulkan Synchronization Explained](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

### Ray Tracing

- [Official Khronos Guide to Ray Tracing in Vulkan](https://www.khronos.org/blog/ray-tracing-in-vulkan/)
- [GLSL_EXT_ray_query Shading Documentation](https://github.com/KhronosGroup/GLSL/blob/main/extensions/ext/GLSL_EXT_ray_query.txt/)
- [Ray Tracing Pipeline vs. Ray Query Performance](https://tellusim.com/rt-perf/)
- [NVIDIA RTX Best Practices (1)](https://developer.nvidia.com/blog/rtx-best-practices/)
- [NVIDIA RTX Best Practices (2 - Updated)](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)
- [A Guide to Fast Voxel Ray Tracing using Sparse 64-trees](https://dubiousconst282.github.io/2024/10/03/voxel-ray-tracing/)
  - [Associated GitHub Project](https://github.com/dubiousconst282/VoxelRT)
  - [Reddit Discussion](https://www.reddit.com/r/VoxelGameDev/comments/1fzimke/a_guide_to_fast_voxel_ray_tracing_using_sparse/)
- [Another View on the Classic Ray-AABB Intersection Algorithm](https://medium.com/@bromanz/another-view-on-the-classic-ray-aabb-intersection-algorithm-for-bvh-traversal-41125138b525)
- [Understanding BRDF and PDF for Sampling](https://computergraphics.stackexchange.com/questions/8578/how-to-set-equivalent-pdfs-for-cosine-weighted-and-uniform-sampled-hemispheres)

### Papers

- [ReSTIR GI: Path Resampling for Real-Time Path Tracing](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)

### Inspirational Tech & Art

- **Procedural Generation**: [Procedural Island Generator in Blender](https://blenderartists.org/t/procedural-island-generator-illustration-using-blenders-geometry-nodes/1483314)
- **Voxel Worlds**:
  - [Exploring an Infinite Voxel Forest](https://www.youtube.com/watch?v=1wufuXY3l1o)
  - [Animated Voxel Trees - Detail Enhancement](https://www.youtube.com/watch?v=BObFTsNeeGc)
- **Physics & Simulation**:
  - [Voxel Water Physics](https://www.youtube.com/watch?v=1R5WFZDXaEXTOI)
  - [Rigid Body Physics](https://www.youtube.com/watch?v=byP6cA71Cgw)
- **Audio**:
  - [Ray Traced Reverb, Wind and Sound Occlusion](https://www.youtube.com/watch?v=UHzeQZD9t2s)
  - [Ray Tracing Sound in a Voxel World](https://www.youtube.com/watch?v=of3HwxfAoQU)
- **Particles & Effects**:
  - [Animated Voxel Grass](https://www.youtube.com/watch?v=dGZDXaEXTOI)
  - [How I added particles! (Grass)](https://www.youtube.com/watch?v=rf9Piwp91pE)
- **General Graphics & Optimization**:
  - [Other Optimization Techs](https://www.youtube.com/watch?v=PYu1iwjAxWM)
  - [Scratchapixel CG Tutorials](https://www.scratchapixel.com/)

---

## 🙏 Special Thanks To

- **[adrien-ben/egui-ash-renderer](https://github.com/adrien-ben/egui-ash-renderer)** for the implementation of `ash` with `egui`.
- **TheMaister's Blog** for the excellent [Vulkan Synchronization Tutorial](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/).
- **Khronos Group** for the official [Vulkan Synchronization Examples](https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples) and documentation on the [Command Buffer Lifecycle](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#commandbuffers-lifecycle).
- **Cambridge in Colour** for the clear tutorial on [Gamma Correction](https://www.cambridgeincolour.com/tutorials/gamma-correction.htm).
- **Sébastien Piquemal** for the interactive explanation of [Gamma Correction and sRGB](https://observablehq.com/@sebastien/srgb-rgb-gamma).

---

## TODO list

- [ ] Leaves
- [ ] Basic Filtering
- [ ] Dynamic Terrain Edit
- [ ] Other types of plants
- [ ] Sky Color
