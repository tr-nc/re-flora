# Re:Flora Static RTX Triangle Tracer Bullet：正确最近命中的 traversal 上限

**NO-GO（数量级加速假设）；最高可信 speedup = 1.309×。** 在 RTX 3060 Ti 上，64³ 静态 dense volume 的 exposed-face triangle global BLAS 相对同 shader、同 rays、同 occupancy/output 的 software grid DDA，合并两份独立 artifact 后为 `215.466 ms → 164.596 ms`，即 **1.309×**；两份独立中位数分别是 **1.392×** 与 **1.223×**。sparse 与 shell/cavity 则只有 **0.600×** 和 **0.432×**，也就是 triangle 分别慢 1.67× 和 2.32×。这不是数量级收益，也不是稳定跨 density 的胜利。

这个结论只回答一次静态构建后、正确的 closest voxel/face/t traversal 上限。它不预测 production whole-frame，不包含 edit/refit/rebuild/streaming/publication/material shading，也不建议保留第二套 terrain lifecycle。

- 日期：2026-09-05（Asia/Shanghai）
- 固定基线：`7ce60e06f1b70793c18339ce60a59a61c985aa82`
- tracer binary source：`49945c5909d8565253881b27fd33f0b4b16a8e26`
- 分支：`agent/rtx-voxel-hardware-rt`
- 冻结 release binary SHA-256：`0f26d405ea2059d0d2df2f47f4daf49c87a025a53df1a2e0fdc0dff5cb4054ce`

## 1. 这个 bullet 实际构建了什么

这是明确标注为 `PROTOTYPE/TRACER BULLET`、default-off、可丢弃的一次性入口：

1. `src/rtx_static_tracer_bullet.rs::extract_exposed_faces` 从 64³ 二值 occupancy 只提取真正 exposed faces；每 face 4 vertices、6 indices、2 triangles，并用 `FaceData { voxel_index, face_index, normal_code }` 保留可验证身份，不枚举材质种类。
2. `crates/re-flora-vkn/src/rtx/acceleration_structure::build_triangle_blas_profiled` 把所有 triangles 放进 **一个 opaque global BLAS geometry**，使用 `R32G32B32_SFLOAT` vertices、`UINT32` indices 和 `PREFER_FAST_TRACE`；`build_single_instance_tlas` 创建 **一个 TLAS instance**。
3. `shader/slang/rtx_static_tracer_bullet.slang::traceTriangles` 用 inline `RayQuery` 走 fixed-function triangle intersection，并由 committed primitive index 经 `primitive >> 1` 回解 face/voxel。
4. 同 shader 的 `traceSoftware` 对完整 64³ grid 做 DDA；`traceAabb` 对每个 surface voxel 建一个 global AABB BLAS，并对每个 conservative candidate 做 unit-box slab exact-confirm。
5. `src/app/core/mod.rs` 只在 `--rtx-static-tracer-bullet PATH` 被显式请求时，在 production renderer 初始化前运行并销毁全部资源。没有 edit、refit、rebuild、streaming、fallback 或 publication 订阅。

default build 不含 `rtx-voxel-experiment` feature。`src/cli.rs::rejects_static_tracer_bullet_without_compile_time_feature` 锁定 feature-off 拒绝行为；feature-on 才让 device path 请求 `VK_KHR_acceleration_structure`、`VK_KHR_ray_query` 与 feature chain。正式日志中的 `[VKN][HARDWARE_RAY_QUERY] enabled`、非零 BLAS/TLAS GPU timestamps、`RayQuery::TraceRayInline/Proceed/CommitProceduralPrimitiveHit` 和 triangle committed hits 共同构成 runtime-authoritative evidence，而不是类型名、CPU 模拟或空 dispatch。

### 为什么最终没有使用 terminate-on-first-hit

最初探针按设想给 triangle query 使用了 `ACCEPT_FIRST_HIT_AND_END_SEARCH`。它产生了非最近 voxel/face/t，无法通过 CPU oracle；因此这些无效数字没有进入正式 artifact。Khronos 规范说明：除非设置 `TerminateOnFirstHitKHR`，实现才必须追踪全部几何中的最近 confirmed hit；设置后首个 confirmed hit 就终止 traversal。该 flag 适合 any-hit/occlusion，不足以证明 closest voxel identity。[Khronos Ray Closest Hit Determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-closest-hit-determination)、[Vulkan candidate determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-candidate-determination)

