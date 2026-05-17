# Water MLS-MPM 性能 Roadmap

## 当前状态

- 默认水体已切到不可压 pressure projection。
- 水域覆盖全世界 `(0,0,0)..(5,2,5)`，grid 为 `160×64×160`。
- terrain edit 后的 SDF/source/cache 链路已分帧、去重、限流。
- SDF/source/cache 队列命名已解耦。
- 手动反馈：当前编辑性能已经明显改善。
- P0 初版已完成：Water Terrain Cache Rebuild 的 `SDF collider chunk -> MLS-MPM water grid cache` 采样/normal 构建已搬到 CPU worker，主线程只做 revision-guarded apply/swap。
- 2026-05-17 手动 visible release perf：cache apply 已不是主要 hitch；实时 terrain edit 的主线程尖峰主要来自 terrain deferred rebuild 和 Terrain SDF Source Refresh；大量水粒子时 CPU water sim 成为主导瓶颈。

## 已完成工作（极简）

- 2026-05-16：水模拟核心热点已从约 `4.9ms/substep` 降到约 `1-1.85ms/substep`。
- 2026-05-17：水体从固定小 pond 扩展到全世界，并保持 grid 密度。
- 2026-05-17：默认水体切到不可压 pressure projection，稳定性改善。
- 2026-05-17：确认 terrain edit hitch 来自水专用 SDF/source/cache 链路。
- 2026-05-17：source refresh、SDF build、water cache rebuild 已进入 chunk queue。
- 2026-05-17：已加入 coalesce、unchanged skip、block-center sample、active-water priority、cache budget、catch-up cap。
- 2026-05-17：SDF/source/cache 命名解耦已完成。
- 2026-05-17：Water terrain cache region rebuild worker 化；edit soak 中原主线程 `5.17ms` cache region rebuild 变为 worker `6.77ms` + 主线程 apply `0.094ms`。
- 2026-05-17：手动 visible release perf 调查完成，确认 P0 cache worker 化达标，并暴露下一批主瓶颈：SDF source refresh / terrain rebuild 主线程成本，以及高粒子数 water sim CPU 成本。
- 2026-05-17：Step 0 初版 instrumentation 已开始：新增 `[PERF][FRAME]` 详细采样、water substep split counters、以及 `tools/parse_perf_log.py` 汇总脚本。

最新相关提交：

- `d7761695` defer water terrain source refresh
- `e921a276` debounce water terrain source refresh
- `bab9fc94` skip unchanged water solid sources
- `202b2a64` sample water solid source at block centers
- `5adaeb99` delay water collider refresh away from particles
- `3dfb863c` budget water terrain cache rebuilds
- `fca74ba0` limit water catchup during terrain work
- `2c617634` coalesce water source refreshes without starvation
- `80b4d0fd` merge water optimization plan into roadmap
- `b12095d5` clarify water roadmap next phases
- `08296202` rename terrain sdf source refresh queue
- `a9aeab4b` rename terrain sdf collider build queue
- `befe1cf9` rename terrain sdf source scheduling

## 当前链路

```text
terrain edit
 -> GPU compute 修改原始 voxel atlas / chunk_atlas
 -> 普通 terrain chunk rebuild queue
    -> surface rebuild
    -> contree rebuild
    -> scene accel update
 -> Terrain SDF Source Refresh Queue
    -> GPU atlas 降采样 256^3 -> 32^3
    -> readback 到 CPU
    -> 更新 CpuSolidVoxelStore
    -> occupancy 未变则跳过
 -> Terrain SDF Collider Build Queue
    -> 32^3 solid occupancy -> SDF collider chunk
    -> worker 线程构建，主线程 publish 最新 revision
 -> Water Terrain Cache Rebuild Queue
    -> SDF collider chunk -> MLS-MPM water grid cache
    -> worker 构建 cache region，主线程 revision-guarded apply/swap
```

