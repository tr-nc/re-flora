# Slang hot-path optimization assessment

## Scope

This note evaluates whether Slang-specific language features can materially improve two current GPU hot paths:

- main tracer traversal, especially `shader/slang/tracer.slang`, `scene_marching.slang`, and `contree_marching.slang`;
- surface extraction and normal calculation, especially `surface_extraction.slang` and `make_surface_sparse.slang`.

It records optimization opportunities only. It does not propose changing rendering behavior or adopting an optimization without a matched release-mode benchmark.

## Executive conclusion

Slang can make aggressive static specialization and target-specific algorithms easier to express and maintain, but changing syntax alone is unlikely to produce a large speedup. The highest-value Slang features for these paths are compile-time generics, link-time specialization, static interfaces, conditional fields, and carefully selected loop unrolling.

Raw pointers are not the primary opportunity. The current workloads are dominated by divergent traversal, random node loads, repeated shared-memory neighborhood reads, and global atomic contention. Pointer arithmetic does not remove those costs and may increase address width, register pressure, and node bandwidth.

The recommended priority is:

1. aggregate surface counters and brick flags within a workgroup before global atomics;
2. reduce the cost of the 5x5x5 surface-normal kernel;
3. compile dedicated normal-gameplay and debug/indirect tracer variants;
4. specialize traversal result payloads for primary, depth-only, any-hit, and collider uses;
5. verify whether the per-ray contree stack needs explicit clearing;
6. selectively unroll small fixed loops;
7. tune workgroup sizes per backend;
8. consider Vulkan buffer-device-address pointers only as an isolated late experiment.

## Existing evidence

The current migration evidence does not show an automatic native-Slang speedup:

- On an RTX 3060 Ti, the native Slang main tracer measured a 473 us median versus 467 us for shaderc GLSL, a repeatable 1.3% pass difference. The broader tracer-render median differed by 0.5%.
- Heavy sparse surface extraction has measured at practical parity between the frontends; the aggregate validation recorded 600/601 us default/native medians.
- Inspection of optimized SPIR-V for the current native tracer and sparse surface entry points found no retained `OpFunctionCall`; helper modules and functions had already been inlined into `main`.

These results mean that modules, member functions, `out`/`inout`, or replacing function parameters with pointers should not be treated as performance optimizations by themselves.

## Pointer assessment

### What a Slang pointer changes

For SPIR-V, a source-level global-memory pointer is lowered to the `PhysicalStorageBuffer` storage class and switches the module to the `PhysicalStorageBuffer64` addressing model. This normally requires Vulkan buffer device address support and introduces 64-bit addresses into the shader.

Slang currently documents source-level pointers for SPIR-V, C++, and CUDA targets. They are therefore not a reliable common-source primitive for the planned Vulkan and Metal backends. Slang-generated MSL already lowers resources such as `StructuredBuffer<T>` to Metal `device T*`, so retaining a structured-buffer source interface does not prevent the Metal compiler from using native pointer-based resource access.

### Why pointers are unlikely to help contree traversal

The current hot load is conceptually:

```text
node = nodeData[nodeOffset + nodeIndex]
```

Replacing the index with a pointer still requires address calculation followed by the same divergent, often random memory load. The expensive parts are cache behavior and divergence between rays, not the 32-bit index addition.

The current contree representation also uses compact relative child indices. Replacing a 32-bit relative offset with a 64-bit child pointer would enlarge nodes or reduce packing efficiency, increasing the bandwidth of every traversal step. That trade is unfavorable unless a benchmark demonstrates a large benefit elsewhere.

Pointers may still be useful for a future heterogeneous graph, a unified device-address arena, or descriptor reduction. Those are data-structure experiments, not a free optimization of the current contiguous buffers. Any such prototype should keep the indexed implementation as the portable Metal path.

### Local values and function parameters

Slang does not support taking pointers to ordinary local variables. Its `out` and `inout` parameters already provide reference-like semantics where aliasing does not interfere, and the optimized shaders currently inline their calls and scalarize values. A pointer to `MarchingResult` would therefore not avoid an observed call or copy cost.

## Tracer opportunities

### Compile specialized tracer variants

The main tracer currently retains optional work behind runtime-uniform conditions, including the indirect/debug path. A uniform branch avoids intra-wave divergence when disabled, but the optional code can still increase static instruction size and register allocation for the entire shader.

Slang generic values or link-time constants can produce separate variants such as:

```text
Tracer<EnableIndirect = false, EnablePreview = false>
Tracer<EnableIndirect = true,  EnablePreview = true>
```

