# Re:Flora 可编辑 Voxel 的 NVIDIA RTX Hardware Ray Tracing 调研与最小实验

- 日期：2026-09-04
- 固定基线：`7ce60e06f1b70793c18339ce60a59a61c985aa82`
- 实验分支：`agent/rtx-voxel-hardware-rt`

## 结论先行

**本轮结论是 production no-go，但 AABB procedural primitive 本身不是 no-go。**

- “AABB broad phase 会产生 false positive，所以 AABB procedural primitive 不可用”这个前提不成立。Vulkan 明确把 AABB 结果定义为候选，允许实现扩大 bounds，并要求应用在报告 hit 前验证候选；候选不等于最终 hit。[Vulkan Ray Traversal：candidate 与 AABB conservative bounds](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-candidate-determination)、[AABB confirmation](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-confirmation-aabb)
- 最终实验在真实 `VK_KHR_acceleration_structure` + `VK_KHR_ray_query` 路径上，对每个 AABB candidate 做宏块内精确 voxel DDA，只把当前最近的真实交点提交给 ray query。2³、4³、8³，5%/25%/75% density，编辑前后，两次独立运行合计 **4,718,592 个 hardware ray/reference 对照**：false positive、false negative、wrong voxel、超容差 hit-t、traversal exhaustion、committed-state disagreement 均为 **0**。最大 hit-t 误差 `0.00008392334`，容差 `0.002`。
- 但性能没有数量级价值。合格的局部 GPU traversal 中位数加速范围是 **0.848×–1.769×**；稀疏场景会退化，最佳点是 75% density、4³、编辑后 `9.0538 ms → 5.1167 ms = 1.769×`。因此不能把 4³ 当通用答案。
- 固定 1600×1000、默认相机、release A/B/B/A 的整帧中位数是 `1.6335 ms → 1.6265 ms = 1.004×`。B 在正常帧前已经销毁一次性实验资源，所以该数字只验证 **default-off 与资源边界没有留下可见开销**，不是“production tracer 已被 RTX 替换”的加速结果。
- 当前 `tracer.pass` 只占默认 `frame.render` 中位数的 13.96%。即使把合成实验的最佳 1.769×原样移植到该 pass，Amdahl 上界也只有 **1.065×**；即使整个 `tracer.render`（29.32%）都得到同样加速，也只有 **1.146×**。即便局部无限快，对应上界也只是 1.162× / 1.415×。
- 4×4×4 二值 occupancy 的原始组合数是 **2^64 = 18,446,744,073,709,551,616**，不是 64。只按 24 个正旋转折叠仍有 `768,614,338,020,786,176` 类；加反射的全部 48 个立方体对称仍有 `384,307,306,807,269,376` 类。全量预生成不可行；只能按需生成/缓存、只存表面、或换表示。

所以本轮不把实验扩成永久的第二套 terrain lifecycle，也不实现 triangle production path。保留 default-off 的能力门与可复现实验；下一步只有在选定一个占整帧足够大的具体 ray consumer，并用 production atlas/terrain revision 做垂直切片时才值得继续。

## 1. 审计范围与基线事实

### 1.1 Production terrain 不是三角网格

仓库自己的决策文档 `docs/terrain_visual_rebuild_pipeline.md` 明确写出当前可见 terrain 链路：

```text
chunk_atlas
  -> SurfaceBuilder
  -> ContreeBuilder: contree_node_data + contree_leaf_data
  -> SceneAccelBuilder: scene_tex[chunk] = node/leaf offsets
  -> tracer.slang: marchScene(...)
```

具体所有权与消费点：

