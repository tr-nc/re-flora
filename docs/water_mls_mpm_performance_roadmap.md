# Water MLS-MPM 性能 Roadmap

## 现状摘要

水体性能的第一轮热点清理已完成。水 box 已从固定 pond `(0,0,0)..(2,1,2)` 扩展到全世界 `(0,0,0)..(5,2,5)`（50 个 terrain chunks），grid 按比例放大到 `160×64×160` 保持每世界单位 32 cells 的密度。默认仍无水粒子，用户用浇水壶工具（按 4）在世界任意位置点击放水。

`2a099d4f stabilize incompressible water` 后，默认水更新已从弱压缩 EOS 切到不可压 pressure projection：`pressure_projection_iterations=8`、`particle_spacing_relaxation_iterations=2`、`substep_dt=1/120s`、`linear_damping_per_sec=0.8`；`--water-pressure-iterations 0` 才回到旧 EOS 路径。下面的早期 kernel 基准仍保留作历史对照，新的优化优先级已转向 terrain edit 时的水专用 collider/cache hitch。

关键结论：

| 指标 | 旧 64x32x64 grid | 当前 160x64x160 grid |
|---|---|---|
| 每世界单位 cells | ~13 | 32 |
| dx (cell 大小) | ~0.078 | ~0.031 |
| avg/substep (2048 粒子, 120 Hz) | ~0.9-1.0 ms | ~2.0-2.2 ms |
| active nodes/substep | ~3k | ~50k |
| terrain_shadow_false_skips | 0 ✅ | 0 ✅ |
| terrain_no_sdf | 0 | ~170（空 chunk，不影响性能） |

`~2.2 ms/substep` × 120 Hz = 约 264 ms/s 主线程 CPU 时间，占 ~44% 帧预算（@200 fps）。提升 grid 密度恢复了碰撞精度，同时增大了 grid 成本。

## 已完成工作的时间线

### 2026-05-16

Phase 1-5：从 `~4.9 ms/substep` 基线砍到 ~1.0-1.85 ms/substep，完成方法见后。

### 2026-05-17：实现级清理（Phase 6）

6A-6E：空模拟跳过、sparse grid、strict collider scope、region cache rebuild、SDF sample reuse、diagnostic throttle。

### 2026-05-17（续）：世界级水体

- **水 box 扩展到全世界** `(0,0,0)..(5,2,5)`：`enqueue_startup_water_terrain_collider_rebuilds` 从 4 个 chunk 变成 50 个。
- **grid 比例放大** `160×64×160`：保持每世界单位 32 cells（同 `2/64` 旧比例），恢复碰撞精度。
- **`SHOVEL_RAY_QUERY_DISTANCE`** 从 2.0 提到 10.0 覆盖全 world raycast。
- **`water_terrain_focus_ws`** 从固定 box 中心改为 camera 位置，collider 构建优先处理玩家附近。

相关提交：

- `5f3eab50` make water placeable anywhere in the world
- `d077d882` keep water grid density proportional to world extent

### 2026-05-17（续）：不可压水稳定化

- 增加 grid pressure projection：`P2G -> gravity/boundary -> Poisson pressure projection -> G2P`。
- `pressure_projection_iterations > 0` 时关闭 EOS pressure，粒子 `j` 固定为 `1.0`。
- 为避免 APIC 能量导致可见震荡，默认用偏 PIC 的 transfer：`INCOMPRESSIBLE_APIC_BLEND=0.10`，并 clamp affine。
- 初始水从 2D surface sheet 改为薄 3D volume，并增加 marker particle spacing relaxation，避免直接渲染粒子塌成一个点。
- 默认配置变为 `120 Hz`、`8` 次 pressure projection、`2` 次 spacing relaxation、`0.8/s` damping。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p re-flora-water` 通过；hidden log 中 `j=1.000..1.000`、`terrain_penetrating=0`、`non_finite=0`。

相关提交：

- `2a099d4f` stabilize incompressible water

### 2026-05-17（续）：terrain edit 性能回归确认

这次回归不是普通地形 mesh/collider 队列失控，而是 terrain edit 成功后同步接入了水专用地形碰撞链路：

```text
terrain edit
 -> deferred chunk mesh rebuild
 -> mark_water_terrain_source_chunk_dirty(chunk)
 -> refresh_water_solid_sample_chunk              // 主线程，GPU atlas solid sample/readback
 -> deferred water terrain collider rebuild       // LatestChunkQueue，全局 1 个 worker inflight
 -> publish_water_terrain_collider_chunk          // 主线程
 -> water_sim.rebuild_terrain_grid_cache_for_chunk // 主线程，约 4-5 ms/chunk
