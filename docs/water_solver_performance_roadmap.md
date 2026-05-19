# Water Solver Performance Roadmap

本文档记录水模拟性能优化计划。2026-05-20 已从“不可压 MLS-MPM + marker spacing”路线转向“经典弱可压缩 MLS-MPM / EOS”路线；不可压 solver 保留为显式 opt-in / A/B 路径，不再作为默认或性能主线。

## 目标

- 优先降低高粒子数下的 CPU water sim 成本。
- 优先做可测量、可回退、物理行为风险低的优化。
- 默认和 performance profile 使用弱可压缩 EOS：`pressure_projection_iterations=0`、`particle_spacing_relaxation_iterations=0`，避免 grid pressure projection 和 marker spacing pass 的行为/性能成本。
- 暂不把“是否存储粒子速度”作为近期性能优化重点；该改动主要是状态表达方式变化，单独收益预计有限。

## 当前判断

当前水 solver 的主要热路径：

```text
repair_particles
clear_grid
particle_to_grid
update_grid
project_grid_incompressible   # only when pressure_projection_iterations > 0
grid_to_particle
relax_incompressible_particle_spacing  # only when incompressible projection is opt-in
record_diagnostic_substep
```

从 8192 / 100000 粒子基准和视觉检查看，当前判断是：

1. 不可压 marker spacing 路线已停止：exact density 太贵，cell-density 虽达到 `<10ms/substep`，但 settled pile 中仍有明显 grid pattern / clustering 抖动，不能作为默认主线。
2. 弱可压缩 EOS 删除 pressure projection 和 spacing pass 后，性能路径回到经典 `P2G -> grid update -> G2P`。后续优化优先测量 `P2G/G2P/terrain`，而不是继续调 marker spacing。
3. terrain cached projection stress 风险仍需复查：100000 粒子极限负载出现过 shadow false skip，需要作为后续 guard-band 风险项。
4. water sim 线程化只改善主线程响应性，不降低单核心总 CPU；当前不是第一优化项。

## 已完成简记（不作为当前优先级）

下面只保留已经完成、验证失败或明确不继续作为主线的目标；它们不再列入当前优先级列表。详细 benchmark 仍保留在后文 8192 / 100000 粒子基准中。

- 验证 `spacing=0`：性能更好，但 marker particles 会坍缩，不能作为默认。
- 用 compression-only density / PBF-like spacing 替换旧 pairwise 默认；旧 pairwise 只保留 fallback。
- 复用 density spacing scratch，并把 pair construction 改为 dense linked-cell bins。
- 加入 no-APIC / pure-PIC performance path。
- 优化 pressure projection 的 active-node stencil 和 buffer swap；pressure iterations 保持 `8`。
- 尝试 P2G/G2P stencil 缓存：实测回归，已放弃；只保留 P2G interior fast path 和 weight 简化。
- 减少 terrain exact SDF fallback，并保留 full shadow validation。
- 清理不可压路径中的 legacy EOS `j / stiffness / gamma` 热路径。
- 移除 G2P→P2G fusion 方向；不再列入 roadmap。
- 完成 8192 和 100000 粒子 release hidden 基准。
- P0 pass 1：为 density `spacing_relax` 增加内部计时，保留 moved-only repair、compact pair index 和直接 kernel math；回退了 gradient recompute pair 和 half-cell bins 两个回归方案。
- P0 pass 2：增加高粒子数 counting-sort contiguous density bins；低粒子数保留 linked-list bins，避免 rebuild overhead 伤害默认/8192 场景。
- P0 pass 3：为高粒子数 contiguous bins 增加 bin-ordered position scratch；8192 / 默认 linked path 保持直接读取粒子位置。
- P0 cell-density MVP：增加 opt-in `cell-density` spacing mode 和独立计时；100000 粒子稳定窗口 `spacing_relax` 降到约 `9.06ms/substep`，达到 P0 性能门槛，但 shadow false-skip stress 风险略升，暂不改默认。
- P0 cell-density 稳定性迭代：关闭 velocity feedback 并加入 rest-distance correction cap 后仍有明显 grid pattern / 同点 clustering；该方向不再作为默认水体路线继续。
- 路线切换：默认和 performance profile 回到经典弱可压缩 MLS-MPM / EOS，禁用 incompressible projection 和 marker spacing；performance profile 同时降到 `60Hz` water substeps 以优先控制 CPU；不可压路径仅保留为显式 flag A/B。