- `src/builder/plain/resources.rs::PlainBuilderResources::chunk_atlas` 创建 `R8_UINT` 3D atlas；`src/builder/plain/mod.rs::read_chunk_atlas_region`、`write_chunk_atlas_region` 和 terrain edit shader 读写它。
- `src/app/core/mod.rs::VOXEL_DIM_PER_CHUNK = 256³`、`CHUNK_DIM = 2³`，因此当前完整世界 atlas 是 512³ voxel。
- `src/builder/plain/resources.rs::PlainBuilderResources::new` 还按 8³ voxel workgroup 维护 `solid_workgroup_flags`；这与本实验要比较的 AS 宏块尺寸不是同一层所有权。
- `src/builder/contree/resources.rs::ContreeBuilderResources` 拥有 `contree_node_data` 与 `contree_leaf_data`。`src/builder/contree/mod.rs::max_node_buffer_size_in_bytes` 显示 Contree 每级按 4³ child 聚合；这同样不证明 4³ 是 RT BLAS 的最优 primitive 粒度。
- 当前 release 日志实际报告 Contree pool 为 node `27.43 MiB`、leaf `90.00 MiB`，不是旧文档里的两个 512 MiB。来源是 `src/builder/contree/mod.rs::pool_sizes_for_chunk_dim` 与 `src/app/core/mod.rs` 的启动日志；原始证据见 `raw/frame_b1.log`。
- `shader/slang/scene_marching.slang::marchScene` 先遍历 `scene_tex` 的 chunk，再进入 Contree；`shader/slang/tracer.slang::generalSceneMarching` 是 production tracer 入口。
- 材质/状态的权威仍是 atlas。`shader/slang/tracer.slang::terrainAtlasData` 从 hit center 回查 binding 16 的 `chunk_atlas`，`parseTraceResult` 再应用材质、湿度与肥力；硬件 AS 不应复制成第二份材质权威。

### 1.2 Terrain edit、publication 与 streaming 边界

- `src/app/core/mod.rs::execute_world_edit` 先让 `WorldEditTransaction` 改 atlas，再构造 `VisibleTerrainChange`。
- `src/app/core/visible_terrain.rs::VisibleTerrainPublication` 拥有一次完整的可见 revision；其 `publish_edit_observers` 在 physical publish 后才通知 emissive、shadow、collider、DDGI 并提交 revision。
- `src/app/physical_visible_terrain.rs::PhysicalTerrainPublication` 把 `Surface -> Contree -> scene_tex` 顺序和 pending GPU job 封装在一个 deep module 中；`publish_scene_records` 只在全部受影响 chunk 的 Contree 完成后发布 scene entry。
- loading 与 edit 使用同一 physical implementation，但 loading 可逐帧推进；普通 edit 由 `run_to_completion` 同步完成。任何 production RT AS 必须加入这一 publication 事务，不能监听 atlas 另起一条最终一致生命周期。

建议的长期所有权边界是一个不可变 `TerrainAccelerationPublication { terrain_revision, chunk_records, tlas, atlas_view }`。内部独占 staging BLAS、scratch、barrier、descriptor rewrite 与 fence retirement；外部只观察完整 revision。这个接口把 AS 复杂度藏在 publication 内，且可在删除 RTX backend 后不影响 atlas/Contree 权威，符合 deep-module 的 locality/deletion test。

### 1.3 基线里的 `rtx` 源码不是 hardware RT 证据

固定基线已有 `crates/re-flora-vkn/src/rtx/acceleration_structure`：

- `build_or_update_blas` 只构造 triangle geometry；`build_tlas` 能构造 TLAS；`AccelStruct` 管 backing buffer 与 handle。
- 对基线做调用点搜索，production 没有调用这些 builder。
- 基线 `crates/re-flora-vkn/src/context/device.rs::device_extension_requirements` 只额外无条件请求 `VK_KHR_deferred_host_operations`，没有请求 `VK_KHR_acceleration_structure` / `VK_KHR_ray_query` / `VK_KHR_ray_tracing_pipeline`，也没有把相应 feature struct 接到 `vkCreateDevice`。
- 基线 `crates/re-flora-vkn/src/descriptor/descriptor_pool.rs::DescriptorPool::new` 没有 `ACCELERATION_STRUCTURE_KHR` pool size。generic descriptor 代码能识别该枚举，也不能证明可以分配和 dispatch。

Khronos 把 acceleration structure、ray query、ray-tracing pipeline 与 deferred host operations列为相互关联但需要显式启用的扩展；ray query 仍必须由 shader 执行 traversal 指令。[Khronos Vulkan Guide：Ray Tracing extensions](https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html)、[Vulkan Ray Traversal](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html)

因此，基线结论是 **dead/helper code present，hardware RT not enabled**。

## 2. 两个必须纠正的前提

### 2.1 4³ occupancy 不是 64 种

4×4×4 有 64 个二值 cell，每个 cell 独立空/实：

```text
N = 2^(4×4×4) = 2^64
  = 18,446,744,073,709,551,616
```

对立方体 24 个保向旋转做 Burnside 计数，置换 cycle 数分布为：

