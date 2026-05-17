# Water MLS-MPM 性能 Roadmap

## 现状摘要

当前水体性能工作的第一轮热点清理已经完成：主要的重复 terrain collision、全网格维护、过宽 collider 刷新、编辑时全量 terrain cache rebuild、以及诊断 observer effect 都已经处理。现在的重点不再是“明显重复工作”，而是基于 release 日志判断剩余的 G2P / P2G / terrain fallback / 编辑发布成本是否值得继续深挖。

关键结论：

- `REPORT.md` 里的老基线约为 `4.9 ms/substep`。
- 经过 Phase 1-4 后，默认 4096 粒子路径降到约 `1.75-1.85 ms/substep`。
- 历史 `--water-profile performance` 在 2048 粒子、`32^3` grid、120 Hz 下曾达到约 `0.88-1.02 ms/substep`。
- 当前固定 pond 扩到 `(0,0,0)..(2,1,2)` 后，grid 是 `64x32x64`；最新显式水体 edit soak 约 `1.28-1.53 ms/substep`，随 active node 增长而上升。
- 默认启动现在没有水粒子；无水时 `PondWaterSim::update()` 会提前返回，不再做空 substep。
- 当前 `--water-profile performance --water-particles 2048 --water-edit-soak` 验证干净：`terrain_shadow_false_skips 0`、`no_sdf 0`，tight-contact 下只接受有界的 sub-cell coarse SDF overlap。

最新代表性日志：

- 当前显式水体基准：`/tmp/re-flora-logs/re-flora-20260517-113134.969-13425.log`
  - 命令：`--water-profile performance --water-particles 2048`
  - 结果：`1.21-1.43 ms/substep`，`terrain_shadow_false_skips 0`，`no_sdf 0`。
- 当前 Phase 6 后 edit soak：`/home/terence/code/re-flora/target/re-flora-logs/re-flora-20260517-124000.876-32047.log`
  - 命令：`--water-profile performance --water-particles 2048 --water-edit-soak`
  - 结果：steady samples 约 `1.28-1.53 ms/substep`。
  - 编辑路径：source refresh `0.750-0.985 ms/edit`，collider build `7.10-7.25 ms/edit`，region terrain cache rebuild `4.20-4.31 ms/edit`，总发布工作约 `12 ms/edit`。
  - shadow 校验：`terrain_shadow_samples/substep 0.1`，`terrain_shadow_false_skips 0`，`no_sdf 0`。

## 已完成工作的时间线

### 2026-05-16：先砍热路径重复 terrain 工作

- Phase 1：terrain query 先采 SDF，只在碰撞时算 normal；普通查询走直接 chunk lookup。
- Phase 2：移除 steady-state `repair_particles()` 中重复的 terrain sweep，只在 G2P 后做 terrain correction；terrain 变化仍调用 `stabilize_after_terrain_change()`。
- Phase 3A：增加 water-grid terrain cache，`update_grid()` 不再对每个活跃 grid node 做 live collider query。
- Phase 3B：增加 G2P cached terrain broadphase：远离地形直接 skip，明确碰撞时 cached projection，模糊区域才 exact fallback；同时加入 shadow verification counters。
- Phase 4：缩小 startup/edit collider scope，只刷新当前 water grid domain 相关的 terrain chunks。
- Phase 5：加入 benchmark/tuning knobs：`--water-particles`、`--water-grid`、`--water-substep-hz`、`--water-terrain-margin-cells`、`--water-damping`、`--water-profile performance`、`--water-edit-soak`。

### 2026-05-17：围绕当前 `64x32x64` pond 做实现级清理

- 6A：无水粒子时跳过 `PondWaterSim::update()` 固定 substeps，清理 stale perf/diagnostic state。
- 6B：P2G 记录 touched grid nodes，`clear_grid()` 和 `update_grid()` 改为 sparse active-node 维护。
- 6C：startup collider chunks 从 18 个收紧到严格重叠的 4 个；startup terrain cache rebuild 批处理；编辑时只 rebuild 受影响 chunk 对应的 water-grid region + halo；terrain-change stabilization 也缩到编辑区域附近。
- 6D：`WaterTerrainColliderChunk::sample_sdf_and_normal_ws()` 复用同一个 8-corner SDF cell 计算 SDF+gradient；`WaterTerrainColliderSet` 边界查询只检查有限相邻 chunks。
- 6E：`PondWaterSim::log_diagnostics_after_update()` 不再每帧 exact terrain sweep；shadow exact-SDF 校验移出 `grid_to_particle_impl()`，改为每个 perf report 采样。

相关提交：

- `cd1cf8ec` skip empty water updates
- `55d85e60` make water grid maintenance sparse
- `dc1965b2` tighten water collider chunk scope
- `de4c89e6` batch startup water collider cache rebuilds
- `12496ed4` remove empty water terrain colliders
- `871582a3` discard stale water collider results
- `2f65b4f4` log water terrain cache rebuilds
- `163a58e5` rebuild water terrain cache regions
- `5c42f252` scope water terrain stabilization
- `b63ff369` reuse terrain sdf samples for normals
- `22ce2932` limit water boundary collider lookups
- `e5d8e523` throttle water diagnostic terrain scans
- `f8c193eb` move water shadow checks out of g2p