## 8192 粒子详细基准

2026-05-19 执行命令：

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 12 --perf --water-profile performance --water-particles 8192
python3 tools/parse_perf_log.py target/re-flora-logs/re-flora-20260519-152746.304-71234.log
```

运行日志：`target/re-flora-logs/re-flora-20260519-152746.304-71234.log`。本次有 10 个 `[PERF][WATER]` samples；前两个 sample 仍在铺展/沉降，下面的分项表使用最后 5 个稳定 samples。稳定窗口平均 `121.6 substeps/report`，`total=525.54ms/report`，`avg=4.322ms/substep`。

Top-level water solver 成本（非重复计数；`grid` 和 `g2p` 是 inclusive 总项）：

| part             | ms/report | ms/substep |   share |
| ---------------- | --------: | ---------: | ------: |
| `spacing_relax`  |  `266.74` |    `2.194` | `50.8%` |
| `g2p`            |  `189.69` |    `1.560` | `36.1%` |
| `grid`           |   `33.32` |    `0.274` |  `6.3%` |
| `p2g`            |   `30.56` |    `0.251` |  `5.8%` |
| `repair`         |    `4.15` |    `0.034` |  `0.8%` |
| `clear`          |    `1.07` |    `0.009` |  `0.2%` |
| `shadow_measure` |    `0.29` |    `0.002` |  `0.1%` |
| `residual`       |    `0.02` |    `0.000` | `<0.1%` |
| `diagnostics`    |    `0.00` |    `0.000` |  `0.0%` |

Nested breakdowns（子项不额外加到 total）：

| parent | part                    | ms/report | ms/substep | share of total |
| ------ | ----------------------- | --------: | ---------: | -------------: |
| `grid` | `pressure`              |   `26.99` |    `0.222` |         `5.1%` |
| `grid` | `grid_update`           |    `6.33` |    `0.052` |         `1.2%` |
| `g2p`  | `g2p_gather`            |   `29.06` |    `0.239` |         `5.5%` |
| `g2p`  | `g2p_terrain`           |   `24.95` |    `0.205` |         `4.7%` |
| `g2p`  | `g2p_repair`            |   `16.33` |    `0.134` |         `3.1%` |
| `g2p`  | `g2p_box`               |   `15.89` |    `0.131` |         `3.0%` |
| `g2p`  | uninstrumented G2P body |  `103.45` |    `0.851` |        `19.7%` |

Stability / workload counters in the same stable window:

| metric                              |        value |
| ----------------------------------- | -----------: |
| `density_pairs/substep`             |     `60,376` |
| `density_bins/substep`              |      `2,127` |
| `active_nodes/substep`              |      `6,380` |
| `terrain_cache_projections/substep` |      `5,517` |
| `terrain_cache_skips/substep`       |      `2,675` |
| `terrain_exact_checks/substep`      |          `0` |
| `terrain_exact_corrections/substep` |          `0` |
| `terrain_shadow_false_skips`        |          `0` |
| `terrain_shadow_sdf_err_avg`        |     `0.0011` |
| `terrain_shadow_sdf_err_max`        |      `0.031` |
| `penetrating`                       | `~23/report` |
| `no_sdf`                            |          `0` |

Frame-level hidden run summary: all 604 frame samples had `water_update mean=9.29ms`, `median=8.73ms`, `p95=13.35ms`, `max=20.71ms`; last 300 frame samples had `water_update mean=10.27ms`, `median=8.95ms`, `p95=13.49ms`, `max=13.85ms`, with frame mean `19.52ms`.

Findings:

1. 8192 粒子下成本已经转为 particle-local pass 主导。`spacing_relax` 单项约 `51%`，`g2p` 约 `36%`；`p2g`、`pressure` 各只有约 `5-6%`。
2. density spacing 的 pair 数随粒子密集沉降显著上升：首个 sample 约 `30k pairs/substep`，稳定后约 `60k pairs/substep`，对应 `spacing_relax` 从约 `1.57ms/substep` 升到约 `2.19ms/substep`。下一轮单核心 kernel 优化应优先看 density spacing 的算法和内存布局，而不是 P2G 或 pressure。
3. `g2p` 还有约 `0.85ms/substep` 未被当前子 timer 细分，占总成本约 `20%`。在动 G2P 逻辑前，应先加更细的 timer，把 position integration、terrain query setup、velocity feedback、repair dispatch 等拆出来。
4. terrain exact fallback 优化在 8192 粒子下仍有效：`terrain_exact_checks=0`、`terrain_exact_corrections=0`、`terrain_shadow_false_skips=0`。当前 `g2p_terrain` 约 `0.205ms/substep`，不是最大热点。
5. `active_nodes/substep` 稳定约 `6.4k`，与粒子数相比增长很小；grid pressure 已不是 8192 粒子主瓶颈。线程化 water sim 能降低主线程 hitch，但若目标是总 CPU 成本，热点主要还是 `spacing_relax` 和 G2P。

## 100000 粒子 hidden 极限基准

2026-05-19 执行命令：

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 12 --perf --water-profile performance --water-particles 100000
python3 tools/parse_perf_log.py target/re-flora-logs/re-flora-20260519-153946.589-81880.log
```

