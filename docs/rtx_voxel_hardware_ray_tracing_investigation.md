# Re:Flora RTX Voxel Hardware Ray Tracing Investigation

- 调研日期：2026-09-04 至 2026-09-05
- 固定主分支基线：`7ce60e06f1b70793c18339ce60a59a61c985aa82`
- 实测设备：NVIDIA GeForce RTX 3060 Ti 8 GiB
- 驱动：NVIDIA 580.159.04
- 最终决定：**当前路线不进入 production**

本文是已删除的一次性 RTX 实验分支留下的唯一长期记录。实验源码、raw artifacts、汇总脚本与 HTML 报告均未合入主分支；下面的数据在删除前经过两份独立 benchmark artifact、fail-closed 汇总和冻结 release binary 独立复跑验证。

## 结论

这次 tracer bullet 值得做，因为它排除了一个代价很高的方向；但当前的 hardware ray-tracing voxel 路线不值得继续产品化。

1. AABB procedural primitive 在正确性上可用。硬件返回的是保守 candidate，不是最终 hit；应用做精确 voxel/box 相交并只提交真实交点后，最终 false positive 可以为零。
2. AABB 路径没有稳定性能收益。macro AABB 实验的局部 traversal speedup 为 `0.848x` 至 `1.769x`；最终 per-surface-voxel AABB 实验仅在 dense volume 达到 `1.294x`，在 sparse 和 shell/cavity 上分别只有 `0.427x` 和 `0.434x`。
3. exposed-face triangle 是测试过的最高效正确方案，但最高可信收益也只有 dense volume 的 `1.309x`。它在 sparse 和 shell/cavity 上分别只有 `0.600x` 和 `0.432x`，明显慢于 software DDA。
4. 上述 static tracer bullet 已经主动舍弃 edit、refit、rebuild、streaming、publication、material shading 和 fallback。连这个有利上界都没有数量级收益，因此再承担动态 terrain 生命周期没有投资价值。

这不是“hardware RT 永远不适合 voxel”的结论，而是：**当前 tested mapping，即 global voxel AABB 或 exposed-face triangle AS 用于 closest-hit voxel traversal，不值得替换 Re:Flora 现有 renderer。**

## AABB false positive 的正确含义

Vulkan 允许实现扩大 AABB 以保证 traversal 保守性，所以硬件 broad phase 可能返回几何上并不相交的 candidate。应用必须验证 candidate，再通过 procedural intersection 报告真实 hit；candidate 不能直接作为最终命中。[Vulkan candidate determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-candidate-determination)、[Vulkan AABB confirmation](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-confirmation-aabb)

实验使用的语义链为：

```text
hardware AABB candidate
  -> 查出 candidate 对应的 voxel 或 macro
  -> 执行精确 unit-box slab test 或 macro 内 voxel DDA
  -> miss: reject，不生成 intersection
  -> hit: 向 ray query 报告精确 t
  -> traversal 继续维护 committed closest hit
  -> 最终结果与独立 CPU closest-hit oracle 对照
```

最终 static 实验每个 sample 实际观察到以下 candidate rejection：

| volume | raw candidates | rejected candidates | confirmed candidates | committed hits | committed false positives |
|---|---:|---:|---:|---:|---:|
| sparse 5% | 1,237,843 | 54,965 | 1,182,878 | 1,024,825 | 0 |
| dense 75% | 2,136,514 | 121,782 | 2,014,732 | 1,048,576 | 0 |
| shell/cavity | 143,359 | 8,906 | 134,453 | 105,747 | 0 |

因此原先担心的 false positive 确实存在于 candidate 层，但不应泄漏为 final hit。AABB 最终被否决的原因是 traversal 和 exact-confirm 成本，而不是无法实现正确结果。

## 实验一：macro AABB ray query

第一轮实验用 inline ray query 构建真实 `VK_KHR_acceleration_structure` 和 `VK_KHR_ray_query` 路径：

- 32x32x32 deterministic binary voxel volume。
- 5%、25%、75% 三种 density。
- 2x2x2、4x4x4、8x8x8 三种 macro 粒度。
- initial 与删除一个 occupied macro 后的 full rebuild 两个 phase。
- 每个 configuration 使用 65,536 条固定 ray；软件 shader DDA 和 hardware candidate 内 DDA 共享 occupancy，独立 Rust CPU DDA 作为 reference。
- 两份独立 artifact 合计 4,718,592 个 hardware ray/reference 对照。