## 当前代码状态

核心文件：

- `crates/re-flora-water/src/mls_mpm.rs`
  - `PondWaterSim::update()`：空水体提前返回。
  - `particle_to_grid()` / `clear_grid()` / `update_grid()`：active-node sparse grid 维护。
  - `grid_to_particle_impl()`：G2P cached terrain path 和 exact fallback。
  - `terrain_grid_particle_query()`：粒子级 cached SDF 查询。
  - `rebuild_terrain_grid_cache()` / region rebuild：terrain cache 全量和局部刷新。
  - `log_diagnostics_after_update()`：诊断 exact terrain 扫描节流。
- `crates/re-flora-water/src/pond.rs`
  - water config/profile、grid/cache 数据结构、perf counters。
- `crates/re-flora-water/src/collider.rs`
  - `WaterTerrainColliderChunk::sample_sdf_and_normal_ws()`：复用 SDF samples 算 normal。
  - `WaterTerrainColliderSet`：直接 chunk lookup 和有限边界候选。
- `src/app/core/water.rs`
  - startup/edit terrain collider refresh、strict water-domain filtering、worker result revision 检查、edit soak。

当前风险/注意点：

- `--water-profile performance` 是低 CPU 候选，不一定已经适合作为默认；仍需要 visible soak 判断水面贴地、弹性、阻尼和编辑观感。
- tight-contact profile 不再要求 coarse collider `penetrating 0`，而是要求 `terrain_shadow_false_skips 0`、`no_sdf 0`、`terrain_sdf_min` 只出现有界 sub-cell overlap。
- `24^3` / `16^3` grid 在历史 sweep 中不是 win：grid 成本下降，但 cached terrain 分类变差，exact fallback/correction 变多。
- 当前 collider chunk SDF build 仍约 `7 ms/chunk`；编辑总发布工作约 `12 ms/edit`。这不是 steady-state 热点，但可能是编辑手感热点。
- 当前 water domain 仍是固定 pond box。支持任意大水体前，需要 active water-domain/chunk set，而不是扩大成全世界 collider rebuild。

## 后续 Roadmap

### Step 1：建立新的 release 基准表

目标：在 Phase 6 清理后重新确认真实剩余热点，避免继续按旧数据优化。

执行：

1. 跑三类 release hidden benchmark：
   - 空水体：`--water-profile performance`
   - 显式水体：`--water-profile performance --water-particles 2048`
   - 编辑 soak：`--water-profile performance --water-particles 2048 --water-edit-soak`
2. 从最新日志提取这些字段：
   - `avg/substep`
   - `p2g`
   - `grid`
   - `g2p`
   - `g2p_gather`
   - `g2p_terrain`
   - `active_nodes/substep`
   - `terrain_exact_checks/substep`
   - `terrain_exact_fallbacks/substep`
   - `terrain_cache_skips/substep`
   - `terrain_cache_projections/substep`
   - `terrain_shadow_false_skips`
   - `terrain_sdf_min`
   - edit path 的 `[WATER][TERRAIN_CACHE]` 和 `[WATER][TERRAIN] built collider`。
3. 把最新基准写回本文件，替换上面的“最新代表性日志”。

完成标准：

- 工作区干净。
- hidden release runs 成功退出。
- 新基准能明确指出下一个最大项是 `g2p_gather`、`g2p_terrain`、`p2g`、`grid`，还是 edit publication。

### Step 2：做 visible/手动观感验证

目标：确认当前 performance profile 的视觉表现，而不是只看 hidden 日志。

执行：

1. 跑 visible app：
   - `cargo run --release -- --perf --water-profile performance --water-particles 2048`
2. 观察：
   - 水是否贴地过深或离地有缝。
   - 水是否过弹、过黏、或明显抖动。
   - terrain 编辑后水是否及时稳定。
3. 如果需要手动 terrain 编辑，再请用户手动操作；否则继续用 `--water-edit-soak` 做自动回归。

完成标准：

- 如果视觉 OK：进入 Step 3/4 做性能优化。
- 如果视觉不 OK：先调 `terrain margin`、grid-node contact band、damping、substep Hz；每次调参都跑 hidden edit soak。

### Step 3：只在有证据时继续调 cached terrain fallback

目标：降低 `g2p_terrain` / exact fallback，但不能牺牲 terrain contact 安全。

候选方向：

1. 小步调整 cached SDF slack / projection threshold。
2. 评估是否为 terrain grid 存更稳定的 gradient/normal，减少 ambiguous fallback。
3. 如果 fallback 仍高，考虑 per-cell classification cache：`empty` / `cached-projectable` / `exact-required`。
4. 保留 shadow verification，并在 perf report 中继续抽样 exact SDF。

禁止条件：