运行日志：`target/re-flora-logs/re-flora-20260519-153946.589-81880.log`。本次有 8 个 `[PERF][WATER]` samples；前两个 sample 处于早期铺展/沉降且 substep/report 不稳定，下面的分项表使用最后 5 个 samples。该场景已经无法实时运行：稳定窗口每个 perf report 只有 `16 substeps`，约两帧；每帧跑满 `MAX_SUBSTEPS_PER_UPDATE=8`，最后 10 帧 `water_update mean=631.1ms`，总帧时间 `633.9ms`，约 `1.5-1.6 fps`。hidden 模式仍保持 audio engine 运行并静音输出，因此 CPU 过载期间出现大量 `Ring buffer underrun` 警告（本次日志 841 条）。渲染 debug snapshot 被上限截断到约 `16k`，但 solver 日志确认 `particles=100000`、`finite=100000`。

稳定窗口平均 `total=1251.06ms/report`，`avg=78.191ms/substep`。

Top-level water solver 成本（非重复计数；`grid` 和 `g2p` 是 inclusive 总项）：

| part             | ms/report | ms/substep |   share |
| ---------------- | --------: | ---------: | ------: |
| `spacing_relax`  |  `900.93` |   `56.308` | `72.0%` |
| `g2p`            |  `294.93` |   `18.433` | `23.6%` |
| `p2g`            |   `41.30` |    `2.581` |  `3.3%` |
| `grid`           |    `6.93` |    `0.433` |  `0.6%` |
| `repair`         |    `6.49` |    `0.406` |  `0.5%` |
| `shadow_measure` |    `3.23` |    `0.202` |  `0.3%` |
| `clear`          |    `0.47` |    `0.029` | `<0.1%` |
| `residual`       |    `0.00` |    `0.000` |  `0.0%` |
| `diagnostics`    |    `0.00` |    `0.000` |  `0.0%` |

Nested breakdowns（子项不额外加到 total）：

| parent | part                    | ms/report | ms/substep | share of total |
| ------ | ----------------------- | --------: | ---------: | -------------: |
| `grid` | `pressure`              |    `5.21` |    `0.326` |         `0.4%` |
| `grid` | `grid_update`           |    `1.73` |    `0.108` |         `0.1%` |
| `g2p`  | `g2p_gather`            |   `44.78` |    `2.799` |         `3.6%` |
| `g2p`  | `g2p_terrain`           |   `36.48` |    `2.280` |         `2.9%` |
| `g2p`  | `g2p_repair`            |   `25.95` |    `1.622` |         `2.1%` |
| `g2p`  | `g2p_box`               |   `25.06` |    `1.566` |         `2.0%` |
| `g2p`  | uninstrumented G2P body |  `162.66` |   `10.166` |        `13.0%` |

Stability / workload counters in the same stable window:

| metric                              |                                                 value |
| ----------------------------------- | ----------------------------------------------------: |
| `density_pairs/substep`             |                                           `1,580,720` |
| `density_bins/substep`              |                                              `15,701` |
| `active_nodes/substep`              |                                               `8,738` |
| `terrain_cache_projections/substep` |                                              `49,242` |
| `terrain_cache_skips/substep`       |                                              `50,758` |
| `terrain_exact_checks/substep`      |                                                   `0` |
| `terrain_exact_corrections/substep` |                                                   `0` |
| `terrain_shadow_samples/substep`    |                                               `6,250` |
| `terrain_shadow_false_skips`        | `0.2/report`（5 个稳定 samples 中有 1 个 false skip） |
| `terrain_shadow_sdf_err_avg`        |                                              `0.0013` |
| `terrain_shadow_sdf_err_max`        |                                               `0.027` |
| `penetrating`                       |                                         `~338/report` |
| `no_sdf`                            |                                                   `0` |