The normal-gameplay variant could completely remove indirect traversal, debug-only results, and inactive preview calculations. This is more promising than converting buffer indexing to pointers because lower register pressure can improve occupancy even when the existing runtime branch is normally skipped.

Variant count must remain bounded. Features that change rarely and remove substantial code are good candidates; continuously tuned numeric parameters should remain runtime data.

### Specialize traversal result payloads

`MarchingResult` carries iteration count, hit state, two positions, distance, normal, voxel type, hash, and voxel address. Not every caller needs every field:

- primary rays need the full shading payload;
- depth-only rays mainly need hit position or depth;
- any-hit rays may need only a boolean;
- collider rays need a different subset.

A generic trace policy can expose compile-time requirements such as `needsNormal`, `needsMaterial`, and `needsCenter`. Slang's `Conditional<T, condition>`, generic value parameters, and link-time types can then remove unused fields and the work that computes them.

The implementation should use concrete generic/static-interface types. Runtime-unknown interface values can require dynamic dispatch and should not be introduced into traversal hot loops.

Optimizers may already eliminate some unused work after inlining, especially for a separate depth entry point. Explicit payload specialization is still useful when it shortens live ranges, lowers register pressure, and makes the intended variants testable instead of depending on whole-program dead-code elimination.

### Audit contree stack initialization

`marchLocalContree` clears all 11 group-shared stack entries for every ray before traversal. The stack is written while descending and read while ascending. If every reachable read is provably preceded by a write at that level, the clear loop is redundant shared-memory traffic.

This invariant must be proven and validated before removing the initialization. A compile-time `InitializeStack` experiment could provide a controlled A/B comparison, but the optimization itself is algorithmic rather than pointer-specific.

### Selective unrolling

The optimized tracer still contains fixed and variable traversal/filter loops. Slang's `[ForceUnroll]` should be tested only on small fixed loops where constant indices enable simpler code. Candidates include six-neighbor tests, 3x3 filters, or short fixed tap loops.

The 1,024-step contree loop and 256-step scene DDA loop must remain dynamic. Fully expanding larger sampling loops can increase instruction-cache and register pressure, and may behave differently on Vulkan and Metal.

`[ForceInline]` has low priority because current optimized SPIR-V already retains only the entry function.

### Lower-priority traversal ideas

Wave-cooperative node loading or packet traversal could exploit coherence between primary rays, but divergent rays make the benefit workload-dependent. Slang's current Metal wave-intrinsic coverage is also incomplete. These approaches should follow simpler specialization and data-layout experiments.

Half precision is inappropriate for contree position bit manipulations and traversal coordinates. It may be useful for selected material or filter intermediates, but requires an isolated quality and performance gate.

## Surface extraction and normal opportunities

### Current normal cost

`calculateNormal` scans a 5x5x5 neighborhood for each surviving surface voxel. That means up to 125 group-shared reads and solidity tests per voxel, with heavily overlapping neighborhoods between adjacent workgroup invocations.

This repeated neighborhood work is a more important target than pointer syntax.

### Low-risk arithmetic changes

Offsets and occupancy are integral. The normal sum can be accumulated as `int3` and converted to `float3` once before normalization instead of converting and adding floating-point vectors for every solid neighbor. A branchless integer select is also worth comparing with the current per-sample branch.

These transformations are available in GLSL too; Slang's benefit is the ability to package and specialize them cleanly.

### Selective normal-loop unrolling

The fixed six-neighbor occlusion loop is a good first unroll candidate. Unrolling only the inner five-iteration axis of the normal kernel may also expose constant offsets without multiplying the entire body 125 times.

Fully unrolling all three loops should not be assumed beneficial. It may reduce loop overhead but substantially increase code size and register pressure. SPIR-V size, vendor ISA statistics, and release-mode GPU timing must all be checked.

### Algorithmic normal variants

The three normal components are weighted occupancy sums over a fixed neighborhood. Potential alternatives include:

- staged separable or sliding-window sums in group-shared memory;
- bit-packed occupancy rows combined with masks and `countbits`;
- precomputed row or plane sums reused by neighboring invocations;
- fast 6/18/26-neighbor estimators and the current smooth 5x5x5 estimator as quality variants.

Slang generics and static interfaces can express `NormalEstimator<Fast>` and `NormalEstimator<Smooth>` without duplicating surface orchestration. Link-time specialization can select a concrete implementation per quality tier or backend without dynamic dispatch.

### Aggregate global atomics