注意：第一步 terrain voxel 修改目前仍是同步 GPU compute + fence；后续 queue 从普通 terrain chunk rebuild 开始。

当前这些 queue 都复用 `LatestChunkQueue<T>`：

- 普通 terrain chunk rebuild：`LatestChunkQueue<ChunkRebuildRequest>`。
- Terrain SDF source refresh：`LatestChunkQueue<TerrainSdfSourceRefreshRequest>`。
- Terrain SDF collider build：`LatestChunkQueue<TerrainSdfColliderRebuildRequest>`，额外用 inflight guard 限制全局 1 个 worker job。
- Water terrain cache rebuild：`LatestChunkQueue<WaterTerrainCacheRebuildRequest>`，region 构建在 worker 线程执行；主线程只做最新 revision 检查和结果应用。

## 当前关键事实

- SDF source 当前是 `32^3` point downsample。
- 当前采样点是 block-center：常见 `256 -> 32` 时取 `4,12,...,252`。
- 暂不默认做 full average reduction；它会把每 chunk 读取量从 `32^3` 提到 `32^3 * 512`。
- occupancy unchanged skip 检测的是降采样后的 `32^3` solid bitset。
- unchanged skip 成本很低，通常远小于一次 SDF build 或 cache rebuild。
- 空 chunk 不生成 SDF collider，因此 `terrain_no_sdf` 可以非零；这不是 bug。
- 当前 cache region build 仍可能约 `40k` grid nodes、worker 侧 `5-7ms/chunk`；主线程 apply 目标保持亚毫秒级。
- 当前 `[PERF] frame` 只每 30 帧采样一次，`[PERF][WATER]` 约每秒聚合一次；要做严格逐帧归因，还需要新增 per-frame/spike-only 细分日志。

## 2026-05-17 手动 visible release perf 调查

运行方式：可见窗口 release run，打开 `--perf --water-profile performance`，并启用 app/water/terrain queue debug 日志；运行中手动修改地形并喷/生成水。日志：`target/re-flora-logs/re-flora-20260517-211139.700-125045.log`。

### Frame 采样结论

| 阶段 | 采样帧数 | frame avg | frame max | GPU avg | CPU/other avg | 结论 |
|---|---:|---:|---:|---:|---:|---|
| `0-14s` 预热/稳态 | 65 | `7.25ms` | `15.33ms` | `5.77ms` | `1.28ms` | 基本正常 |
| `14-20.2s` terrain edit burst | 23 | `8.58ms` | `17.37ms` | `4.97ms` | `3.51ms` | terrain source/rebuild 有主线程尖峰 |
| `20.2-23s` 水粒子增加 | 6 | `16.85ms` | `26.13ms` | `4.29ms` | `12.33ms` | water sim 开始主导 |
| `23s+` 水体饱和 | 2 | `74.79ms` | `75.05ms` | `2.32ms` | `72.23ms` | CPU water sim 明确主导 |

### Terrain / SDF / cache 事件统计

| 项目 | 数量 | 执行位置 | 平均 | 最大 | 结论 |
|---|---:|---|---:|---:|---|
| terrain deferred rebuild | 111 | 主线程 | `5.57ms` | `12.21ms` | 实时编辑期间的主线程 spike 来源之一 |
| Terrain SDF Source Refresh | 88 | 主线程 GPU/readback | `5.21ms` 全局；edit burst 中约 `7.24ms` | `8.47ms` | 当前最值得继续异步化/降预算的 terrain-water 链路 |
| Terrain SDF Collider Build | 64 | worker | `7.46ms` | `19.38ms` | 主线程只 publish，但 worker latency 仍会影响追随速度 |
| Water terrain cache build | 34 | worker | `5.43ms` | `8.01ms` | P0 后计算成本已离开主线程 |
| Water terrain cache apply | 34 | 主线程 | `0.108ms` | `0.211ms` | 达标；不再是主要 hitch |