正确性结果全部为零：false positive、false negative、wrong voxel、hit-t over tolerance、traversal exhaustion 和 committed-state disagreement。最大 hit-t 误差为 `0.00008392334`，容差为 `0.002`。

局部 traversal speedup 范围为 `0.848x` 至 `1.769x`。最佳点是 75% density、4x4x4、edit phase：

```text
software DDA 9.0538 ms -> macro AABB ray query 5.1167 ms = 1.769x
```

4x4x4 不是稳定最优：它在 5% density 上会慢于 software，最佳 macro 尺寸随 density 与 ray distribution 改变。

## 实验二：完全静态 traversal 上界

为了排除动态维护与 chunk 设计的干扰，第二轮直接测试可想到的简单静态上界：

- 64x64x64 static binary voxel volume。
- sparse 5%、dense 75%、shell/cavity 三种 topology。
- 每个 sample 固定 1,048,576 条 ray。
- 三条路径共享同一个 shader module、ray buffer、occupancy、result ABI 和 CPU reference。
- 每种 volume/path 两份独立 artifact，各 12 个 GPU timestamp sample，合计 24 个 sample。
- extraction 与 AS build 完全排除在 traversal timing 之外。

三条路径为：

1. 完整 grid software DDA。
2. 每个 surface voxel 一个 procedural AABB，candidate 经过精确 unit-box confirm。
3. 只生成 exposed faces 的 opaque indexed triangles，全部放入一个 global BLAS，再由一个 TLAS instance 引用。

triangle 路径使用 fixed-function intersection 和完整 closest-hit traversal。早期 `TerminateOnFirstHit` 探针会返回错误的最近 voxel、face 或 t，因此没有进入正式数据；该 flag 适合 any-hit/occlusion，不能用于需要稳定最近命中身份的比较。[Vulkan closest-hit determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-closest-hit-determination)

### Traversal performance

下表为两份 artifact 合并后的 pooled median 与 p95：

| volume | path | median ms | p95 ms | speedup vs software |
|---|---|---:|---:|---:|
| sparse 5% | software DDA | 69.620 | 73.831 | 1.000x |
| sparse 5% | AABB exact | 162.947 | 179.432 | 0.427x |
| sparse 5% | triangles | 116.038 | 123.199 | 0.600x |
| dense 75% | software DDA | 215.466 | 224.429 | 1.000x |
| dense 75% | AABB exact | 166.575 | 176.093 | 1.294x |
| dense 75% | triangles | 164.596 | 176.985 | **1.309x** |
| shell/cavity | software DDA | 91.557 | 100.202 | 1.000x |
| shell/cavity | AABB exact | 211.182 | 219.730 | 0.434x |
| shell/cavity | triangles | 212.123 | 235.616 | 0.432x |

Dense triangle 的两份独立 speedup 为 `1.392x` 和 `1.223x`；最终采用全部 24 个 sample 的 pooled `1.309x`，没有挑选最快单次结果。

### Correctness

三种 volume x 三条 path 共 9,437,184 个不同的 volume/path/ray case；计入每次 timed output 的重复验证后，共 226,492,416 个 result/reference comparison。

以下计数全部为零：

- false positive 与 committed false positive；
- false negative；
- wrong voxel、face 或 normal；
- primitive mapping mismatch；
- hit-t mismatch；
- traversal exhaustion；
- committed disagreement。

全矩阵最大 hit-t 误差为 `0.00039672852`，低于 `0.002` 容差。因此 NO-GO 是性能结论，不是由错误输出导致的失败。

### Geometry、build 与内存

| volume | surface voxels | exposed triangles | triangle extraction host ms | triangle BLAS GPU ms | triangle AS bytes |
|---|---:|---:|---:|---:|---:|
| sparse | 14,681 | 155,884 | 4.715 | 1.063 | 10,909,056 |
| dense | 163,101 | 610,528 | 18.254 | 3.056 | 42,710,144 |
| shell/cavity | 1,604 | 6,944 | 0.233 | 0.307 | 491,136 |

