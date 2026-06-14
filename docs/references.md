# References

Technical references used while building Verdarium.

## Vulkan

- [Descriptor Sets - Vulkan Guide](https://vkguide.dev/docs/chapter-4/descriptors/#binding-descriptors)
- [Descriptor Sets - NVIDIA Guide](https://developer.nvidia.com/vulkan-shader-resource-binding)
- [Vulkan Synchronization Explained](https://themaister.net/blog/2019/08/14/yet-another-blog-explaining-vulkan-synchronization/)

## Ray Tracing

- [Ray Tracing in Vulkan - Khronos](https://www.khronos.org/blog/ray-tracing-in-vulkan/)
- [GLSL_EXT_ray_query Shading Documentation](https://github.com/KhronosGroup/GLSL/blob/main/extensions/ext/GLSL_EXT_ray_query.txt/)
- [Ray Tracing Pipeline vs. Ray Query Performance](https://tellusim.com/rt-perf/)
- [NVIDIA RTX Best Practices](https://developer.nvidia.com/blog/rtx-best-practices/)
- [NVIDIA RTX Best Practices (Updated)](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)
- [Fast Voxel Ray Tracing using Sparse 64-trees](https://dubiousconst282.github.io/2024/10/03/voxel-ray-tracing/)
  - [GitHub project](https://github.com/dubiousconst282/VoxelRT)
  - [Reddit discussion](https://www.reddit.com/r/VoxelGameDev/comments/1fzimke/a_guide_to_fast_voxel_ray_tracing_using_sparse/)
- [Ray-AABB Intersection Algorithm](https://medium.com/@bromanz/another-view-on-the-classic-ray-aabb-intersection-algorithm-for-bvh-traversal-41125138b525)
- [BRDF and PDF for Sampling](https://computergraphics.stackexchange.com/questions/8578/how-to-set-equivalent-pdfs-for-cosine-weighted-and-uniform-sampled-hemispheres)

## Shadow Mapping

- [Microsoft: Common Techniques to Improve Shadow Depth Maps](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/common-techniques-to-improve-shadow-depth-maps)
- [Microsoft: Cascaded Shadow Maps](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/cascaded-shadow-maps)
- [MJP: A Sampling of Shadow Techniques](https://therealmjp.github.io/posts/shadow-maps/)
- [Long Forgotten Blog: Stable Cascaded Shadow Maps](http://longforgottenblog.blogspot.com/2014/12/rendering-post-stable-cascaded-shadow.html)
- [Michal Valient, GDC09 Killzone 2 rendering talk](https://www.guerrilla-games.com/media/News/Files/GDC09_Valient_Rendering_Technology_Of_Killzone_2.pdf)

## Water Simulation

- [Hu et al. 2018: A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling](https://yuanming.taichi.graphics/publication/2018-mlsmpm/)
- [Gao et al. 2018: GPU Optimization of Material Point Methods](https://cemyuksel.com/research/papers/gpu_mpm.pdf)
- [Wang et al. 2020: A Massively Parallel and Scalable Multi-GPU Material Point Method](https://yuxingqiu.github.io/publication/mpmgpu2020siggraph/paper.pdf)
- [Barsamian, Chargueraud, Ketterlin 2017: A Space and Bandwidth Efficient Multicore Algorithm for the Particle-in-Cell Method](https://chargueraud.org/research/2017/pic_chunk/PIC-chunks.pdf)
- [Schechter and Bridson 2012: Ghost SPH for Animating Water](https://www.cs.ubc.ca/~rbridson/docs/schechter-siggraph2012-ghostsph.pdf)
- [English et al. 2022: Modified dynamic boundary conditions for general-purpose SPH](https://doi.org/10.1007/s40571-021-00403-3)
- [Bender et al. 2019: Volume Maps: An Implicit Boundary Representation for SPH](https://dl.acm.org/doi/10.1145/3359566.3360077)

## Papers

- [ReSTIR GI: Path Resampling for Real-Time Path Tracing](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)

## Rendering and Color Science

- [Synchronization Examples - Khronos](https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples)
- [Command Buffer Lifecycle - Vulkan Spec](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#commandbuffers-lifecycle)
- [Gamma Correction Tutorial - Cambridge in Colour](https://www.cambridgeincolour.com/tutorials/gamma-correction.htm)
- [Interactive Gamma Correction and sRGB](https://observablehq.com/@sebastien/srgb-rgb-gamma)