Each extracted surface voxel currently increments the global active-voxel count and atomically ORs its brick bit. The returned old active-voxel index is not used. Multiple voxels in one 4x4x4 brick repeatedly update the same global flag, while an 8x8x8 workgroup covers only eight such bricks.

A portable workgroup algorithm can:

1. retain an active predicate for every invocation;
2. reduce the active count in group-shared memory;
3. perform one global count increment per workgroup;
4. aggregate the eight local brick states;
5. perform at most one global flag operation per active brick.

This can reduce global atomic traffic by far more than changing frontend syntax. Care is required to keep every invocation alive through required barriers instead of returning early.

A Vulkan-specific wave implementation could reduce shared-memory synchronization further, but the common Vulkan/Metal implementation should use workgroup primitives until Slang's Metal wave support is sufficient. A static aggregation-policy interface can keep both implementations in one source architecture.

## Backend-specific specialization

Slang's strongest performance advantage is the ability to share an algorithmic interface while compiling concrete backend policies:

```text
Vulkan:
  SPIR-V, optional wave aggregation, optional BDA experiment

Metal:
  MSL, native device-buffer lowering, portable group-shared aggregation
```

Workgroup dimensions should also be specializable. The current 8x8x8 surface group balances halo reuse against 512-thread occupancy, but the best point may differ between NVIDIA, AMD, Intel, and Apple GPUs. Generic value parameters can generate a small curated set of group-size variants without preprocessor duplication.

Target-specific specialization must not fragment correctness rules. Traversal semantics, packed data layouts, normal quality definitions, and output contracts should remain shared sources of truth.

## Slang feature priority

| Feature | Expected value for these paths | Notes |
| --- | --- | --- |
| Generic/link-time specialization | High | Removes optional tracer code and creates bounded quality/backend variants |
| Static interfaces | High | Encodes traversal, normal, and aggregation policies without runtime dispatch |
| Conditional fields | Medium to high | Can reduce result payload and register live ranges |
| Selective `[ForceUnroll]` | Medium | Best for short fixed loops; must watch code size and registers |
| Wave intrinsics | Potentially high for atomics | Vulkan-first until Metal support is adequate |
| Raw pointers | Low | Physical 64-bit addressing does not remove divergent memory latency |
| `[ForceInline]` | Low | Current optimized entries already inline all helpers |
| `half`/small integer types | Targeted | Avoid in traversal coordinates; test in material/filter intermediates |
| Automatic differentiation | None | Not relevant to these kernels |
| Parameter blocks/reflection | Architectural | Improves portability and binding ownership, not inner-loop execution |

## Recommended experiment order

Every experiment should preserve a selectable reference and use matched, order-reversed release runs on representative Vulkan and Metal hardware.

1. Add measurement that separately attributes sparse extraction's normal work and global atomics if practical.
2. Aggregate `active_voxel_len` and active-brick updates per workgroup.
3. Test integer normal accumulation and selective six-neighbor/inner-loop unrolling.
4. Prototype one alternative normal estimator while preserving the current quality mode.
5. Build a normal-gameplay tracer variant with indirect/debug and inactive preview code statically removed.
6. Inspect vendor ISA register counts and occupancy, not only SPIR-V instruction count.
7. Introduce result-payload policies where they demonstrably remove loads or registers.
8. Prove and benchmark removal of contree stack initialization.
9. Tune a small set of workgroup sizes per backend.
10. Only then test a Vulkan BDA pointer representation against the compact indexed representation.

Required evidence includes output hashes or image comparisons, Vulkan validation, MSL compilation, GPU pass medians and tails, generated-code size, register usage where available, and end-to-end frame timing. Debug builds and source-level instruction intuition are not performance evidence.

## References

- Existing project evidence: [`slang-validation-plan.md`](slang-validation-plan.md) and the completed
  migration record at [`003e535d`](https://github.com/tr-nc/re-flora/blob/003e535dc26bf877c6dd5c3e643b4c2d5549a9aa/docs/slang-poc.md)
- Slang pointers: <https://docs.shader-slang.org/en/latest/external/slang/docs/user-guide/03-convenience-features.html#pointers-limited>
- SPIR-V global pointers: <https://docs.shader-slang.org/en/latest/external/slang/docs/user-guide/a2-01-spirv-target-specific.html#global-memory-pointers>
- Slang link-time specialization: <https://shader-slang.org/slang/user-guide/link-time-specialization>
- Slang interfaces and generics: <https://shader-slang.org/slang/user-guide/interfaces-generics>
- Slang Metal target behavior: <https://docs.shader-slang.org/en/latest/external/slang/docs/user-guide/a2-02-metal-target-specific.html>