编辑热点几乎都落在 `UVec3(1,0,1)` / `IVec3(1,0,1)`，每次 water cache region 约 `41650` nodes。terrain edit burst 中，water cache worker 总计约 `173.7ms`，但主线程 apply 总计只有约 `3.5ms`。

### Water sim 结论

| 水粒子数 | avg/substep | water total / sec | 观察 |
|---:|---:|---:|---|
| 672 | `~1.55ms` | `~185ms/s` | frame 多在 `6-8ms` |
| 768 | `2.27ms` | `270.6ms/s` | frame 约 `10.6ms` |
| 1056 | `3.91ms` | `469.0ms/s` | frame 约 `22ms` |
| 1344 | `5.37ms` | `639.2ms/s` | frame 约 `26ms` |
| 1584 | `6.79ms` | `828.8ms/s` | 明显卡顿 |
| 1728 | `~7.98ms` | `~890ms/s` | frame 约 `75ms`，CPU/other 约 `72ms` |

当前 water perf breakdown 没有单独记录 `relax_incompressible_particle_spacing()` / pressure projection 细项。高粒子数时，已记录的 `repair/clear/p2g/grid/g2p` 子项与 `total` 之间存在很大 residual，疑似 spacing relaxation 或未拆分的 incompressible 相关成本；需要先补日志再下结论。

### 调查后的优先级含义

1. P0 cache worker 化有效：后续不应再优先优化主线程 cache apply，除非 tile 化是为了降低 worker latency / stale 浪费。
2. 下一批 terrain edit 主线程优化目标应是 Terrain SDF Source Refresh 的 GPU/readback/fence 路径，以及普通 terrain deferred rebuild 的 per-frame 预算/分摊。
3. 水体粒子数超过约 `1000` 后，water sim CPU 成本超过 terrain edit 成本；Phase 4 需要先补齐 `spacing_relax_ms`、`pressure_projection_ms` 等 breakdown，再优化或自适应限流。
4. 如果目标是隐藏主线程帧尖峰，Phase 6 water sim 线程化仍有价值；但它不能替代 water kernel 优化，因为 1728 粒子时总 CPU 成本本身已经接近 `~890ms/s`。

## 后续优化执行计划（基于 2026-05-17 调查）

原则：先补可归因日志，再做单点优化；每个阶段都用 release run 和最新日志表格验收。继续保持现有三段 pipeline：`Terrain SDF Source Refresh -> Terrain SDF Collider Build -> Water Terrain Cache Rebuild`，同 chunk 串行、跨 chunk 流水，worker 不直接修改 `PondWaterSim`。

### Step 0：补齐性能归因日志

目标：让下一轮优化能判断“某帧为什么慢”，而不是只看 30 帧采样和一秒 water 聚合。

任务：

1. 已有初版：新增 per-frame/spike-only 汇总日志 `[PERF][FRAME]`，包含 frame total、egui、GPU/present、terrain deferred rebuild、SDF source refresh、collider queue、cache queue、water update、particle update。
2. 已有初版：`[PERF][FRAME]` 输出 terrain-water 队列 pending/active/inflight；后续还可补 job age/drop/stale 聚合。
3. 已有初版：拆 water substep 计时：`repair`、`clear`、`p2g`、`grid_update`、`pressure`、`g2p`、`spacing_relax`、diagnostics、residual、shadow_measure。
4. 已有初版：`tools/parse_perf_log.py` 可把 latest/指定 log 解析成 frame、water、terrain/cache 表。

验收：一次 visible/manual 或 hidden/script run 后，可以直接得到 frame spike 对应的 subsystem breakdown。

### Step 1：先压 terrain edit 主线程尖峰

依据：手动 edit burst 中 `Terrain SDF Source Refresh` 平均约 `7.24ms/job`，terrain deferred rebuild 平均约 `5.57ms/job`，而 water cache apply 平均只有 `0.108ms`。

任务：

