# Water MLS-MPM 性能 Roadmap

## 现状摘要

水体性能的第一轮热点清理已完成。水 box 已从固定 pond `(0,0,0)..(2,1,2)` 扩展到全世界 `(0,0,0)..(5,2,5)`（50 个 terrain chunks），grid 按比例放大到 `160×64×160` 保持每世界单位 32 cells 的密度。默认仍无水粒子，用户用浇水壶工具（按 4）在世界任意位置点击放水。

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
- 编辑路径 `~12 ms/edit` 在空 chunk 编辑时可能无 collider 旧数据，首次发布会构建新 collider（~7 ms）。
- 当前 grid 是单块 160x64x160 = 1.6M 节点。永远 dense allocated。sparse 只优化了 per-substep 循环，不减少内存。

## 后续 Roadmap

### Step 1：visible 观感验证

当前 priority。只看 hidden 日志不够。

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

### Step 5：评估编辑路径 hitch

当前编辑发布 ~12 ms/edit。如果 visible 验证发现编辑卡顿：

- 合并同一帧多个 dirty chunks
- collider build 分帧发布
- region cache rebuild 进一步缩小范围

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