- 只要出现 `terrain_shadow_false_skips > 0`，立即回退。
- `no_sdf` 不能变成非零。
- tight-contact 下 `terrain_sdf_min` 只能是小范围 sub-cell overlap，不能持续加深。

完成标准：

- `g2p_terrain` 或 `terrain_exact_checks/substep` 有稳定下降。
- `avg/substep` 有 release 可复现改善。
- hidden edit soak 和 visible 观感都不退化。

### Step 4：优化剩余 P2G/G2P gather CPU 成本

目标：如果 Step 1 显示 `g2p_gather` 或 `p2g` 已经超过 terrain fallback，转向粒子-grid 核心循环。

候选方向：

1. 在 release 日志里先确认 P2G/G2P 占比和粒子数线性关系。
2. 检查 3x3x3 kernel 中是否有可 hoist 的重复计算：cell base、weights、bounds、node index stride。
3. 评估是否能复用 P2G/G2P 的权重计算，或把 per-axis weights/gradients 缓存在 stack 小数组中。
4. 保持数据结构简单，先做单线程微优化；不要过早引入并行 scatter。

完成标准：

- 2048 粒子 performance profile 的 `p2g` 或 `g2p_gather` 有可重复下降。
- 单元测试和 hidden release soak 通过。
- 粒子行为不变或只出现可解释的浮点级差异。

### Step 5：评估编辑路径 hitch 是否需要继续优化

目标：当前 steady-state 已经不是 edit publication，但编辑手感可能仍受 `~12 ms/edit` 影响。

候选方向：

1. 先做 visible/manual 编辑验证，确认是否真的有可感知 hitch。
2. 如果有：合并同一帧多个 terrain dirty chunks，避免重复 publish/rebuild。
3. 如果 collider build `~7 ms/chunk` 是主因：考虑降低 SDF build 工作、分帧发布、或后台构建完成后做更平滑的 terrain cache swap。
4. 如果 region cache rebuild `~4 ms/edit` 是主因：继续缩小 affected water-grid region 或延迟低优先级远端 nodes。

完成标准：

- 只有在 visible 编辑确实卡顿时才实现。
- edit soak 继续只刷新 water-domain chunks。
- stale worker result 仍必须丢弃，不能发布旧 revision。

### Step 6：从固定 pond box 走向 active water domain

目标：支持任意大水体/玩家放水，同时保持 collider 和 grid 工作局部化。

设计方向：

1. 用水粒子 bounds / active chunks 维护 active water-domain set。
2. terrain collider 按 active water chunks lazy build / priority build，不做全世界扫描。
3. water grid 需要随 active domain 扩展或分块；不要简单把单个 dense grid 扩到全世界。
4. 明确 collider chunk 生命周期：需要、构建中、已发布、过期、可回收。
5. debug watering-can 放水工具应从“固定 pond box 内”升级为“激活/扩展 water domain”。

完成标准：

- 固定 pond 行为不退化。
- 新水域只构建相关 terrain collider chunks。
- terrain edit 只影响相交 active water chunks。

### Step 7：决定 profile 产品化

目标：把当前实验参数变成可维护的用户/开发者选项。

执行：

1. 如果 visible soak 通过，考虑把 `--water-profile performance` 暴露到 GUI/settings。
2. 保留 `default` 质量路径和 no-startup-water 行为。
3. 文档化 benchmark 命令：默认无水，测水必须传 `--water-particles <N>`。

完成标准：

- 开发者能稳定复现实验。
- 用户不会误以为默认无水 benchmark 代表水体成本。

### Step 8：最后再考虑高风险并行/GPU路线

目标：只有在单线程、cache、domain scope 都明确到位后，再做结构性改动。

候选方向：

1. CPU 并行 G2P：风险较低，可按粒子分块。
2. CPU 并行 P2G：需要 thread-local grid/tile accumulation 或其他避免原子竞争的策略，风险高于 G2P。
3. GPU/storage-buffer water：只有在 CPU 路线收益不足且渲染/同步成本可控时考虑。
4. adaptive CFL/substep：这是稳定性/手感改动，不是纯性能改动，必须 visible 验证。

完成标准：

- 有 release 数据证明 CPU 单线程继续优化收益不足。
- 有清晰 correctness/validation 方案。
- 每个大改动单独提交、单独 benchmark。

## 验证策略

性能判断以 release hidden app run 为准；debug build 和 unit tests 不能当性能证据。

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

每次优化提交前至少满足：

- `cargo fmt --check`
- `cargo check`
- 相关测试：通常是 `cargo test`，水 crate 改动额外跑 `cargo test -p re-flora-water`
- 至少一个 release hidden perf run
- 如果涉及 terrain edit/collider/cache：跑 `--water-edit-soak`

验收指标：

- `avg/substep` 下降，且不是日志/采样偶然值。
- `terrain_shadow_false_skips 0`。
- `no_sdf 0`。
- conservative profile 尽量保持 `penetrating 0`。
- tight-contact profile 只接受有界 sub-cell `terrain_sdf_min` overlap。
- latest log 无 water / Vulkan / shader 错误。