1. 把 SDF source refresh 从“submit 后同帧等待 readback/fence”改成 async GPU readback：本帧 submit，下几帧 poll ready result。
2. 给 source result apply 设每帧预算，保持 latest-per-chunk 和 active-water/camera-near priority。
3. 复查普通 terrain deferred rebuild 的 per-frame budget，避免连续编辑时同帧堆多个高成本 job。
4. 保留 unchanged skip；继续记录 stale/drop/age，防止异步化后水体长期追不上地形。

验收目标：edit burst 中 CPU/other spike 明显下降；source/collider/cache revision 不回退；`terrain_penetrating` 不恶化。

### Step 2：处理高粒子数 water sim CPU 瓶颈

依据：粒子数超过约 `1000` 后，water sim CPU 成本超过 terrain edit 成本；1728 粒子附近 frame 可到 `~75ms`，但当前 breakdown 中 residual 未拆清。

任务：

1. 用 Step 0 的新计时确认 residual 是否主要来自 `relax_incompressible_particle_spacing()` 或 pressure projection。
2. 如果 spacing relaxation 是主因：尝试自适应 iterations、按活跃区域/粒子密度限流、分帧执行或更低成本邻域查询。
3. 如果 pressure projection 是主因：评估 iteration count、活跃 grid bounds、early-exit/residual threshold。
4. 做 MLS-MPM kernel 微优化：复用 kernel weights、减少 index/bounds 计算、减少临时分配。
5. 所有优化都保留对稳定性和碰撞诊断的对照，不用无证据的默认降质换性能。

验收目标：高粒子数 release run 的 `avg/substep` 和 frame CPU/other 稳定下降；`terrain_shadow_false_skips` 保持 0；`terrain_penetrating` 不显著增加。

### Step 3：降低 cache worker latency 和 stale 浪费

依据：P0 已把 cache 计算移出主线程；后续 cache 优化重点不再是 apply hitch，而是 worker job 延迟、stale work 和结果追随速度。

任务：

1. 把 chunk cache rebuild 拆成 tile/range job。
2. 收窄 rebuild bounds，只重采样 SDF influence band 或受影响 water grid nodes。
3. 建立 collider chunk -> water grid tile/node 索引，避免每次访问完整长方体 region。
4. 主线程按小预算 apply tile result；stale tile 直接丢弃，不回退 revision。

验收目标：单个 worker job latency 下降；持续编辑时 stale 比例可控；主线程 apply 仍保持亚毫秒级。

### Step 4：再评估 water sim 线程化

依据：线程化可以隐藏主线程帧尖峰，但不能降低总 CPU 成本；应在 Step 2 至少知道 water sim 热点后再做。

任务：

1. 先定义 command queue 和 render snapshot 边界，保持单线程行为不变。
2. behind flag 原型化 water worker：主线程发送 update/spawn/terrain revision 命令，渲染最多使用一帧旧 snapshot。
3. terrain collider/cache 消息全部带 revision，禁止旧消息覆盖新状态。
4. 增加 shutdown/backpressure/lag 日志。

验收目标：主线程 frame time 下降；water latency 可接受；退出无 hang；terrain/cache revision 不回退。

### 推荐执行顺序

1. `instrument terrain and water frame costs`
2. `parse perf logs into tables`
3. `make terrain sdf source refresh readback async`
4. `budget terrain source apply and deferred rebuilds`
5. `split water substep perf counters`
6. `optimize spacing relaxation or pressure projection based on data`
7. `tile/narrow water terrain cache worker jobs`
8. `prototype threaded water sim behind flag`

每步提交都应附一组 release benchmark 日志和简短结论；debug/unit test 只作为正确性补充，不作为性能证据。

## 后续 Roadmap

### P0：worker 化 Water Terrain Cache Rebuild（初版完成）

目标：把单个 `4-5ms/chunk` 的 water terrain cache rebuild 从主线程搬到 CPU worker，用其他 CPU 核心构建 cache 结果；主线程不再同步遍历并采样整个 cache region，只做轻量的 revision-guarded apply/swap。