Findings:

1. 100000 粒子下 `spacing_relax` 已经是压倒性瓶颈，约 `56.3ms/substep`、`72%` 总成本；density pair 数稳定在约 `1.58M pairs/substep`。当前单线程 density spacing 不适合 100k 粒子量级。
2. `g2p` 仍是第二热点，约 `18.4ms/substep`、`23.6%`；其中未细分 G2P body 约 `10.2ms/substep`。若继续做 kernel 优化，应先给 G2P 增加更细 timer，再决定是否拆/并行。
3. `p2g` 只有约 `2.58ms/substep`、`3.3%`，`pressure` 只有约 `0.33ms/substep`、`0.4%`。在 100k 场景下 grid-side 优化几乎不是主线。
4. terrain exact fallback 仍完全没有触发（`exact_checks=0`），但 shadow validation 在极限负载下出现过一次 false skip；如果要正式支持 100k 粒子，需要复查 cached terrain projection guard 或把 shadow false-skip 作为 stress-only 风险记录。
5. 主线程已经被 water solver 长时间阻塞，audio pump 被饿死并持续 underrun。线程化 water sim 可以保护主线程/音频/渲染响应性，但不会减少总 CPU。当前用户目标仍是单核心优化，因此下一阶段先集中优化 density `spacing_relax` 本身；worker 线程化后移。

## 当前优先级计划

当前列表保留当前优先级、已完成的 P0 实验结果和后续未完成目标；已完成摘要也同步记录在“已完成简记”中。

### P0：用 grid/cell-density push 替代 exact particle-pair spacing

目的：寻找数量级优化机会。标准 MLS-MPM / 不可压 MPM 的主流体路径是 `P2G -> grid pressure projection -> G2P`；当前 `relax_incompressible_particle_spacing` 只是额外的 marker anti-clump / particle redistribution 补丁，不应比主 MPM 路径更贵。100000 粒子最新稳定窗口中 exact density spacing 仍约 `44ms/substep`、其中 pair accumulation 约 `33ms/substep`，而 grid pressure projection 只有约 `0.3ms/substep`。继续优化 exact pair loop 只能争取常数因子；P0 改为在 flag 后面实现 `O(particles + occupied_cells)` 的 grid/cell-density push，并用 release hidden perf 判断是否能把 spacing 成本打到个位数 ms/substep。

P0 实施计划：

1. 增加一个非默认 spacing mode，例如 `WaterParticleSpacingMode::CellDensity`，CLI 暂定 `--water-spacing-mode cell-density`；保留现有 `density` / `pairwise` fallback，未验证前不改默认。
2. 第一版 MVP 复用或扩展现有 density bin scratch：每次 spacing 只统计 `cell_count`、`cell_centroid/sum_pos`、可选 `cell_excess`，不构建 particle-pair list，不做 per-pair sqrt/gradient。
3. 对每个粒子只基于所在 cell 和少量邻近 cell 计算 correction：
   - 只处理 compression / overfull cell，不对低密度 cell 产生吸引，避免自由表面被拉扯。
   - 初版可以从 overfull cell centroid 向外推；若 blocky artifact 明显，再升级为 6/26 邻居 cell density gradient。
   - correction 继续使用现有 `max_correction` clamp、box/terrain repair 和 velocity feedback。
4. 增加独立计时和 counter：`cell_density_rebuild`、`cell_density_push`、`cell_density_occupied_cells`、`cell_density_overfull_cells`、`cell_density_max_excess`；保留旧 `density_pairs/substep` 作为 exact mode 指标，新 mode 下应为 0 或明确不适用。
5. A/B 验证 8192 和 100000：同一命令只切换 spacing mode，对比 `spacing_relax ms/substep`、总 `avg/substep`、marker clump、穿地、边界 pinning、`terrain_shadow_false_skips`。
6. 第一阶段目标不是物理完全等价，而是证明数量级潜力：100000 粒子下 `spacing_relax < 10ms/substep` 作为保留门槛，理想目标 `< 5ms/substep`；若视觉明显退化，再迭代 cell size / target occupancy / strength / cadence。

风险和边界：

