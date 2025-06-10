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

### Art

[Some Islands](https://blenderartists.org/t/procedural-island-generator-illustration-using-blenders-geometry-nodes/1483314)

### Tech

#### Vulkan

[Descriptor set - Vulkan's guide](https://vkguide.dev/docs/chapter-4/descriptors/#binding-descriptors)

[Descriptor set - Nvidia's guide](https://developer.nvidia.com/vulkan-shader-resource-binding)

[Vulkan Synchronisation](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

#### Rust

https://refactoring.guru/refactoring/smells

https://rust-unofficial.github.io/patterns/patterns/structural/small-crates.html

### Inspirations

[Exploring an Infinite Voxel Forest](https://www.youtube.com/watch?v=1wufuXY3l1o)

[Ray Traced Reverb, Wind and Sound Occlusion (Path Traced Voxel Project)](https://www.youtube.com/watch?v=UHzeQZD9t2s)

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

[BRDF, PDF](https://computergraphics.stackexchange.com/questions/8578/how-to-set-equivalent-pdfs-for-cosine-weighted-and-uniform-sampled-hemispheres)

### Papers

[ReSTIR GI](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)

# Special Thanks To

[Implementation of ash with egui](https://github.com/adrien-ben/egui-ash-renderer)

[Synchronization Tutorial](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

[Official Synchronization Examples](https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples)

[Command Buffer Life Cycle](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#commandbuffers-lifecycle)

[Gamma Correction](https://www.cambridgeincolour.com/tutorials/gamma-correction.htm)

[Gamma Correction, SRGB color space](https://observablehq.com/@sebastien/srgb-rgb-gamma)

---

Grass Design Decisions

### **Project Design Document: Dynamic Voxel Grass Rendering**

#### **1. Project Vision & Core Architecture**

The project is a real-time ray tracer built using the **Vulkan API**. Its primary goal is to render a fully dynamic world with a distinct, uniform, **cubic voxel art style**.

The core rendering architecture is built upon the modern Vulkan ray tracing extensions (`VK_KHR_acceleration_structure`, `VK_KHR_ray_tracing_pipeline`, etc.). The world is composed of two main types of geometry:

- **Static Terrain:** The vast, non-changing voxel landscape. To manage this sparse and complex data efficiently for ray tracing, the terrain is built into a specialized acceleration structure known as a **Contree**. A Contree is a pointer-based tree structure, similar to an Octree but often more efficient for ray traversal in sparse voxel scenes, as it can have a variable number of children per node, leading to a more compact and faster-to-traverse structure. The Contree is built once and treated as a highly optimized, static piece of the world.
- **Dynamic Objects:** Elements like grass, effects, and characters that need to be updated every frame. It is computationally prohibitive to rebuild the entire terrain's Contree structure just to update these smaller, dynamic elements.

#### **2. The Aesthetic Constraint: The "Golden Rule"**

There is one critical, non-negotiable rule that dictates all design decisions for new geometry:

> **All rendered geometry must be composed of axis-aligned cubes that are snapped to a global voxel grid.**

This rule ensures a cohesive and intentional art style. Its primary technical implication is that **standard instance transformations like rotation and shear are forbidden for animation**, as they would deform the cubes and misalign them from the global grid.

#### **3. The Challenge: Rendering Performant, Dynamic Grass**

The immediate goal is to add large fields of grass that adhere to the Golden Rule and meet the following criteria:

- **Visually Consistent:** The grass must be made of voxels.
- **Dynamic:** It must animate (e.g., react to wind).
- **Granular & Natural:** It should not be rendered in repeating, grid-like patterns. Borders between grass and other terrain types must look natural.
- **Performant:** It must be rendered efficiently without rebuilding the static terrain's Contree.

#### **4. The Design Journey: Exploring Solutions**

We analyzed several architectural approaches to solve this problem.

##### **Approach A: The Initial User Implementation (Chunk-Based BLAS)**

- **Concept:** A compute shader generates the geometry for an entire 8x8 chunk of grass blades. This chunk is built into a single Bottom-Level Acceleration Structure (BLAS). The Top-Level Acceleration Structure (TLAS) then places instances of this chunk BLAS around the world.
- **Analysis:**
  - **Pros:** Efficiently reuses one BLAS; keeps the TLAS instance count low.
  - **Cons (Fatal Flaws):** Creates highly visible, repetitive grid patterns. Offers no control over individual blades, resulting in unnatural, blocky borders and no per-blade variation. This was deemed insufficient to meet the visual quality goals.

##### **Approach B: Shearing Instance Transforms (The Standard Method - Rejected)**

- **Concept:** Create one BLAS for a single, straight blade of grass. In the TLAS, create thousands of instances of this BLAS. Apply a shear transformation to each instance's matrix to simulate bending from wind.
- **Analysis:**
  - **Pros:** Extremely fast and efficient for animating large quantities of simple objects.
  - **Cons (Fatal Flaw):** **Violates the Golden Rule.** The shear transform would deform the cubes within the BLAS, breaking the axis-aligned, grid-snapped aesthetic. This approach was immediately rejected.

##### **Approach C: The Dynamic "Mega-BLAS"**

- **Concept:** Every frame, a compute shader generates the complete, final geometry for _every visible blade of grass_ into a single, massive vertex/index buffer. A new "Mega-BLAS" is built from this buffer every frame. The TLAS then only needs two instances: one for the static terrain and one for this all-encompassing grass BLAS.
- **Analysis:**
  - **Pros:** Offers maximum flexibility for animation and placement. Perfectly enforces the Golden Rule by calculating snapped voxel positions in the shader.
  - **Cons:** Incurs the performance cost of building a large, new BLAS every single frame.

##### **Approach D: BLAS Caching (The Chosen Method)**

- **Concept:** Acknowledges that animation must come from changing geometry, but avoids the per-frame build cost of Approach C. It involves pre-generating a library of different grass blade shapes and selecting from them at runtime.
- **Analysis:**
  - **Pros:** Extremely fast runtime performance as no new geometry or BLASes are built in the main loop. Perfectly enforces the Golden Rule.
  - **Cons:** Animation is "jumpy" as it snaps between pre-defined poses. Consumes more VRAM to store the pool of BLASes. Animation variety is limited to the number of pre-built poses.

#### **5. Final Design Decision & Implementation Plan**

Based on your preference for maximum runtime performance and simplicity over perfectly smooth animation, we have selected **Approach D: The BLAS Caching Approach**.

This strategy provides the best balance of performance and visual fidelity _within the project's unique constraints_.

**The Action Plan is as follows:**

1.  **Phase 1: Offline BLAS Library Generation (One-Time Setup)**

    - A setup routine will run a compute shader multiple times.
    - This shader will generate the geometry for a variety of grass blades (e.g., `blade_height5_bendX1`, `blade_height6_bendZ-2`, etc.).
    - Inside the shader, the final position of each cube will be passed through a `round()` function to snap it to the global voxel grid.
    - Each unique blade shape will be used to build a separate BLAS. The handles to these BLASes will be stored in a "pool" or "library" on the GPU.

2.  **Phase 2: Real-Time Frame Logic**
    - **A) Instance Generation (Compute Shader):** A compute shader will run every frame to determine grass placement. For each potential blade, it will:
      1.  Use noise and terrain data (material, normal) to decide if a blade should exist there, providing natural placement and borders.
      2.  Perform camera frustum and distance culling.
      3.  Simulate wind using a noise function based on world position and time.
      4.  Based on the wind simulation, **select the ID of the most appropriate BLAS** from the pre-built library.
      5.  Output an instance structure containing the blade's world position and the selected `blasId`.
    - **B) TLAS Construction:** The main TLAS will be rebuilt every frame. It will contain one instance for the static Contree/terrain, and a dynamic list of instances generated by the compute shader, where each grass instance can point to a different BLAS from the library.
    - **C) Ray Tracing:** The ray tracing shaders will execute, tracing against this final TLAS to render the scene.