```

队列确实限制了“每帧/每 chunk 无限构建”：普通 chunk rebuild 每帧最多 pop 一个；水 SDF collider worker 全局最多一个 inflight；同 chunk revision 会保留最新。但这只能防止无界并发，**不能消除连续编辑时每次成功编辑都触发一次水源刷新 + SDF build + cache rebuild** 的成本。

已有 log 证据：

| log | 场景 | source refresh rev>1 | SDF collider build rev>1 | water grid cache rebuild | frame_dt 诊断 |
|---|---|---:|---:|---:|---|
| `re-flora-20260517-165534.900-86173.log` | visible 手动验证，粒子 `1024..2560` | `n=278 avg=0.96ms p95=1.41ms max=2.65ms` | `n=276 avg=7.15ms p95=7.39ms max=11.31ms` | `n=268 avg=4.73ms p95=5.31ms max=5.96ms`，约 `40.6k` nodes/update | `n=189 avg=20.1ms p95=75.9ms max=138.4ms`，`>=50ms` 26 次，`ran_substeps=8` 25 次 |
| `re-flora-20260517-181133.047-94268.log` | 后续编辑 trace，粒子 `48..1344` | `n=319 avg=0.91ms p95=1.08ms max=1.18ms` | `n=319 avg=7.12ms p95=7.62ms max=11.61ms` | `n=311 avg=4.71ms p95=5.30ms max=6.45ms`，约 `40.8k` nodes/update | `n=148 avg=13.2ms p95=20.4ms max=36.7ms`，`>=33ms` 2 次 |

热点集中在少数正在编辑的 chunk 上，而不是全世界随机扩散：

- `86173`：`(0,0,1)` 134 次、`(1,0,1)` 73 次、`(0,0,0)` 44 次、`(1,0,0)` 24 次。
- `94268`：`(0,0,1)` 180 次、`(1,0,1)` 98 次、`(2,0,1)` 15 次、`(1,0,0)` 13 次、`(0,0,0)` 12 次。

注意 tail 中存在 `sdf_hash` 连续相同但仍 rebuild cache 的情况，例如 `rev 178` 和 `rev 179` 都是 `f34133c5d48ff0ca`，说明可以进一步用内容 hash 跳过无效发布/cache rebuild。

### 耗时路径现状

```
Report 1 (first second):  0.90 ms/substep — 粒子还聚在原处
Report 2 (second):        2.02 ms/substep
Report 3 (third):         2.15 ms/substep
Report 4 (4-5s):          2.24 ms/substep — start substep spread wide
```

成分（稳态 ~2.2 ms）：`clear ~0.17`, `p2g ~0.40`, `grid ~0.84`, `g2p_gather ~0.32`, `g2p_box ~0.03`, `g2p_terrain ~0.22`, `g2p_repair ~0.05` ms/substep。

**当前最大项：`grid`（38%）和 `p2g`（18%）和 `g2p_gather`（15%）。** Terrain 本身（`g2p_terrain` + `g2p_box`）只占 ~11%。

## 关键概念说明

### 命名约定

为避免后续讨论里多个 collider 概念混在一起，本文统一使用：

- **SDF Collider**：水体专用、按 terrain chunk 独立构建的 signed-distance collider。当前代码主要对应 `WaterTerrainColliderChunk` / `WaterTerrainColliderSet`，数据核心是 `sdf_ws`。它只用于水的 terrain collision、pressure projection solid 判断、以及 water grid cache 采样。
- **Raycast Collider**：CPU 端 terrain raycast/query 用的碰撞/查询结构。当前实现由 contree CPU cache / scene accel 路径提供，用于 shovel raycast、玩家碰撞查询、terrain height query、audio occlusion 等。它不直接参与水的 SDF collision。代码级重命名可后续单独做机械 rename。
- **Water grid cache**：SDF Collider 到 MLS-MPM water grid 的派生缓存，当前代码对应 `terrain_grid: Vec<WaterTerrainGridSample>`。它不是新的 collider 源数据，而是为了让每个 water substep 快速读取 SDF/normal/solid 状态。

当前两条 collider/query 链路共享 terrain voxel atlas 作为源，但彼此不互相生成：

```text
terrain voxel atlas
 ├─ Raycast Collider path: contree / CPU cache / scene accel -> CPU raycast/query
 └─ SDF Collider path: 32^3 solid samples -> SDF chunk -> water grid cache -> water sim