```text
1 × 64 cycles
9 × 32 cycles
8 × 24 cycles
6 × 16 cycles
```

所以旋转等价类数为：

```text
(2^64 + 9×2^32 + 8×2^24 + 6×2^16) / 24
= 768,614,338,020,786,176
```

若允许反射，48 个对称的 cycle 分布为 `1×64 + 6×40 + 13×32 + 8×24 + 12×16 + 8×12`，仍有：

```text
384,307,306,807,269,376
```

材质不止二值时组合数还会继续增长。因此可利用的不是“全表”：

1. 只按需为实际出现的 occupancy 生成，并以 canonical rotation key 做有界 LRU；收益取决于重复率，必须记录 cache hit rate。
2. 只生成 exposed faces/greedy quads，不把内部 occupancy 当模板身份。
3. 用共享 unit-cube/box BLAS 实例化，换取 TLAS instance 数与更新成本。
4. 保持 Contree/atlas 作为表示，只给选定 ray consumer 增加 hardware broad phase。

### 2.2 AABB false positive 是 candidate，不是 hit

Vulkan 规范明确允许实现为精度保守而扩大 AABB；这会把 false-positive **candidate** 返回应用。规范随后明确要求应用验证 AABB candidate，只报告落在所需几何范围内的交点。对 ray query，`Proceed` 返回 candidate，应用通过 generated intersection 报告真实命中；traversal 继续维护 committed closest hit。[Vulkan candidate determination](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-candidate-determination)、[Vulkan AABB confirmation / closest hit](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html#ray-traversal-intersection-confirmation-aabb)

本实验中的对应关系是：

```text
RTX traversal: macro AABB candidate
  -> shader/slang/rtx_voxel_benchmark.slang::traceVoxelRange
  -> candidate 宏块内 exact voxel DDA + occupancy lookup
  -> miss: reject，不 commit
  -> hit 且比当前 exact hit 更近: CommitProceduralPrimitiveHit(interior t)
  -> traversal 结束后从 CommittedPrimitiveIndex 回解并再次验证
  -> 与独立 CPU DDA reference 比较
```

最终数据的 candidate rejection 比例为 **2.1%–80.1%**，说明 broad-phase false candidates 确实很多；但最终 false positive 是 **0**。结论不是“AABB 没有 false positive”，而是“false candidate 可正确过滤，不能据此判 AABB 不可用”。

开发过程中还暴露了两个必要的正确性约束：candidate entry 不得 clamp 回宏块内部；generated hit 要放在真实 occupied voxel 的非零 interior interval，并且只在优于当前 exact closest 时 commit。最终 shader 的 committed primitive、committed t 与 CPU reference 都纳入门槛，而非只比较手工记录的候选。

## 3. 四种实质不同的架构

| 方案 | 几何/层级 | edit 与 build | 材质与 descriptor | 优势 | 主要风险 | 判断 |
|---|---|---|---|---|---|---|
| A. Procedural AABB | 每个可见 chunk 一个 BLAS；每个 occupied macro 一个 AABB；TLAS 每 chunk 一个 instance；candidate 内 exact voxel DDA | macro 由空变实/实变空会改变 primitive count，必须重建 dirty BLAS；受影响 chunk 全部 staging 后更新/重建 TLAS并原子发布 | primitive metadata 只存 macro origin；真实 voxel/material 回查 authoritative atlas；compute ray-query descriptor 复用 atlas | 无 surface mesh；能直接表达 cavity；实验最小、最有信息量 | 稀疏场景 candidate rejection 很高；custom intersection/DDA 成本；动态 topology 不能普通 refit | 本轮原型；语义可行，性能不足以 production go |
| B. Triangle surface/greedy mesh | dirty chunk/page 生成 exposed voxel faces 或 greedy quads；每 chunk BLAS；TLAS chunk instance | face topology/primitive count 常变，通常 full BLAS rebuild；只移动 vertex 且 topology 不变才可 update | primitive→face/voxel metadata，或用 hit position 回查 atlas；需处理边界取样与材质 face | fixed-function triangle intersection；NVIDIA 一般建议优先 triangles | meshing、temporary/index/vertex memory、edit latency；greedy merge 会增加材质/状态切分；另一套 surface 表示 | 只有 AABB 垂直切片显示消费者级价值后再对照，不应先造全世界 mesh |
| C. 共享模板/实例化 | 一个 unit-cube BLAS 或少量 box template；每 occupied voxel/box 一个 TLAS instance | BLAS 静态；edit 主要改 instance list/TLAS；若 instance count 变化仍 rebuild | instance custom index 定位 world voxel，再查 atlas | 避免 2^64 模板表；最大化 BLAS 复用 | 满世界最坏 512³ = 134,217,728 voxel instance；本机 `maxInstanceCount=16,777,215`，还未算 TLAS memory/traversal overlap | 只适合稀疏对象层或 greedy boxes，不适合完整 terrain |
| D. 混合 publication | Contree/atlas 继续服务 primary、fallback 与 CPU；只把占比足够大的 shadows/GI/query consumer 接到按 chunk 的 RT snapshot | `VisibleTerrainPublication` 内 staging dirty chunks；全部 ready 后一次发布 revision；旧 AS 等 fence 退休 | atlas 是唯一材质权威；RTX descriptor/sync 由 backend 私有；unsupported GPU 自动走 Contree | 删除 test 好；跨硬件；不强迫 primary renderer 改写 | 同 revision 双表示需要严格事务；如果 consumer 占帧太小仍受 Amdahl 限制 | 若继续，推荐的 production 所有权形状 |

NVIDIA 的一般实践是优先 triangle fixed-function intersection、实例化共享 BLAS、合并适当几何、避免过多小 AS，并对具体内容做 profile；这支持把 triangle 当后续对照，而不是证明 voxel terrain 必然应转成 triangles。[NVIDIA：Effectively Integrating RTX](https://developer.nvidia.com/blog/effectively-integrating-rtx-ray-tracing-real-time-rendering-engine/)、[NVIDIA RTX Best Practices](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)

### AS update、compaction 与同步约束

- Vulkan `UPDATE` 只允许改 instance 定义、transform、vertex/AABB position；不能改变 geometry/instance/primitive 数，也不能切换 active/inactive。因此本实验清空一个 occupied macro 后 primitive 数减一，明确执行 `BLAS BUILD + TLAS BUILD`，没有把 rebuild 冒充 refit。[Vulkan Acceleration Structure Update Rules](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html#acceleration-structure-update)
- 固定槽位并把不用的 AABB 退化为点，规范上可允许 degenerate 状态切换，但 degenerate AABB 仍可能调用 intersection shader；它把 topology 问题换成常驻 primitive/candidate 成本，必须另测。[Vulkan Inactive and Degenerate Primitives](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html#acceleration-structure-inactive-primitives)
- build input/scratch 可在 build 完成且正确同步后复用；BLAS 必须在引用它的 TLAS 使用期间有效，scratch access 需使用 acceleration-structure build stage/access 同步。[Vulkan Building Acceleration Structures](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html#acceleration-structure-building)
- compaction 要额外查询 size、copy、同步并延后释放 source；NVIDIA 认为它更适合不常 rebuild/update 的 BLAS，不适合频繁动态几何。本轮 editable synthetic BLAS 不做 compaction，避免把一次性数据美化成稳定内存收益。[NVIDIA：Acceleration Structure Compaction](https://developer.nvidia.com/blog/tips-acceleration-structure-compaction/)

## 4. 选择的最小实验

选择 AABB + inline ray query，而不是先做 triangle mesh，原因是它直接回答最有争议的 candidate 正确性，并以最小 surface-area 真实启用硬件 traversal。Khronos 说明 ray query 可在更广泛 shader stage 中使用，但 traversal 逻辑显式写在 shader 内，可能限制实现可做的调度优化；所以结果不能外推为 ray-tracing pipeline 的绝对上限。[Khronos Ray Query](https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html#ray-query)

### Default-off 实现边界

- root `Cargo.toml` 与 `crates/re-flora-vkn/Cargo.toml` 新增 `rtx-voxel-experiment` feature，默认 feature 集不包含它。
- `src/cli.rs::rtx_voxel_benchmark` 仅在 feature-on 接受 `--rtx-voxel-benchmark PATH`；feature-off 明确报错。
- `crates/re-flora-vkn/src/context/vulkan_context.rs::DeviceCapabilities::hardware_ray_query` 是创建 device 前的能力请求。
- `crates/re-flora-vkn/src/context/device.rs::device_extension_requirements` 只在该能力为 true 时请求 `VK_KHR_deferred_host_operations`、`VK_KHR_acceleration_structure`、`VK_KHR_ray_query`；`create_logical_device` 才链接 `PhysicalDeviceAccelerationStructureFeaturesKHR` 与 `PhysicalDeviceRayQueryFeaturesKHR`。
- `crates/re-flora-vkn/src/descriptor/descriptor_pool.rs::new_for_hardware_ray_query` 私有化 AS descriptor pool，不扩大默认 pool。
- `crates/re-flora-vkn/src/rtx/acceleration_structure::build_aabb_blas_profiled`、`build_tlas_profiled` 真正调用 `vkCmdBuildAccelerationStructuresKHR` 并用 GPU timestamps 计时。
- `shader/slang/rtx_voxel_benchmark.slang` 声明 `RaytracingAccelerationStructure`，执行 `RayQuery::TraceRayInline/Proceed/CommitProceduralPrimitiveHit`。
- `src/rtx_voxel_benchmark.rs::run` 只持有一个 synthetic snapshot，完成 artifact 后在 production renderer 初始化前 drop；它没有订阅 terrain edit，也没有成为第二套生命周期。

### Hardware 能力证据

本机：

| 项 | 实测 |
|---|---:|
| GPU | NVIDIA GeForce RTX 3060 Ti |
| Driver | 580.159.04 |
| VRAM | 8192 MiB (`nvidia-smi`) |
| Vulkan device API | 1.4.312 |
| device UUID | `2cf8ee5131672062b2a587c546461acb` |
| driver UUID | `c7d8a05ebe165883a8186d61bf0812b6` |
| AS extension / feature | true / true |
| ray query extension / feature | true / true |
| ray-tracing pipeline extension | true（本实验未创建 RT pipeline/SBT） |
| max geometry / instance / primitive | 16,777,215 / 16,777,215 / 536,870,911 |
| scratch address alignment | 128 bytes |

这些值由 `src/rtx_voxel_benchmark.rs::machine_identity`、`nvidia-smi` 与 `vulkaninfo` 写入 `summary.json`。两份 TOML 还记录每个非空 BLAS/TLAS 的 primitive count、非零 GPU build 时间与真实 candidate count；因此不是 CPU 模拟、源码存在性或空 dispatch。

## 5. 可复现实验方法

### 5.1 Local traversal/correctness workload

- release build；32³ deterministic voxel volume。
- density：5%、25%、75%，并叠加 deterministic shell/cavity、diagonal、world boundary、macro seam fixture。
- macro：2³、4³、8³。
- 每个 configuration：initial build；随后清空 `[8,8,8]` 所在的完整宏块，使一个 AABB primitive 消失；再 full rebuild。
- 每阶段 256×256 = 65,536 rays；固定 camera-like grid 加 10 条 axis-parallel、inside-volume、boundary、diagonal、grazing、两侧 seam rays。
- warm-up 后按 software / hardware / hardware / software 采样；两份独立 artifact，所以每个 mode/阶段有 4 个 GPU timestamp 样本。
- software shader DDA 与 hardware candidate 内 DDA 使用同一 occupancy，但 correctness reference 是 Rust `cpu_reference` 的独立 CPU DDA；比较 hit/miss、voxel index、hit t。
- candidate budget 4096，shader DDA budget 512；任一 exhaustion 均使汇总器失败。

### 5.2 Whole-frame workload

- 生产 app release 路径：`--hidden --mute --perf --camera-snapshot player-default --auto-exit 8`。
- hidden 仍创建 native window、Vulkan surface 与 swapchain；四次实际 render extent 都是 1600×1000。
- 顺序 A1 / B1 / B2 / A2。A 是 default feature；B 编译 `rtx-voxel-experiment` 并在启动时生成 hardware artifact，然后 drop 实验资源，再进入相同 production frames。
- 每 run 取最后 64 条完整 `[PERF][GPU_FRAME_SCOPE]`；日志每 30 rendered frames 采一条，因此每 run 覆盖最后 1,920 帧。四 run 合计 256 个 frame scope samples。

### 5.3 原始 artifact 与复算

- `docs/evidence/rtx_voxel_hardware_rt/raw/rtx_b1.toml`
- `docs/evidence/rtx_voxel_hardware_rt/raw/rtx_b2.toml`
- `docs/evidence/rtx_voxel_hardware_rt/raw/frame_a1.log`
- `docs/evidence/rtx_voxel_hardware_rt/raw/frame_b1.log`
- `docs/evidence/rtx_voxel_hardware_rt/raw/frame_b2.log`
- `docs/evidence/rtx_voxel_hardware_rt/raw/frame_a2.log`
- 汇总：`docs/evidence/rtx_voxel_hardware_rt/summary.json`
- 脚本：`scripts/summarize_rtx_voxel_hardware_rt.py`

复算命令：

```bash
python3 scripts/summarize_rtx_voxel_hardware_rt.py \
  --ray-query-artifact docs/evidence/rtx_voxel_hardware_rt/raw/rtx_b1.toml \
  --ray-query-artifact docs/evidence/rtx_voxel_hardware_rt/raw/rtx_b2.toml \
  --frame-run A1=docs/evidence/rtx_voxel_hardware_rt/raw/frame_a1.log \
  --frame-run B1=docs/evidence/rtx_voxel_hardware_rt/raw/frame_b1.log \
  --frame-run B2=docs/evidence/rtx_voxel_hardware_rt/raw/frame_b2.log \
  --frame-run A2=docs/evidence/rtx_voxel_hardware_rt/raw/frame_a2.log \
  --binary-a target/release/re-flora-rtx-a \
  --binary-b target/release/re-flora-rtx-b \
  --tail-samples 64 \
  --output docs/evidence/rtx_voxel_hardware_rt/summary.json
```

汇总器校验两份 artifact 的 machine/workload 完全相同，要求 AS/ray-query capability 为 true、candidate 与 GPU build time 非零，并在任一 correctness/exhaustion 计数非零时失败。测量 binary SHA-256：A `2d0b4a5e...58830`，B `55a1e352...a1544`；完整值在 `summary.json`。

## 6. 结果

### 6.1 Local traversal

下表均为两份 artifact、每份 B/B 与 A/A 采样合并后的 GPU timestamp 中位数；speedup = software / hardware。initial 与 edit 都通过完整 correctness gate。

| density | macro | phase | software ms | hardware ms | speedup | reject candidates | occupied macros |
|---:|---:|---|---:|---:|---:|---:|---:|
| 5% | 2³ | initial | 3.8549 | 4.5480 | 0.848× | 72.3% | 1,429 |
| 5% | 2³ | edit | 4.6858 | 4.4961 | 1.042× | 72.1% | 1,428 |
| 5% | 4³ | initial | 4.1596 | 4.2774 | 0.972× | 80.1% | 482 |
| 5% | 4³ | edit | 4.1616 | 4.6370 | 0.897× | 79.8% | 481 |
| 5% | 8³ | initial | 4.1997 | 4.2748 | 0.982× | 64.6% | 64 |
| 5% | 8³ | edit | 4.6756 | 4.4621 | 1.048× | 64.8% | 63 |
| 25% | 2³ | initial | 4.9772 | 4.7719 | 1.043× | 54.5% | 3,683 |
| 25% | 2³ | edit | 5.2548 | 4.2863 | 1.226× | 54.3% | 3,682 |
| 25% | 4³ | initial | 5.8571 | 4.9123 | 1.192× | 38.3% | 511 |
| 25% | 4³ | edit | 5.4019 | 4.7235 | 1.144× | 38.1% | 510 |
| 25% | 8³ | initial | 5.5160 | 5.3841 | 1.025× | 14.7% | 64 |
| 25% | 8³ | edit | 5.0527 | 4.8935 | 1.033× | 15.7% | 63 |
| 75% | 2³ | initial | 9.0860 | 5.5329 | 1.642× | 18.0% | 4,069 |
| 75% | 2³ | edit | 9.2142 | 6.5779 | 1.401× | 18.0% | 4,068 |
| 75% | 4³ | initial | 9.3256 | 5.7710 | 1.616× | 11.2% | 511 |
| 75% | 4³ | edit | 9.0538 | 5.1167 | **1.769×** | 11.1% | 510 |
| 75% | 8³ | initial | 9.7332 | 7.4476 | 1.307× | 2.1% | 64 |
| 75% | 8³ | edit | 9.7015 | 6.8765 | 1.411× | 2.1% | 63 |

解释：

- 4³ 不是统一最优。它在 75% edit 最好，但在 5% initial/edit 都慢于 software；75% initial 又是 2³ 略快。
- 8³ 的 BLAS 最小，但宏块内 DDA 工作更大；2³ 的 exact work 小，但 primitive/AS 更多。最优点随 density 与 ray distribution 改变。
- 这个 software baseline 是同 shader 内完整 32³ grid DDA，不是 production Contree；它控制了 ray/occupancy/output，却不能代表将 `marchScene` 换掉后的实际速度。局部结果只能筛选方向，不能直接承诺 production 倍数。

### 6.2 AS build、edit rebuild 与显存

代表性的 initial 中位数：

| density | macro | BLAS primitives | BLAS GPU build ms | BLAS host wait ms | BLAS bytes | scratch bytes | TLAS GPU build ms |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 5% | 2³ | 1,429 | 0.1800 | 1.2425 | 55,424 | 111,232 | 0.0422 |
| 5% | 4³ | 482 | 0.1472 | 0.5158 | 19,968 | 39,168 | 0.0411 |
| 5% | 8³ | 64 | 0.1005 | 0.3519 | 4,224 | 7,296 | 0.0412 |
| 75% | 2³ | 4,069 | 0.1853 | 0.5524 | 154,240 | 312,192 | 0.0377 |
| 75% | 4³ | 511 | 0.1210 | 0.4481 | 20,992 | 41,088 | 0.0370 |
| 75% | 8³ | 64 | 0.0888 | 0.3317 | 4,224 | 7,296 | 0.0364 |

编辑后的 BLAS GPU rebuild 范围是 `0.0899–0.1903 ms`，TLAS 是 `0.0365–0.0413 ms`。这是 32³ synthetic volume、一个 BLAS/一个 TLAS instance 的结果；不能线性外推到 512³ world 或多 chunk publication。

实验 live logical resources 为 `4.133–4.429 MiB`，其中约 4 MiB 是 ray/result/occupancy 公共 buffer，不是 AS 本身；AS 的可比数字已单列。进程看到的 device-local heap peak 都是约 `429.8 MiB`，没有足够粒度给出小 AS 的 heap delta，所以报告不声称“只增加了某个精确显存值”。

### 6.3 正确性

| 项 | 最终结果 |
|---|---:|
| hardware ray/CPU reference comparisons | 4,718,592 |
| false positives | 0 |
| false negatives | 0 |
| wrong voxel | 0 |
| hit-t over tolerance | 0 |
| max hit-t error | 0.00008392334 |
| tolerance | 0.002 |
| traversal exhaustion | 0 |
| committed primitive/t disagreement | 0 |

覆盖标签由 artifact 固化：surface、cavity、world boundary、axis parallel、diagonal、grazing、macro seam、dynamic edit。它证明 32³ deterministic fixture 上的 AABB candidate 过滤正确；没有证明 512³ production atlas 的全部浮点尺度、跨 chunk transform、透明材质、多 instance overlap 或长期连续编辑。

### 6.4 Whole frame 与 Amdahl

每 run 最后 64 个 scope 样本：

| run | frame.render median / p95 ms | tracer.render median / p95 ms | tracer.pass median / p95 ms | shadow prepass median / p95 ms |
|---|---:|---:|---:|---:|
| A1 | 1.635 / 2.080 | 0.481 / 0.483 | 0.229 / 0.230 | 0.771 / 1.216 |
| B1 | 1.632 / 2.103 | 0.478 / 0.483 | 0.228 / 0.230 | 0.769 / 1.228 |
| B2 | 1.625 / 2.075 | 0.477 / 0.480 | 0.226 / 0.228 | 0.768 / 1.215 |
| A2 | 1.628 / 2.072 | 0.477 / 0.481 | 0.226 / 0.228 | 0.769 / 1.219 |

平衡合并 A1+A2 与 B1+B2：

| scope | A median ms | B median ms | A/B |
|---|---:|---:|---:|
| frame.render | 1.6335 | 1.6265 | 1.004× |
| tracer.render | 0.4790 | 0.4770 | 1.004× |
| tracer.pass | 0.2280 | 0.2260 | 1.009× |
| tracer.shadow_prepass | 0.7700 | 0.7690 | 1.001× |

这些接近 1.0 的差异是噪声/漂移尺度，且 B 的 hardware query 已在 production frames 前结束，不能归因为 RTX。它只支持“feature-off 保持原行为、一次性实验不常驻”。

Amdahl 计算：

```text
f_tracer_pass   = 0.2280 / 1.6335 = 13.96%
f_tracer_render = 0.4790 / 1.6335 = 29.32%
S_local_best    = 1.7695

S_frame(pass)   <= 1 / ((1-f) + f/S) = 1.0646
S_frame(tracer) <= 1 / ((1-f) + f/S) = 1.1462

S_local -> infinity:
pass-only <= 1.1622
all tracer.render <= 1.4149
```

这只是说明“即使 synthetic 局部倍数可移植，也达不到数量级”；production Contree workload 与 synthetic DDA 不等价，因此不能把 1.0646/1.1462 当预测值。

## 7. Go / No-go 与后续门槛

### 明确决定

1. **AABB semantic feasibility：go。** 规范与实测都否定“AABB candidate false positives 必然成为错误 hit”。
2. **当前 AABB macro ray-query 替换 production tracer：no-go。** 最好仅 1.769×，稀疏时可慢到 0.848×；没有数量级局部价值，更没有 production 整帧证据。
3. **现在投入 triangle/greedy full-world lifecycle：no-go。** triangle 可能更快，但会先引入 mesh build、增量 topology、publication、retirement 与额外显存；当前数据没有证明这笔复杂度会突破 Amdahl。
4. **保留 default-off 实验与 capability seam：go。** 它可复跑、不会改变默认 device feature/resource boundary，也能作为 triangle 或 production consumer 垂直切片的共同基准。

### 若继续，最小下一步

只选一个当前占帧足够大的真实 consumer（优先测量 `tracer.shadow_prepass` 的 terrain visibility 部分，而不是把整个 primary renderer 重写），建立以下 acceptance gate：

- 使用 production `chunk_atlas` 与 `VisibleTerrainPublication` revision，不复制材质 authority。
- 只为 1 个 dirty chunk staging AABB 与 triangle/greedy 两个 backend；同一 rays、同一 outputs、同一 atlas reference 做 A/B/B/A。
- publication 原子性：affected chunks 全部 ready 后一次 TLAS/descriptor publish；旧 snapshot 在 fence 后释放。
- density、ray coherence、edit footprint 分层；记录 build CPU/GPU、scratch/AS/metadata、cache hit、candidate rejection。
- 必须先在 consumer 内达到显著局部收益，再看 whole-frame；若整帧上界仍低，则删除 backend。
- 再测非 NVIDIA 的 `VK_KHR_ray_query` 设备；unsupported/missing extension 必须无条件 fallback 到当前 Contree。当前结果只对列出的 3060 Ti/driver 有效。

## 8. 未验证边界

- 没有把 AS 接入 512³ production atlas、真实 chunk streaming 或 terrain edit publication。
- 没有实现/测量 triangle surface、greedy mesh、ray-tracing pipeline/SBT、compaction、async compute build。
- 没有测多 BLAS/TLAS instance overlap、跨 chunk floating-point scale、透明/玻璃 hit semantics、材质 atlas 的真实 cache 行为。
- 没有 NVIDIA 之外的硬件结果；`ray query` 是跨厂商 Vulkan KHR API，但性能与支持不能从本机推断。
- 整帧 B 不是 integrated RTX renderer；因此本报告没有声称 production frame speedup。
- 每个 local cell 只有 4 个 timing samples，适合方向筛选，不适合微小差异排序；0.9×–1.1×应视为无明确收益。

## 9. 一手资料

- [Khronos Vulkan Specification — Ray Traversal](https://docs.vulkan.org/spec/latest/chapters/raytraversal.html)
- [Khronos Vulkan Specification — Acceleration Structures](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html)
- [Khronos Vulkan Guide — Ray Tracing](https://docs.vulkan.org/guide/latest/extensions/ray_tracing.html)
- [Khronos Vulkan Samples — Basic Ray Queries](https://github.khronos.org/Vulkan-Site/samples/latest/samples/extensions/ray_queries/README.html)
- [NVIDIA — Best Practices for Using NVIDIA RTX Ray Tracing](https://developer.nvidia.com/blog/best-practices-for-using-nvidia-rtx-ray-tracing-updated/)
- [NVIDIA — Effectively Integrating RTX Ray Tracing](https://developer.nvidia.com/blog/effectively-integrating-rtx-ray-tracing-real-time-rendering-engine/)
- [NVIDIA — Acceleration Structure Compaction](https://developer.nvidia.com/blog/tips-acceleration-structure-compaction/)

项目事实均来自固定基线或本分支的具体源码符号；性能与机器事实只来自本 worktree 的 release artifact/log 与 `summary.json`。