原则：SDF collider 仍是独立产物，不把它包成 water 内部 pipeline。Water terrain cache 是 SDF collider 的 consumer，cache job 应该读取 immutable collider snapshot，生成 water-grid cache patch/result。

建议拆分提交：

1. `represent water terrain cache rebuild jobs`
   - 定义 worker 输入：chunk id、collider revision/source revision、water grid config、cache range、near-surface band。
   - 输入只包含 worker 可安全读取的 immutable 数据，例如 `Arc<WaterTerrainColliderSet>` 或相关 chunk snapshots。
   - 不让 worker 持有或修改 `PondWaterSim`。

2. `build water terrain cache regions on worker`
   - 新增 cache worker 线程或小 worker pool。
   - worker 执行 SDF/normal sampling，生成 `Vec<WaterTerrainGridSample>` 加 range metadata。
   - 支持多个 chunk 排队，但同一 chunk 只保留 latest revision。

3. `apply water terrain cache worker results`
   - 主线程 poll worker result。
   - 检查 chunk/revision 是否仍是 latest，过期结果直接丢弃。
   - 只把 result copy/swap 到 `water_sim.terrain_grid` 的对应 range。

4. `guard water terrain cache backpressure`
   - 限制 inflight jobs 数量，避免持续编辑时 cache worker 积压。
   - 对 active-water / camera-near chunk 保持优先级。
   - 日志记录 submit/build/apply/discard 耗时和 pending/inflight 数。

5. `benchmark worker water terrain cache rebuilds`
   - 对比 worker 前后的 release hidden edit soak。
   - 重点看主线程 frame time、cache rebuild apply time、terrain penetration、`terrain_shadow_false_skips`、stale discard 比例。

验收：water terrain cache rebuild 不再产生 `4-5ms/chunk` 主线程尖峰；主线程 apply 控制在小预算内；revision 不回退；`terrain_penetrating 0` 或只有已知可解释的有界值。

### P1：降低 Terrain SDF Source Refresh / terrain rebuild 主线程尖峰

目标：针对手动 visible perf 中暴露的 edit burst 主线程成本，把 Terrain SDF Source Refresh 的 GPU/readback/fence 路径异步化或严格预算化，同时继续收紧普通 terrain deferred rebuild 的每帧占用。

建议拆分提交：

1. `log per-frame terrain water queue costs`
   - 增加 per-frame 或 spike-only 汇总：deferred rebuild、source refresh submit/poll/apply、collider publish、cache apply、pending/inflight 数。
   - 现有 `[PERF] frame` 每 30 帧采样不足以严格归因；先补观测。

2. `make terrain sdf source refresh readback async`
   - 把当前同步 GPU atlas sample/readback/fence 等待拆成 submit + later poll。
   - 这是 GPU async readback job，不是普通 CPU worker；注意 Vulkan command pool / queue submission 的外部同步。
   - 主线程不在同帧等待 source refresh fence；ready 后再更新 `CpuSolidVoxelStore`。

3. `budget terrain sdf source apply`
   - 每帧只 publish 少量 ready source results。
   - 继续保持 latest-per-chunk、active-water/camera-near 优先级和 unchanged skip。
   - 记录 source job age、stale/drop 比例和追随延迟。

4. `smooth terrain deferred rebuild spikes`
   - 复查普通 terrain chunk rebuild 的每帧预算、chunk priority、和 edit loop 提交频率。
   - 目标是降低 `5-12ms/job` rebuild 对 visible frame 的直接影响。

验收：手动 edit burst 中 frame max / CPU-other max 明显下降；source/collider/cache revision 不回退；water terrain collision 诊断不退化。

### Phase 1：确认 stale SDF build 是否影响 UX

目标：判断旧 revision build 完成后直接丢弃，是否会让水体长时间等不到中间 collider。