```

### SDF Collider 的当前采样方式

当前 SDF Collider 的 source refresh 对每个 terrain chunk 执行一次 GPU compute dispatch 和一次 readback：

```text
GPU terrain voxel atlas 当前 chunk
 -> sample_chunk_atlas_solid_grid(sample_dim=32x32x32)
 -> CpuSolidVoxelChunk
 -> signed_distance_from_solid_samples()
 -> SDF Collider chunk
```

注意当前 shader 是 **point downsample**：`32^3` 个 sample point 各自读取原始 chunk 中一个 voxel。它不是把 `256/32≈8` 个 voxel 的块做 OR/average/max reduction。因此当前 source hash/unchanged skip 可以直接围绕这 `32^3` 个 sample 设计。

当前取样位置是 endpoint-aligned：每轴 `source_voxel = floor(sample_id * 256 / 31)`，即 `0, 8, 16, 24, 33, ..., 247, 255`。它覆盖 chunk 两端边界，但不是每个 `8` voxel block 的中心。后续可改成 center-biased point sampling，例如每轴 `source_voxel = min(sample_id * 8 + 4, 255)`（每个 8-wide block 中间四个 voxel 中任选一个代表点，先取 `+4`），改动面积小，通常能更好代表 block 内部。

当前 SDF Collider **不读取邻居 chunk**，也没有 SDF source halo；一个 chunk 的 SDF 只依赖自己这个 chunk 的 solid samples。邻居 halo 目前只存在于 water grid cache rebuild 的更新范围上，用来避免缓存插值读到旧数据。SDF source halo 可以作为未来质量选项，但它会引入 neighbor dependency，不是当前编辑性能优化的优先项。

### `no_sdf`

`terrain_no_sdf` = **粒子落在没有 terrain collider chunk 覆盖的区域**。

成因：`build_water_terrain_collider_chunk` 在 chunk 内找到 `solid_sample_count == 0`（全是空气）时跳过该 chunk，不加入 collider 集合。粒子落在这些区域时精确 collider 采样返回 `None`，统计为 `no_sdf`。

**这是持久状态，不是临时 cache miss。** 只要该 chunk 确实没有固体，它就永远不会被构建 collider，那里的粒子一直 `no_sdf`。只有当你编辑地形往里面放固体后，collider 才会被构建。

**不影响性能。** `no_sdf` 粒子只走一次 HashMap miss 就结束，不做任何 SDF 采样或位置修正。比真正碰到 terrain 做 trilinear + projection 的粒子快得多。当前 ~170/2048 个 `no_sdf` 粒子的路径开销可以忽略。

### 264 ms/s

`avg/substep` × 每秒 substep 数（120 Hz）的绝对 CPU 时间。水模拟跑在**主线程**上，这个时间直接占用帧预算。

## 当前风险/注意点

- `no_sdf` 在世界级水体下不可避免：一定有 chunk 没有固体。不是 bug。
- `--water-profile performance` 仍需 visible 观感验证。
- **terrain edit hitch 已确认**：不是普通编辑 collider 被水算法改坏，而是水专用 terrain collider/cache 更新链路在连续编辑时持续占用主线程预算。
- 水 terrain source refresh 当前在 `mark_water_terrain_source_chunk_dirty` 中同步执行；即使后续 SDF build 有队列，refresh 本身已经吃掉当前帧约 `0.8-1.4ms/chunk`。
- `rebuild_terrain_grid_cache_for_chunk` 当前在 publish 时同步执行，单次约 `4.5-5.3ms`，是编辑 hitch 的主要主线程成本。
- 当前 grid 是单块 160x64x160 = 1.6M 节点。永远 dense allocated。sparse 只优化了 per-substep 循环，不减少内存。

## 后续 Roadmap

### Step 0：terrain edit / water collider 解耦（当前最高优先级）

目标：地形编辑必须保持响应；水可以短暂使用旧 terrain collider，等编辑稳定后再追上。

建议按低风险到高风险推进：

1. **把 source refresh 也纳入 deferred/budgeted 队列**  
   不要在 `mark_water_terrain_source_chunk_dirty` 里立即 GPU sample/readback。只记录 dirty chunk + revision，每帧按预算刷新最多 N 个或最多 X ms。

2. **per-chunk debounce/coalesce**  
   连续编辑同一个 chunk 时，等最后一次 dirty 后 `100-250ms` 再刷新水 collider；或者限制同一 chunk 最快每 N 帧/每 X ms 发布一次。水在编辑中允许暂时使用旧 collider。

3. **内容 hash / unchanged skip**  
   对 SDF Collider 的 `solid_samples` 或 `sdf_hash` 做内容比较。如果 solid occupancy/SDF hash 没变，不发布 collider，也不 rebuild water grid cache。已有 log 看到相同 `sdf_hash` 仍触发 cache rebuild。当前 source 是 `32^3` point samples，适合先做 cheap solid-sample hash。

4. **SDF source point sample 改为 block-center 代表点**  
   当前每轴用 `floor(sample_id * source_dim / (sample_dim - 1))`，包含 endpoint，但代表点偏 block 低端且间距 8/9 混合。后续把 `256 -> 32` 的采样改为每个 8-wide block 的中间代表点，例如 `source_voxel = min(sample_id * 8 + 4, 255)`。中间有四个候选 voxel（`+3/+4` 或附近），先选一个固定点即可；不做 OR reduction，保持改动小。

5. **只更新会影响水的 chunk**  
   除了 water domain bounds，还要用 active particle AABB / water occupancy AABB 过滤。没有水粒子、或 dirty chunk 距离水粒子很远时，跳过或低优先级延迟。

6. **budgeted water grid cache rebuild**  
   `rebuild_terrain_grid_cache_for_chunk` 不要 publish SDF Collider 时同步无预算地执行。改成 pending cache ranges，每帧最多处理一个或最多 X ms；必要时把 cache rebuild 也 worker 化并用 revision swap。

7. **编辑中限制 water catch-up**  
   如果 frame_dt 因编辑升高，水模拟追帧会触发 `ran_substeps=8`，进一步放大 hitch。编辑活跃时可临时降低 max catch-up substeps 或丢弃过量 accumulator，优先保障交互。

需要新增诊断：

- water source dirty/pending/active/completed/stale counts
- debounce skips、content-hash unchanged skips
- SDF Collider source refresh ms、SDF build ms、publish ms、water grid cache rebuild ms 的 per-frame budget 汇总
- cache pending ranges、每帧处理 nodes
- terrain edit active 标记与 water `ran_substeps` 关系

完成标准：连续编辑时 `frame_dt p95` 不因水 terrain 更新超过一帧预算；没有 `ran_substeps=8` 追帧尖峰；水在编辑停止后能在可接受延迟内更新到最新 terrain。

### Step 1：visible 观感验证

不可压水已经通过初步 visible 验证，但 edit 优化后还要重新验证。只看 hidden 日志不够。

执行：

```bash
cargo run --release -- --perf --water-profile performance --water-particles 2048
```

观察：

- 水面是否离地有缝或插地过深。
- 水是否过弹/过黏/抖动。
- 地形编辑后水是否稳定。

如果需要地形编辑，告诉我我来手动操作。否则继续 `--water-edit-soak`。

完成标准：视觉可接受后进入 benchmark/kernel 优化。

### Step 2：更新 release 基准表

跑三类 release hidden benchmark：

- 空水体：`--water-profile performance`
- 显式水体：`--water-profile performance --water-particles 2048`
- 编辑 soak：`--water-profile performance --water-particles 2048 --water-edit-soak`

提取：`avg/substep`、`p2g`、`grid`、`g2p_gather`、`g2p_terrain`、active nodes、edit collider build/cache rebuild 时间。写入本文件。

### Step 3：kernel 微优化

如果 Step 2 确认当前瓶颈是 `grid`、`p2g`、`g2p_gather`，不来自 terrain fallback：

- 检查 3×3×3 kernel 的可 hoist 重复计算（weights、bounds、base coord）
- 评估 per-axis weights/gradients 的 stack 缓存
- 保持单线程，不要并行

安全条件：terrain fallback slack 不动，`terrain_shadow_false_skips 0` 不动。

### Step 4：空 chunk collider 生成

当前 `solid_sample_count == 0` 的 chunk 跳过。可以改为生成全 `+∞` SDF 的 collider（代表"全空"），消除 `no_sdf`。

好处：

- G2P cache 全部 `has_sdf = true`，消除 ExactFallback 路径的 HashMap miss
- 诊断日志更干净

代价：

- 50 个 chunk → 额外约 7ms × N 个空 chunk 的构建时间
- 增加了 collider 集合中不必要的空 chunk 条目

### Step 5：进一步缩小 water terrain cache 成本

在 Step 0 解耦后，再做 cache 本身的算法优化：

- `terrain_grid_cache_range_for_chunk` 当前一个 1m chunk 会覆盖约 `40k-42k` water grid nodes；检查 halo/band 是否还能安全缩小。
- 对每个 collider chunk 预计算 influence bounds，只重采样 SDF band 内节点。
- 维护 chunk->grid-node 或 tile 索引，避免每次 region rebuild 都访问完整长方体。
- 若 cache rebuild worker 化，保证 `terrain_grid` swap 有 revision guard，G2P/pressure projection 永远读一致快照。

### Step 6：评估水线程化

将 `PondWaterSim::update()` 移到独立线程。

**优点**：主线程省掉 ~2.2 ms/帧（~44% @200fps）；水模拟不受帧时间波动影响。

**风险与复杂度**：

- 粒子数据延迟一帧（~5ms @200fps），对水体行为很可能不敏感。
- terrain collider 同步链路过长：collider worker 线程 → 主线程 → 水线程。
- 线程寿命管理、退出信号、data race 需要仔细设计。
- 当前 ~2.2 ms/substep × 120 Hz = 264 ms/s。在 200 fps（5ms 预算）下是 44%；在 60 fps（16ms 预算）下只有 14%。

**建议先做 Step 3-5，确认单线程优化后的实际成本，再决定线程化是否值得。**

### Step 7：产品化 `performance profile`

如果 visible soak 通过，把 `--water-profile performance` 暴露到 GUI/settings。保留 `default` 质量路径。

### Step 8：高风险路线

只在 CPU 单线程优化见底后考虑：

1. CPU 并行 G2P（按粒子分块，风险较低）
2. CPU 并行 P2G（thread-local grid tile 累加，风险高于 G2P）
3. GPU water（只在 CPU 收益不足且同步成本可控时）
4. adaptive CFL（稳定性改动，必须 visible 验证）

## 验证策略

Benchmark 以 release hidden app run 为准；debug build 非性能证据。

常用命令：

```bash
cargo fmt --check
cargo check
cargo test
cargo test -p re-flora-water
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 3 --perf --water-profile performance'
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance --water-particles 2048'
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 14 --perf --water-profile performance --water-particles 2048 --water-edit-soak'
zsh -lc 'source ~/.zshrc && cargo run --release -- --perf --water-profile performance --water-particles 2048'
cargo run --release -- --tail-latest-log 200
```

验收指标：

- `avg/substep` 下降且可复现。
- `terrain_shadow_false_skips 0`。
- `no_sdf` 可以非零（空 chunk，路线图 Step 4 可选消除）。
- tight-contact profile 只接受有界 sub-cell `terrain_sdf_min` overlap。
- latest log 无 water / Vulkan / shader 错误。