- 这是行为变化，不是 kernel 等价优化；必须放在新 mode 或明确 feature flag 后面。
- grid/cell-density push 只负责 marker 分布维护，不替代 grid pressure projection 的不可压约束。
- 如果 cell-density push 无法防 clump，再评估 reseeding / sim-render particle decoupling；不要回到长期维护 exact pairwise spacing 作为 100000 粒子主线。

历史：exact density spacing kernel 优化已完成三轮，下面保留 benchmark 记录；后续不再把 no-list double traversal / pair loop micro-opt 作为 P0 主线。

2026-05-19 pass 1 完成：

- 增加 `spacing_relax` 内部 breakdown timer：`spacing_bin_rebuild / spacing_pair_accum / spacing_lambda / spacing_corr_accum / spacing_corr_apply / spacing_post_repair / spacing_velocity`。
- 增加轻量 counter：`density_active_lambdas/substep`、`density_moved/substep`。逐 candidate pair 计数实测会污染 hot loop，未保留。
- spacing 后只对实际 moved particles 做 box / terrain / repair；当前 8192/100000 稳定窗口几乎所有粒子都会移动，所以收益很小，但语义更精确。
- `DensitySpacingPair` 的 particle index 改为 `u32`，并预计算 `inv_support_radius` / gradient scale，在 pair loop 中直接计算 kernel weight / gradient，减少除法和 helper 边界。
- 尝试但未保留：只存 `(i, j)` 并在 correction pass 重算 gradient（100000 粒子下 `spacing_relax` 明显回归到约 `1.08s/report`）；半 cell size + range-2 neighbor bins（occupied bins 约 `75k`，pair traversal 回归）。

最终保留版本 benchmark（release hidden，performance profile，稳定窗口取最后 5 个 `[PERF][WATER]` samples）：

| particles | log                                                           | avg/substep | spacing_relax/report | spacing_relax/substep | pair_accum/substep | density_pairs/substep | density_bins/substep | density_moved/substep |
| --------: | ------------------------------------------------------------- | ----------: | -------------------: | --------------------: | -----------------: | --------------------: | -------------------: | --------------------: |
|    `8192` | `target/re-flora-logs/re-flora-20260519-172018.174-62851.log` |   `4.235ms` |           `254.30ms` |             `2.095ms` |          `1.461ms` |              `60,420` |              `2,130` |               `8,127` |
|  `100000` | `target/re-flora-logs/re-flora-20260519-171943.689-62029.log` |  `75.896ms` |           `862.01ms` |            `53.875ms` |         `43.407ms` |           `1,580,500` |             `15,706` |              `99,360` |

对比旧稳定窗口：8192 粒子 `spacing_relax` 约 `266.74ms/report -> 254.30ms/report`（约 `4.7%` faster），100000 粒子约 `900.93ms/report -> 862.01ms/report`（约 `4.3%` faster）。主要剩余瓶颈仍是 pair accumulation。

2026-05-19 pass 2 完成：

- 为高粒子数 density bins 增加 counting-sort contiguous layout：先 count / prefix sum，再把 particle indices 填进连续 `u32` index buffer，pair traversal 走 contiguous ranges。
- 8192 / 默认小粒子数继续使用 linked-list bins；contiguous-only 8192 实测 pair traversal 略快但 bin rebuild 约翻倍，净收益不足。
- 当前阈值：`DENSITY_SPACING_CONTIGUOUS_BIN_MIN_PARTICLES = 50_000`。

最终保留版本 benchmark（release hidden，performance profile，稳定窗口取最后 5 个 `[PERF][WATER]` samples）：

| particles | log                                                           | bin layout | avg/substep | spacing_relax/report | spacing_relax/substep | pair_accum/substep | bin_rebuild/substep | density_pairs/substep | density_bins/substep |
| --------: | ------------------------------------------------------------- | ---------- | ----------: | -------------------: | --------------------: | -----------------: | ------------------: | --------------------: | -------------------: |
|    `8192` | `target/re-flora-logs/re-flora-20260519-174605.683-88055.log` | linked     |   `4.266ms` |           `257.88ms` |             `2.121ms` |          `1.491ms` |           `0.053ms` |              `60,430` |              `2,129` |
|  `100000` | `target/re-flora-logs/re-flora-20260519-174644.710-88950.log` | contiguous |  `74.629ms` |           `830.75ms` |            `51.922ms` |         `40.648ms` |           `0.956ms` |           `1,636,146` |             `15,261` |