所以最终正式比较保留 `FORCE_OPAQUE` fixed-function triangles，但完整推进 query 得到正确 committed closest hit。这是对“完整 grid DDA 返回最近 voxel”的同语义比较。若只测 any-hit shadow，可以更早 terminate，但那是另一个问题；不能拿错误 identity/t 的更短路径宣称本题 speedup。

## 2. AABB false-positive：candidate 不是 final hit

Vulkan 允许实现为 traversal 精度保守而扩大 AABB，因此 broad phase 可产生几何上不相交的 candidate；应用必须在 intersection 阶段验证并只生成真实 intersection。[Vulkan candidate determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-candidate-determination)、[Vulkan AABB confirmation](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-confirmation-aabb)

本 bullet 的语义链是：

```text
hardware AABB broad-phase candidate
  -> surface_voxels[CandidatePrimitiveIndex]
  -> exact unit-box slab test
  -> miss: rejected_candidate_count++，不 generate
  -> hit: confirmed_candidate_count++
  -> 比当前 exact closest 更近：CommitProceduralPrimitiveHit，generated_candidate_count++
  -> traversal 结束：CommittedPrimitiveIndex 再做一次 exact slab
  -> committed_candidate_count++，并与 CPU closest oracle 对照
```

因此五个数有不同含义：raw candidate 是保守候选；rejected 是 exact slab 排除的 candidate false positive；confirmed 是真实 unit-box intersection；generated 是向 ray query 报告过的“当前最近”交点；committed 是最终最近 hit。`candidate = rejected + confirmed`，而 candidate 绝不直接充当 final hit。

| volume | raw candidate / sample | rejected | confirmed | generated | committed | broad-phase rejection | committed FP |
|---|---:|---:|---:|---:|---:|---:|---:|
| sparse 5% + fixture | 1,237,843 | 54,965 | 1,182,878 | 1,031,070 | 1,024,825 | 4.440% | 0 |
| dense 75% + fixture | 2,136,514 | 121,782 | 2,014,732 | 1,073,542 | 1,048,576 | 5.700% | 0 |
| shell/cavity | 143,359 | 8,906 | 134,453 | 106,102 | 105,747 | 6.212% | 0 |

结论很直接：**AABB candidate false positives 真实存在，但全部能被 exact-confirm 正确拒绝；committed false positive 是 0。** 用户原先“只拼 AABB 会有 false positives”描述的是 broad phase 的正常现象，不能推出 procedural AABB 不可用。它在本 workload 上性能仍不好，但原因是 candidate/exact-confirm 工作量，不是 final hit 语义错误。

## 3. 固定 benchmark 方法

### 3.1 Machine 与真实 Vulkan 能力

| 项 | 实测 |
|---|---|
| GPU | NVIDIA GeForce RTX 3060 Ti，8192 MiB |
| Driver | NVIDIA proprietary 580.159.04 |
| Vulkan device API | 1.4.312 |
| device UUID | `2cf8ee5131672062b2a587c546461acb` |
| driver UUID | `c7d8a05ebe165883a8186d61bf0812b6` |
| AS extension / feature | true / true |
| ray query extension / feature | true / true |
| ray-tracing pipeline extension | true（本 bullet 使用 ray query，不创建 RT pipeline/SBT） |
| CPU / kernel | i5-12600KF；Fedora kernel 7.0.12-101.fc43.x86_64 |

机器命令与 binary identity 见 [`raw/machine.txt`](evidence/rtx_static_tracer_bullet/raw/machine.txt)。每份 TOML 也独立记录 device/driver UUID、capability bits 与 timestamp period。

### 3.2 Workload