Dense 实验中，triangle scratch 为 12,918,144 bytes；同时保留 triangle 和 AABB comparator 时，static live resources 为 139,289,716 bytes，accounted build peak 为 190,529,068 bytes。它们未计入 traversal speedup，并且 comparator 同时驻留不是 production footprint。

## 4x4x4 模板枚举不可行

4x4x4 grid 有 64 个二值 cell，因此 occupancy 组合数不是 64，而是：

```text
2^(4x4x4) = 2^64 = 18,446,744,073,709,551,616
```

只按 24 个正旋转折叠仍有 768,614,338,020,786,176 个等价类；包含反射的 48 个立方体对称仍有 384,307,306,807,269,376 类。材质种类不进入 geometry key 也无法解决二值 topology 的指数爆炸。

可行选择只能是按实际 occupancy 动态生成、提取 exposed faces/greedy quads，或共享极少量 primitive 并实例化；它们都需要单独承担 build、instance count、edit publication 和内存成本。

## Production 意义

早期整帧测量中，`tracer.pass` 约占 `frame.render` 中位数的 13.96%，整个 `tracer.render` 约占 29.32%。即使假设 macro 实验最佳的 `1.769x` 可以无损移植：

| 假设替换范围 | Amdahl 整帧上界 |
|---|---:|
| 只替换 tracer.pass | 1.065x |
| 替换整个 tracer.render | 1.146x |

这只是上界，不是 production 预测。真实集成还需要 material/atlas lookup、descriptor publication、AS rebuild、同步、retirement、streaming 和 fallback。

最终 static triangle bullet 的最高可信局部收益仅为 `1.309x`。若它替换整帧的 25%，Amdahl 上界约为 `1.063x`；即使不现实地替换 100%，上限也仍然只有 `1.309x`。

因此不应继续实现 triangle/greedy full-world lifecycle、edit/refit 或第二套 terrain authority。若未来重新调查，必须先指定一个占帧足够大的 production ray consumer，并重新测量它的真实语义；最可能有信息量的是只要求遮挡结果的 any-hit shadow，而不是 closest-hit primary traversal。

## 未验证边界

- 只测试 RTX 3060 Ti / driver 580.159.04；不能外推到其他 NVIDIA 世代、AMD 或 Intel。
- 使用 compute ray query，不是 ray-tracing pipeline/SBT。
- static bullet 没有接入 production atlas、material shading、lighting 或 secondary-ray consumer。
- 没有测试 512x512x512 production world、真实跨 chunk transform 或长期连续编辑。
- throughput ray 避开了没有唯一 voxel/face 归属的精确 shared-edge/shared-corner；显式 axis、grazing 与 boundary case 已验证。
- synthetic software grid DDA 不等同于 production Contree traversal，不能把局部倍数直接当作整帧预测。

## 调查来源与留存边界

被删除的实验分支：`agent/rtx-voxel-hardware-rt`

- 最终实验 HEAD：`1f6148ed6881d36058421edbc361261787cb1af2`
- static tracer source revision：`49945c5909d8565253881b27fd33f0b4b16a8e26`
- frozen release binary SHA-256：`0f26d405ea2059d0d2df2f47f4daf49c87a025a53df1a2e0fdc0dff5cb4054ce`
- static artifact B1 SHA-256：`848a75b78345cb3c1848a2624e2de96283bc6f446c162ba7cb97f5fbc9cdc6a3`
- static artifact B2 SHA-256：`febffce55a0ff8a3f26a7d7ffb7ab5f93dfd49687474622b8e429daa97830413`
- static summary SHA-256：`80717d2721937b6d0619549599186e60e4046868044b3d059765dd5b633ff967`

这些 revision 与 hashes 只保留实验 provenance；按清理决定，主分支不保留实验实现、raw evidence 或复算工具，因此本文是唯一 retained artifact。

## 一手资料

- [Khronos Vulkan Specification: Ray Traversal](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html)
- [Khronos Vulkan Specification: Acceleration Structures](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html)
- [Khronos Vulkan Guide: Ray Tracing](https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html)
- [NVIDIA: Best Practices for Using NVIDIA RTX Ray Tracing](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)
- [NVIDIA: Effectively Integrating RTX Ray Tracing](https://developer.nvidia.com/blog/effectively-integrating-rtx-ray-tracing-real-time-rendering-engine/)