建议拆分提交：

1. `log stale terrain sdf collider builds`
   - 记录每次 stale discard 的 chunk、revision、latest revision、build age。
   - 记录连续编辑期间 stale discard 占比。

2. `publish intermediate terrain sdf collider revisions`（仅在数据证明需要时做）
   - 若 completed result 虽不是 latest，但比当前 published revision 新，可先发布为中间态。
   - latest revision 继续排队追上。
   - 需要 revision guard，避免回退。

3. `prefer latest terrain sdf worker submissions`
   - 提交 worker 前再次确认 queued latest revision。
   - 尽量避免把明显过期的 work 送进 worker。

验收：水体在持续编辑中能周期性追上中间地形；没有 collider revision 回退。

### Phase 2：拆分 / 收窄 water terrain cache rebuild

目标：在 P0 worker 化之后，把单个 cache region job 继续拆小、收窄和索引化，降低 worker 延迟、结果 apply 成本和 stale work 浪费。

建议拆分提交：

1. `represent water terrain cache rebuild ranges`
   - 把 chunk cache rebuild 转成 range/tile work items。
   - 保留 revision 信息。

2. `process water terrain cache tiles incrementally`
   - worker 按 tile/range 构建 cache result。
   - 主线程每帧 apply 若干 tile 或最多 X ms。
   - 完成所有 tile 后标记该 chunk cache ready。

3. `shrink water terrain cache rebuild bounds`
   - 检查当前 halo 是否过大。
   - 尝试只重采样 SDF influence band 内节点。

4. `index water terrain cache tiles by collider chunk`
   - 建立 collider chunk -> water grid tile/node 索引。
   - 避免每次访问完整长方体 region。

验收：cache worker 单个 job 延迟下降；主线程 apply 不产生可见尖峰；持续编辑时 stale tile/result 可控。

### Phase 3：采样质量实验（非默认）

目标：只在 visible 验证发现漏碰撞时再做。

建议拆分提交：

1. `add terrain sdf sampling mode flag`
   - 增加隐藏/调试配置：center point、`2x2x2`、`4x4x4`、full average、OR。
   - 默认仍是 center point。

2. `benchmark terrain sdf sampling modes`
   - 比较 `gpu_sample_total`、readback、frame_dt、terrain penetration、视觉效果。

3. `select terrain sdf sampling default`（仅在证据充分时做）
   - 如果 multi-sample 明显更好且成本可接受，再改默认。

验收：没有证据前不改变默认采样路径。

### Phase 4：水模拟 kernel 微优化

目标：高粒子数下 water sim CPU 成本已超过 terrain edit 成本；先补齐 breakdown，再优化稳态水模拟。

建议拆分提交：

1. `refresh water kernel release baseline`
   - 更新空水体、有水体、edit soak、手动/脚本化高粒子数四类 release 基准。
   - 固定记录粒子数、`avg/substep`、frame CPU/other、terrain penetration。

2. `split water substep perf counters`
   - 把当前 `grid` 继续拆成 grid update / pressure projection。
   - 单独记录 `relax_incompressible_particle_spacing()`，验证高粒子数 residual 是否来自 spacing relaxation。
   - 记录每帧 `update_water_sim` wall time，而不只是一秒聚合值。

3. `bound particle spacing relaxation cost`
   - 如果数据确认 spacing relaxation 是主因，尝试按粒子数/活跃区域自适应降低 iterations 或分帧执行。
   - 保持稳定性和不可压视觉优先，不做无证据的默认降质。

4. `hoist mls mpm kernel weights`
   - 检查 3×3×3 kernel 的 weights、bounds、base coord 是否可复用。

5. `reduce grid gather index overhead`
   - 减少 G2P/P2G 中重复 index/bounds 计算。

6. `document water kernel perf delta`
   - 每个优化提交都记录前后 `avg/substep`、frame time 和主要 breakdown。