- release binary；每个 volume 固定 64³ occupancy。
- `sparse_5_percent_with_fixture`、`dense_75_percent_with_fixture`、`shell_cavity` 三种 density/topology。
- 每个 sample 固定 1024×1024 = **1,048,576 rays**；12 条前置定向 ray 覆盖 axis-parallel、grazing 两侧、world boundary、diagonal 与 cavity，其余为 deterministic camera-like rays。
- 吞吐 rays 会确定性避开无唯一 face/voxel 归属的精确 edge/corner ties；边界与 grazing 由显式的 `23.9999/24.0001` 等非共面 ray 覆盖。
- 三条路径共享同一个 shader module、ray buffer、occupancy buffer、result ABI 和 CPU reference；只有 mode 与 AS descriptor 不同。
- build 和 extraction 完全在 traversal timing 外；每 mode 先 warmup 2 次。
- 每个 volume 的 36 个 timed dispatch 按 12 项平衡序列重复三次：`S/A/T/T/A/S/T/A/S/S/A/T`。这让每条路径均处于早、中、晚位置；每份 artifact 每 mode 12 samples，两份合计 24 samples。
- CPU reference 对每次 timed output 检查 hit/miss、voxel、face/normal、hit-t；primitive metadata 另验 identity mapping。

64³ 已足以避免 dispatch overhead 主导：正式合并中位数是 69.6–212.1 ms/sample，最短 p95 也为 73.8 ms，且每 sample 有一百万 rays。扩大到 128³ 会主要增加 extraction/AS/memory，而不会改变“当前 dispatch 不是微秒级空调用”的判断，因此没有执行 128³。

## 4. 正确性结果

三 volume × 三 path 共有 **9,437,184 个不同的 volume/path/ray 对照**；按两份 artifact、每 mode 12 次 timed sample 重复检查，共 **226,492,416 个 result/reference comparisons**。

| gate | software DDA | voxel AABB exact | exposed-face triangles |
|---|---:|---:|---:|
| false positive | 0 | 0 | 0 |
| committed false positive | 0 | 0 | 0 |
| false negative | 0 | 0 | 0 |
| wrong voxel | 0 | 0 | 0 |
| wrong face / normal | 0 / 0 | 0 / 0 | 0 / 0 |
| primitive mapping mismatch | 0 | 0 | 0 |
| hit-t mismatch（容差 0.002） | 0 | 0 | 0 |
| traversal exhaustion | 0 | 0 | 0 |
| committed disagreement | 0 | 0 | 0 |

全矩阵最大绝对 hit-t 误差是 software shell 的 `0.00039672852`，小于 `0.002` 容差。triangle/AABB 最大值是 `0.00019836426`。性能结论只使用通过这些门的样本。

## 5. Traversal 性能

下表把两份 artifact 的 24 个样本合并；p95 用线性分位数。speedup 是 pooled `software median / path median`。

| volume | path | median ms | p95 ms | median ns/ray | median Mray/s | vs software |
|---|---|---:|---:|---:|---:|---:|
| sparse 5% | software DDA | 69.620 | 73.831 | 66.395 | 15.062 | 1.000× |
| sparse 5% | AABB exact | 162.947 | 179.432 | 155.398 | 6.435 | **0.427×** |
| sparse 5% | triangles | 116.038 | 123.199 | 110.662 | 9.037 | **0.600×** |
| dense 75% | software DDA | 215.466 | 224.429 | 205.484 | 4.867 | 1.000× |
| dense 75% | AABB exact | 166.575 | 176.093 | 158.859 | 6.295 | **1.294×** |
| dense 75% | triangles | 164.596 | 176.985 | 156.971 | 6.371 | **1.309×** |
| shell/cavity | software DDA | 91.557 | 100.202 | 87.315 | 11.453 | 1.000× |
| shell/cavity | AABB exact | 211.182 | 219.730 | 201.399 | 4.965 | **0.434×** |
| shell/cavity | triangles | 212.123 | 235.616 | 202.296 | 4.943 | **0.432×** |