对比 pass 1 的 100000 粒子稳定窗口：`spacing_relax` 约 `862.01ms/report -> 830.75ms/report`（约 `3.6%` faster），`spacing_pair_accum` 约 `43.41ms/substep -> 40.65ms/substep`（约 `6.4%` faster）。8192 场景保留 linked path；最新样本与 pass 1 在小幅噪声范围内，但没有采用 contiguous-only 小粒子路径。

2026-05-19 pass 3 完成：

- 为高粒子数 contiguous density bins 增加 bin-ordered position scratch：填充 contiguous particle index buffer 时同步写入该 bin 顺序下的 `Vec3` position，pair traversal 直接读取相邻 position slices。
- 8192 / 默认小粒子数继续使用 linked-list bins，并保持直接读取 `WaterParticle.x`；先前尝试的全局 per-particle position scratch 对 8192 没有稳定收益，未保留为小粒子路径。
- 该优化减少 contiguous pair traversal 的随机 `WaterParticle` 读取；solver 行为保持同一组 particle index pairs 和同一 density/correction 公式。

最终保留版本 benchmark（release hidden，performance profile，稳定窗口取最后 5 个 `[PERF][WATER]` samples）：

| particles | log                                                           | bin layout             | avg/substep | spacing_relax/report | spacing_relax/substep | pair_accum/substep | bin_rebuild/substep | density_pairs/substep | density_bins/substep |
| --------: | ------------------------------------------------------------- | ---------------------- | ----------: | -------------------: | --------------------: | -----------------: | ------------------: | --------------------: | -------------------: |
|    `8192` | `target/re-flora-logs/re-flora-20260519-182540.271-17362.log` | linked                 |   `4.110ms` |           `241.67ms` |             `1.971ms` |          `1.343ms` |           `0.051ms` |              `60,407` |              `2,130` |
|  `100000` | `target/re-flora-logs/re-flora-20260519-182658.010-18344.log` | contiguous + positions |  `63.997ms` |           `697.51ms` |            `43.594ms` |         `33.086ms` |           `0.966ms` |           `1,615,841` |             `15,351` |

对比 pass 2 的 100000 粒子稳定窗口：`spacing_relax` 约 `830.75ms/report -> 697.51ms/report`（约 `16.0%` faster），`spacing_pair_accum` 约 `40.65ms/substep -> 33.09ms/substep`（约 `18.6%` faster），`avg/substep` 约 `74.63ms -> 64.00ms`（约 `14.2%` faster）。100000 粒子下 `spacing_pair_accum` 仍是最大单项，约 `33ms/substep`。

exact spacing 结论：当前三轮优化已经证明 exact particle-pair density spacing 可以做常数因子改善，但仍随 `density_pairs/substep` 线性增长。继续做 no-list double traversal、pair list 压缩、sqrt 近似等只适合作为后备小优化，不再符合“数量级优化”目标。

2026-05-20 cell-density MVP 完成：

- 增加 `WaterParticleSpacingMode::CellDensity`，CLI 使用 `--water-spacing-mode cell-density`；默认和 performance profile 仍保持 `density`，新 mode 只做 opt-in A/B。
- 第一版使用 dense cell count / centroid / generation-stamped occupied-cell scratch，不构建 particle-pair list；每个粒子只对所在 overfull cell 做 compression-only centroid-outward push。
- 增加 `cell_density_rebuild`、`cell_density_push`、`cell_density_occupied_cells/substep`、`cell_density_overfull_cells/substep`、`cell_density_moved/substep`、`cell_density_max_excess` 日志和 parser 字段。
- cell-density spacing 的 terrain repair 使用 cached terrain projection，并对向 terrain normal 内推的 correction 加 conservative SDF guard；这保留了性能门槛，同时避免 cached-only 版本在 100000 粒子下显著增加 penetrating。

最终保留版本 benchmark（release hidden，performance profile，稳定窗口取最后 5 个 `[PERF][WATER]` samples）：