验收：release 下高粒子数 `avg/substep` 稳定下降；`terrain_shadow_false_skips` 保持 0；`terrain_penetrating` 不显著恶化。

### Phase 5：空 chunk collider（可选）

目标：消除 `terrain_no_sdf` 诊断噪音。

建议拆分提交：

1. `represent empty terrain sdf chunks`
   - 对全空 chunk 生成全 `+∞` 或明确 empty collider sentinel。

2. `skip expensive sdf build for empty chunks`
   - 空 chunk 不跑完整 SDF build。

验收：`no_sdf` 降低或消失，同时启动/编辑成本不明显上升。

### Phase 6：水模拟线程化

目标：评估并原型化把 `PondWaterSim::update()` 移到独立线程，减少主线程帧预算压力。

建议拆分提交：

1. `measure water sim threading boundary`
   - 记录主线程中 water update、render snapshot、terrain collider publish/cache rebuild 的耗时。
   - 明确哪些 `PondWaterSim` 访问必须留主线程，哪些可以搬到 worker。

2. `introduce water sim command queue`
   - 定义主线程 -> water 线程的命令：update、spawn particles、terrain collider upsert/remove、cache invalidate/rebuild、shutdown。
   - 先保持单线程执行，只改调用边界。

3. `add water sim render snapshots`
   - 定义 water 线程 -> 主线程的只读 snapshot。
   - 允许渲染最多使用一帧旧粒子数据。
   - snapshot 包含粒子、统计、诊断计数。

4. `prototype threaded water sim behind flag`
   - 增加隐藏开关启用 water worker。
   - 主线程发送 frame delta，worker 执行 update 并返回最新 snapshot。
   - 主线程不可阻塞等待 worker，最多复用上一帧 snapshot。

5. `move terrain collider sync onto water thread`
   - SDF collider publish 后通过命令发送给 water 线程。
   - terrain grid cache invalidate/rebuild 也在 water 线程内处理。
   - 所有 collider/cache 消息带 revision，避免旧消息覆盖新状态。

6. `harden water thread shutdown and backpressure`
   - 明确退出信号和 join 顺序。
   - 限制 command/snapshot 队列积压。
   - 日志输出 worker lag、dropped snapshots、latest applied terrain revision。

7. `benchmark threaded water simulation`
   - 对比 threaded on/off 的 release hidden 和 visible 结果。
   - 重点看主线程 frame time、water latency、terrain edit 后追随延迟。

验收：主线程帧时间下降；water 视觉延迟可接受；退出无 hang；terrain collider/cache revision 不回退。

### Phase 7：产品化和高风险路线

目标：前面阶段稳定后再做。

建议拆分提交：

1. `expose water performance profile in gui`
   - 把 `--water-profile performance` 暴露到 GUI/settings。

2. `parallelize water g2p`（高风险）
   - 按粒子分块并行，风险低于 P2G。

3. `parallelize water p2g`（更高风险）
   - 需要 thread-local grid/tile accumulation。

4. `prototype gpu water`（最高风险）
   - 只有 CPU 路线见底后再考虑。

## 验证策略

Benchmark 以 release hidden app run 为准；debug build 非性能证据。

常用命令：

```bash
cargo fmt --check
cargo check
cargo test -p re-flora-water
cargo test
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 3 --perf --water-profile performance'
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance --water-particles 2048'
zsh -lc 'source ~/.zshrc && cargo run --release -- --hidden --auto-exit 14 --perf --water-profile performance --water-particles 2048 --water-edit-soak'
zsh -lc 'source ~/.zshrc && cargo run --release -- --perf --water-profile performance --water-particles 2048'
cargo run --release -- --tail-latest-log 200
```

验收指标：

- `avg/substep` 下降且可复现。
- `terrain_shadow_false_skips 0`。
- `terrain_penetrating 0` 或只有已知可解释的有界值。
- `no_sdf` 可以非零。
- latest log 无 water / Vulkan / shader 错误。
