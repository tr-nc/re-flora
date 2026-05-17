# Water MLS-MPM 性能 Roadmap

## 当前状态

- 默认水体已切到不可压 pressure projection。
- 水域覆盖全世界 `(0,0,0)..(5,2,5)`，grid 为 `160×64×160`。
- terrain edit 后的 SDF/source/cache 链路已分帧、去重、限流。
- 手动反馈：当前编辑性能已经明显改善。
- 下一步重点：补数据、清命名、继续拆 cache、降低单帧尖峰。

## 已完成工作（极简）

- 2026-05-16：水模拟核心热点已从约 `4.9ms/substep` 降到约 `1-1.85ms/substep`。
- 2026-05-17：水体从固定小 pond 扩展到全世界，并保持 grid 密度。
- 2026-05-17：默认水体切到不可压 pressure projection，稳定性改善。
- 2026-05-17：确认 terrain edit hitch 来自水专用 SDF/source/cache 链路。
- 2026-05-17：source refresh、SDF build、water cache rebuild 已进入 chunk queue。
- 2026-05-17：已加入 coalesce、unchanged skip、block-center sample、active-water priority、cache budget、catch-up cap。

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
    -> 主线程按预算重建 cache region
```

注意：第一步 terrain voxel 修改目前仍是同步 GPU compute + fence；后续 queue 从普通 terrain chunk rebuild 开始。

当前这些 queue 都复用 `LatestChunkQueue<T>`：

- 普通 terrain chunk rebuild：`LatestChunkQueue<ChunkRebuildRequest>`。
- Terrain SDF source refresh：`LatestChunkQueue<WaterTerrainSourceRefreshRequest>`。
- Terrain SDF collider build：`LatestChunkQueue<WaterTerrainColliderRebuildRequest>`，额外用 inflight guard 限制全局 1 个 worker job。
- Water terrain cache rebuild：`LatestChunkQueue<WaterTerrainCacheRebuildRequest>`，当前仍在主线程执行。

## 当前关键事实

- SDF source 当前是 `32^3` point downsample。
- 当前采样点是 block-center：常见 `256 -> 32` 时取 `4,12,...,252`。
- 暂不默认做 full average reduction；它会把每 chunk 读取量从 `32^3` 提到 `32^3 * 512`。
- occupancy unchanged skip 检测的是降采样后的 `32^3` solid bitset。
- unchanged skip 成本很低，通常远小于一次 SDF build 或 cache rebuild。
- 空 chunk 不生成 SDF collider，因此 `terrain_no_sdf` 可以非零；这不是 bug。
- 当前 cache region rebuild 仍可能约 `40k` grid nodes、`4-5ms/chunk`。

## 后续 Roadmap

### Phase 1：补正式数据和诊断

目标：用 release 数据确认当前“体感不错”是否稳定。

建议拆分提交：

1. `add terrain sdf queue diagnostics`
   - 记录 source/cache queue 的 pending、active、ready、processed。
   - 记录 stale discard、unchanged skip、low-priority delay 次数。
   - 记录每帧 source/cache budget 实际耗时。

2. `summarize water edit soak metrics`
   - 跑 release `--water-edit-soak`。
   - 提取 `frame_dt p95/max`、`ran_substeps`、source refresh、SDF build、cache rebuild。
   - 把数据写回本文件。

3. `document water collider latency`
   - 统计 dirty -> source refresh -> SDF publish -> cache ready 的延迟。
   - 区分 active-water chunk 和 low-priority chunk。

验收：连续编辑时没有明显 `ran_substeps=8` 追帧尖峰；`frame_dt p95` 不被水 terrain 更新拉爆。

### Phase 2：命名解耦和语义整理

目标：把 SDF/source/cache 三层命名讲清楚，减少 water-specific 命名污染。

建议拆分提交：

1. `rename water terrain source refresh queue`
   - `deferred_water_terrain_source_refreshes` -> `deferred_terrain_sdf_source_refreshes`。
   - `WaterTerrainSourceRefreshRequest` -> `TerrainSdfSourceRefreshRequest`。
   - 日志标签同步改为 `TERRAIN_SDF_SOURCE`。

2. `rename water terrain collider build queue`
   - `deferred_water_terrain_collider_rebuilds` -> `deferred_terrain_sdf_collider_rebuilds`。
   - `WaterTerrainColliderRebuildRequest` -> `TerrainSdfColliderRebuildRequest`。
   - 保持行为不变。

3. `rename dirty scheduling entrypoints`
   - `mark_water_terrain_source_chunk_dirty` -> `schedule_terrain_sdf_source_refresh`。
   - `mark_water_terrain_source_chunk_dirty_immediate` -> `schedule_terrain_sdf_source_refresh_immediate`。
   - `mark_water_terrain_source_chunk_dirty_after` -> `schedule_terrain_sdf_source_refresh_after`。
   - 保留 `schedule`，因为这里还会做 domain check、priority、delay、coalesce。

验收：纯 rename，不改变 benchmark；`rg water_terrain_source` 只剩兼容/注释中合理出现。

### Phase 3：确认 stale SDF build 是否影响 UX

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

### Phase 4：从 count budget 升级到 time budget

目标：让不同机器上每帧水 terrain 工作的开销更可控。

建议拆分提交：

1. `time budget terrain sdf source refreshes`
   - source refresh 从“每帧最多 1 个”改为“每帧最多 X ms，同时最多 N 个”。
   - 保留 active-water priority。

2. `time budget water terrain cache rebuilds`
   - cache rebuild 从“每帧最多 1 个”改为“每帧最多 X ms”。
   - 如果单个 cache rebuild 超过 X ms，先记录，再进入 Phase 5 拆分。

3. `expose terrain sdf budget diagnostics`
   - 日志输出预算命中次数、超时次数、剩余 pending。

验收：单帧 source/cache 工作不会在常规机器上超过预设预算太多。

### Phase 5：拆分 water terrain cache rebuild

目标：把单个 `4-5ms` cache region rebuild 拆成更小的可调度任务。

建议拆分提交：

1. `represent water terrain cache rebuild ranges`
   - 把 chunk cache rebuild 转成 range/tile work items。
   - 保留 revision 信息。

2. `process water terrain cache tiles incrementally`
   - 每帧处理若干 tile 或最多 X ms。
   - 完成所有 tile 后标记该 chunk cache ready。

3. `shrink water terrain cache rebuild bounds`
   - 检查当前 halo 是否过大。
   - 尝试只重采样 SDF influence band 内节点。

4. `index water terrain cache tiles by collider chunk`
   - 建立 collider chunk -> water grid tile/node 索引。
   - 避免每次访问完整长方体 region。

5. `worker water terrain cache rebuilds`（可选）
   - 如果主线程 tile 化后仍有尖峰，再 worker 化。
   - 主线程只做 revision-guarded swap。

验收：cache rebuild 不再产生单个 `4-5ms` 主线程尖峰。

### Phase 6：采样质量实验（非默认）

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

### Phase 7：水模拟 kernel 微优化

目标：如果 edit 链路不再是瓶颈，再优化稳态水模拟。

建议拆分提交：

1. `refresh water kernel release baseline`
   - 更新空水体、有水体、edit soak 三类 release 基准。

2. `hoist mls mpm kernel weights`
   - 检查 3×3×3 kernel 的 weights、bounds、base coord 是否可复用。

3. `reduce grid gather index overhead`
   - 减少 G2P/P2G 中重复 index/bounds 计算。

4. `document water kernel perf delta`
   - 每个优化提交都记录前后 `avg/substep` 和主要 breakdown。

验收：release 下 `avg/substep` 稳定下降；`terrain_shadow_false_skips` 保持 0。

### Phase 8：空 chunk collider（可选）

目标：消除 `terrain_no_sdf` 诊断噪音。

建议拆分提交：

1. `represent empty terrain sdf chunks`
   - 对全空 chunk 生成全 `+∞` 或明确 empty collider sentinel。

2. `skip expensive sdf build for empty chunks`
   - 空 chunk 不跑完整 SDF build。

验收：`no_sdf` 降低或消失，同时启动/编辑成本不明显上升。

### Phase 9：产品化和高风险路线

目标：前面阶段稳定后再做。

建议拆分提交：

1. `expose water performance profile in gui`
   - 把 `--water-profile performance` 暴露到 GUI/settings。

2. `evaluate threaded water simulation`
   - 评估把 `PondWaterSim::update()` 移到独立线程。
   - 重点处理 terrain collider 同步、退出信号、数据延迟。

3. `parallelize water g2p`（高风险）
   - 按粒子分块并行，风险低于 P2G。

4. `parallelize water p2g`（更高风险）
   - 需要 thread-local grid/tile accumulation。

5. `prototype gpu water`（最高风险）
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