| particles | mode           | log                                                           | avg/substep | spacing_relax/substep | rebuild/substep | push/substep | post_repair/substep | pair_accum/substep | density_pairs/substep | cell_occupied/substep | cell_moved/substep | penetrating | shadow false skips/report |
| --------: | -------------- | ------------------------------------------------------------- | ----------: | --------------------: | --------------: | -----------: | ------------------: | -----------------: | --------------------: | --------------------: | -----------------: | ----------: | ------------------------: |
|    `8192` | `density`      | `target/re-flora-logs/re-flora-20260520-005503.207-13070.log` |   `6.270ms` |             `3.167ms` |       `0.196ms` |          `-` |           `0.908ms` |          `1.785ms` |              `60,694` |                   `0` |                `0` |        `25` |                       `0` |
|    `8192` | `cell-density` | `target/re-flora-logs/re-flora-20260520-010134.952-16676.log` |   `3.871ms` |             `0.817ms` |       `0.226ms` |    `0.157ms` |           `0.390ms` |          `0.000ms` |                   `0` |               `3,097` |            `6,619` |        `10` |                       `0` |
|  `100000` | `density`      | `target/re-flora-logs/re-flora-20260520-005549.313-13532.log` |  `96.606ms` |            `65.755ms` |       `3.604ms` |          `-` |          `10.145ms` |         `45.853ms` |           `1,449,140` |                   `0` |                `0` |       `337` |                     `0.2` |
|  `100000` | `cell-density` | `target/re-flora-logs/re-flora-20260520-010111.496-16292.log` |  `39.409ms` |             `9.061ms` |       `2.347ms` |    `1.852ms` |           `4.343ms` |          `0.000ms` |                   `0` |              `13,345` |           `79,822` |       `495` |                     `2.4` |

结论：cell-density MVP 删除了 exact pair list / per-pair kernel，100000 粒子下 `spacing_relax` 达到 P0 `<10ms/substep` 性能门槛，并把总 `avg/substep` 从本次 exact density 基线约 `96.6ms` 降到约 `39.4ms`。8192 粒子也从约 `3.17ms/substep` 的 spacing 降到约 `0.82ms/substep`。行为方面，8192 粒子 terrain penetrating 没有变差；100000 stress 下 penetrating 与 shadow false-skip 比 exact density 略高但仍比 cached-only 试验安全，暂作为 opt-in mode 保留，不默认替换 `density`。后续若要把它设为默认，需要先做视觉检查并复查 P2 terrain cache guard-band / false-skip 风险。

2026-05-20 stability pass：用户观察到 cell-density mode 下静止堆积水粒子有来回抖动。原因判断为 hard cell occupancy + centroid push 在 cell 边界不连续，且原先把 spacing correction 以 `0.15` blend 回灌进速度，容易把 marker regularization 变成持续 impulse。第一轮调整为更保守的 marker-shift：cell size 从 `1.0x` rest distance 放大到 `1.25x`，push strength 从 `0.35` 降到 `0.20`，near-threshold excess 使用 `excess / (target + 1)` 软化，cell-density 独立 velocity blend 降到 `0.04`。用户反馈明显改善但仍有可见抖动后，第二轮进一步把 push strength 降到 `0.12`，按 rest distance 把每 substep correction 限到 `0.06x rest_distance`，并关闭 cell-density velocity feedback。release hidden 复测：8192 粒子 `spacing_relax` 约 `0.796ms/substep`、diag speed_avg 约 `0.124`、penetrating 约 `0.8/report`；100000 粒子 `spacing_relax` 约 `9.253ms/substep`，仍满足 P0 `<10ms/substep` 门槛。后续尝试 27-cell smoothed centroid 后视觉仍无实质改善，已回退该试验，并决定放弃不可压 marker spacing 作为默认路线。

2026-05-20 weak EOS pivot：默认和 performance profile 切回经典弱可压缩 MLS-MPM / EOS，`pressure_projection_iterations=0`、`particle_spacing_relaxation_iterations=0`；performance profile 的 water substep 从 `120Hz` 降到 `60Hz`。release hidden 初测：8192 粒子 log `target/re-flora-logs/re-flora-20260520-015349.951-38759.log`，`avg_substep≈4.41ms`（sample mean）、`pressure=0`、`spacing_relax=0`；100000 粒子 log `target/re-flora-logs/re-flora-20260520-015402.156-39166.log`，`avg_substep≈38.1ms`、`pressure=0`、`spacing_relax=0`，主要成本回到 `G2P/P2G/terrain`。100000 粒子仍不能实时，但已经不再受不可压 projection 或 marker spacing 控制。

P0 验收：