triangle 只有 dense volume 获胜，而且独立运行的 dense speedup 有 `1.392× / 1.223×` 漂移；以全部 24 样本 pooled median 得到的 **1.309×** 是本报告采用的最高可信值。NVIDIA 的实践资料建议尽量使用 fixed-function triangles、避免过多小 AS 并按具体内容 profile；本 bullet 已把层级压到一个 BLAS geometry + 一个 TLAS instance，因此结果正好说明这些一般建议并不保证 voxel workload 获得数量级收益。[NVIDIA RTX Best Practices](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)、[NVIDIA Effectively Integrating RTX](https://developer.nvidia.com/blog/effectively-integrating-rtx-ray-tracing-real-time-rendering-engine/)

### Amdahl 边界（只作假设，不作 frame 预测）

本 bullet 没有替换 production pass，也没有测量可替换的 frame fraction。因此不能给 production whole-frame speedup。若纯粹把 1.309× 作为某段代码的局部加速并假设其占比为 `f`，Amdahl 上界是 `1 / ((1-f) + f/1.309)`：

| 假设可替换 fraction | 理论整帧上界 |
|---:|---:|
| 25% | 1.063× |
| 50% | 1.134× |
| 75% | 1.215× |
| 100% | 1.309× |

这些不是 production 预测；真实 fraction、atlas/material shading、ray consumer 和同步开销都未测。

## 6. Build 与内存（不计入 traversal）

下面是两份独立 build 的中位数。每个 volume 同时持有 triangle 与 AABB 对照资源，因此 `static live` 不是 triangle-only production footprint。

| volume | triangle extract host ms | triangle BLAS GPU ms | triangle TLAS GPU ms | AABB extract host ms | AABB BLAS GPU ms | AABB TLAS GPU ms |
|---|---:|---:|---:|---:|---:|---:|
| sparse | 4.715 | 1.063 | 0.199 | 0.017 | 0.485 | 0.198 |
| dense | 18.254 | 3.056 | 0.041 | 0.255 | 0.832 | 0.040 |
| shell/cavity | 0.233 | 0.307 | 0.037 | 0.002 | 0.265 | 0.115 |

| volume | surface voxels | exposed faces | triangles | triangle AS bytes | triangle scratch bytes | AABB AS bytes | AABB scratch bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| sparse | 14,681 | 77,942 | 155,884 | 10,909,056 | 3,299,712 | 551,168 | 1,120,384 |
| dense | 163,101 | 305,264 | 610,528 | 42,710,144 | 12,918,144 | 6,104,192 | 12,423,424 |
| shell/cavity | 1,604 | 3,472 | 6,944 | 491,136 | 148,608 | 61,952 | 124,416 |

| volume | triangle vertex/index/metadata input | AABB input/metadata | static live | build peak accounted | device-local heap peak |
|---|---:|---:|---:|---:|---:|
| sparse | 3,741,216 / 1,870,608 / 1,247,072 B | 352,344 / 58,724 B | 97,704,772 B | 108,093,388 B | 452,132,864 B |
| dense | 14,652,672 / 7,326,336 / 4,884,224 B | 3,914,424 / 652,404 B | 139,289,716 B | 190,529,068 B | 452,132,864 B |
| shell/cavity | 166,656 / 83,328 / 55,552 B | 38,496 / 6,416 B | 85,553,808 B | 86,119,664 B | 452,132,864 B |

所有 BLAS/TLAS host/GPU build time、AS bytes、scratch bytes 都逐 artifact 要求 `> 0`；任一空证据会让 summarizer 非零退出。

## 7. 可复算证据与 fail-closed 门

原始证据没有覆盖或改写上一轮 `docs/evidence/rtx_voxel_hardware_rt`：

- [`static_b1.toml`](evidence/rtx_static_tracer_bullet/raw/static_b1.toml) 与 [`static_b2.toml`](evidence/rtx_static_tracer_bullet/raw/static_b2.toml)：两份独立结构化 artifact。
- [`static_b1.console.log`](evidence/rtx_static_tracer_bullet/raw/static_b1.console.log) 与 [`static_b2.console.log`](evidence/rtx_static_tracer_bullet/raw/static_b2.console.log)：capability/build/query/complete/clean-shutdown runtime evidence。
- [`summary.json`](evidence/rtx_static_tracer_bullet/summary.json)：由原始证据确定性重算。
- [`default_off.console.log`](evidence/rtx_static_tracer_bullet/raw/default_off.console.log)：feature-off release smoke；有 clean shutdown，且没有 `HARDWARE_RAY_QUERY`、`RTX_STATIC_TRACER_BULLET` 或 `RTX_VOXEL` marker。

运行一次新的 tracer bullet：

```bash
cargo run --release --features rtx-voxel-experiment -- --hidden --mute --rtx-static-tracer-bullet target/rtx-static-tracer-bullet.toml --auto-exit 0.5
```

用报告采用的准确命令重算 summary：

```bash
python3 scripts/summarize_rtx_static_tracer_bullet.py --artifact docs/evidence/rtx_static_tracer_bullet/raw/static_b1.toml --artifact docs/evidence/rtx_static_tracer_bullet/raw/static_b2.toml --run-log docs/evidence/rtx_static_tracer_bullet/raw/static_b1.console.log --run-log docs/evidence/rtx_static_tracer_bullet/raw/static_b2.console.log --binary-sha256 0f26d405ea2059d0d2df2f47f4daf49c87a025a53df1a2e0fdc0dff5cb4054ce --source-head 49945c5909d8565253881b27fd33f0b4b16a8e26 --output docs/evidence/rtx_static_tracer_bullet/summary.json
```

mutation/self-test：

```bash
python3 scripts/tests/test_summarize_rtx_static_tracer_bullet.py
```

测试只复制 committed raw inputs 到 `target/tmp/rtx-static-summarizer-test-*` 后变异，覆盖：triangle primitive count、triangle traversal/BLAS/TLAS time、AABB raw/confirmed/committed 关系、任一 correctness/exhaustion/committed disagreement、artifact/log 数量与独立性、volume 笛卡尔积、sample 数量/顺序、runtime authority marker。原始 artifact 不被修改。

## 8. 明确结论与未验证边界

**最终 verdict：NO-GO for order-of-magnitude static traversal。** 对“正确最近命中”的同语义比较，global opaque exposed-face triangle BLAS 的最高可信收益只有 dense volume 的 **1.309×**；它对 sparse 和 shell/cavity 明显更慢。per-surface-voxel AABB exact-confirm 正确但也没有更高上限。这个结果反驳“只要静态、全局 triangle BLAS 就会有数量级 traversal 增益”的假设。

它同时给出两个正面结论：

1. fixed-function triangle identity mapping 可以用一个 global geometry / one-instance TLAS 正确闭环；材质种类无需进入几何组合。
2. AABB conservative false candidates 不是 committed false hits；exact-confirm 可做到所有 final correctness gate 为 0。

仍未验证、也不应从本报告外推的边界：

- 只有 RTX 3060 Ti / driver 580.159.04；没有 AMD/Intel/其他 NVIDIA 世代。
- 使用 compute ray query，不是 ray-tracing pipeline/SBT；pipeline 可能有不同调度特征。
- 没有 material/atlas lookup、lighting、secondary rays、coherence-specific consumer 或 production frame fraction。
- 没有 edit/refit/rebuild/compaction/streaming/publication/fallback；这是用户明确要求抛弃的范围。
- triangle 与 AABB comparator 同时驻留，device heap peak 不等于单一 production backend。
- 吞吐集合排除精确 edge/corner 的多解归属；显式 axis/grazing/boundary cases 仍通过，但没有定义“恰好沿共享边”时唯一 voxel 的跨实现 tie-break 规范。

如果未来再开新实验，唯一有信息量的方向是先指定一个 production ray consumer 和可替换 frame fraction，再测 any-hit shadow 专用的 terminate-on-first 语义；不能把本次 closest-hit 结果或错误的 any-hit identity 外推为整帧收益。

## 一手来源

- [Khronos Vulkan Specification — Ray Traversal](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html)
- [Khronos Vulkan Specification — AABB candidate confirmation](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-confirmation-aabb)
- [Khronos Vulkan Specification — Ray Closest Hit Determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-closest-hit-determination)
- [Khronos Vulkan Guide — Ray Tracing / Ray Query](https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html)
- [NVIDIA — Best Practices for Using NVIDIA RTX Ray Tracing](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)
- [NVIDIA — Effectively Integrating RTX Ray Tracing into a Real-Time Rendering Engine](https://developer.nvidia.com/blog/effectively-integrating-rtx-ray-tracing-real-time-rendering-engine/)