- 新 mode 至少跑 `8192` 和 `100000` 粒子 release hidden perf，对比旧 exact `density` mode 的 `spacing_relax ms/substep`、`avg/substep`、frame `water_update`。
- 100000 粒子第一保留门槛：`spacing_relax < 10ms/substep`；若不能显著低于 exact mode，则不作为 P0 主线继续。
- 8192 粒子不能出现明显 marker clump、穿地增加、边界 pinning 或速度异常；必要时允许 cell-density 只在高粒子数/指定 mode 启用。
- `terrain_exact_checks` 保持 0 或解释变化；`terrain_shadow_false_skips` 不应因 spacing 改动系统性增加。
- 文档必须记录：cell-density push 是 marker maintenance，不是主不可压约束；主不可压仍由 grid pressure projection 负责。

预期收益：如果 cell-density push 能删除 pair list 和 per-pair kernel，spacing 部分有机会从约 `44ms/substep` 降到个位数 ms/substep；总 CPU 的数量级目标仍可能需要后续结合 G2P 优化、sim/render particle decoupling 或降低 water sim cadence。

### P1：补充 G2P 细分计时

目的：100000 粒子下 `g2p` 仍约 `17-18ms/substep`，是 `spacing_relax` 之后的第二热点；其中未细分 G2P body 仍约 `10ms/substep`。先拆 timer，再决定是否优化 position integration、velocity feedback、terrain query setup 或 repair dispatch。

任务：

1. 增加 G2P 内部 breakdown，不改变 solver 行为。
2. 拆出至少以下阶段：position integration / velocity update、terrain query setup、APIC/PIC update、repair dispatch、其他未归类 body。
3. 8192 和 100000 粒子各跑 release hidden perf，记录各阶段 ms/substep。

验收：计时 overhead 低；能解释当前 uninstrumented G2P body 的主要来源。

### P2：复查 terrain cached projection stress false-skip

目的：100000 粒子极限负载下 shadow validation 出现过 1 次 false skip。该问题不是当前最大性能瓶颈，但在正式支持 100000 粒子前需要确认是否为偶发 stress-only 风险。

任务：

1. 保持 `terrain_exact_checks=0` 的前提下复查 cached projection guard band。
2. 用 100000 粒子 release hidden perf 重跑，记录 `terrain_shadow_false_skips`、`terrain_shadow_sdf_err_max`、`penetrating`。
3. 只有在 false skip 可重复或穿地增加时再调整 guard。

验收：false skip 不系统性出现，或有明确 guard 调整和性能影响记录。

### P3：water sim 线程化（后移）

任务：

1. 定义主线程 command queue 和 render snapshot 边界。
2. water worker 独立推进 solver，主线程最多渲染一帧旧 snapshot。
3. terrain collider/cache 消息全部带 revision，禁止旧消息覆盖新状态。
4. 增加 shutdown、backpressure、lag 日志。

验收：主线程 frame time 降低；water 延迟可接受；退出无 hang；terrain/cache revision 不回退。

预期收益：对主线程 hitch 高，对总 CPU 成本无本质改善。由于当前目标是单核心数量级优化，线程化放在 cell-density / algorithmic spacing 替换之后。

## 推荐执行顺序

不可压 P0 prototype 和 8192 / 100000 A/B benchmark 已完成但视觉失败。下一步建议：

1. `keep weak EOS as default/performance profile`
2. `benchmark P2G/G2P/terrain on weak EOS before new solver work`
3. `tune stiffness/gamma/j_min/damping only with release hidden logs and visual review`
4. `recheck terrain cache guard-band under 100000-particle weak EOS stress`
5. `prototype threaded water sim behind flag only after single-core CPU work`

不要继续把 exact pairwise / density / cell-density marker spacing 当作默认路线；它们只作为不可压 opt-in A/B baseline。当前 roadmap 优先保留经典弱可压缩 MLS-MPM 的简单稳定行为和性能。

## 每步验证要求

性能相关结论以 release-mode app run 为准。推荐验证梯度：

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance
cargo run --release -- --latest-log
python3 tools/parse_perf_log.py
```

重点记录：

- water avg ms/substep
- `repair / clear / p2g / grid_update / pressure / g2p / spacing_relax / diagnostics / residual`
- frame CPU/other spike
- particle count
- active nodes/substep
- terrain contact / terrain penetrating
- exact SDF checks/corrections
- visual或日志中是否出现异常 clump、穿地、边界 pinning、速度饱和
