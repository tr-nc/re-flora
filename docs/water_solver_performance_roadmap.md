# Water Solver Performance Roadmap

本文档记录 2026-05-19 讨论后的水模拟性能优化计划。它聚焦 `crates/re-flora-water` 里的不可压 MLS-MPM solver 本身，是当前唯一保留的水体性能 roadmap。

## 目标

- 优先降低高粒子数下的 CPU water sim 成本。
- 优先做可测量、可回退、物理行为风险低的优化。
- 保持当前不可压路线：主不可压约束仍由 MPM grid pressure projection 解决；不再把当前 pairwise spacing relaxation 当作长期方案，若需要粒子位置约束，优先用 PBF-like density projection 替代。
- 暂不把“是否存储粒子速度”作为近期性能优化重点；该改动主要是状态表达方式变化，单独收益预计有限。

## 当前判断

当前水 solver 的主要热路径：

```text
repair_particles
clear_grid
particle_to_grid
update_grid
project_grid_incompressible
grid_to_particle
relax_incompressible_particle_spacing
record_diagnostic_substep
```

从 8192 / 100000 粒子基准看，当前未完成的主要性能嫌疑点是：

1. density `spacing_relax`：dense linked-cell traversal 已替代旧 HashMap 查询，但 100000 粒子下仍有约 `1.58M pairs/substep`，pair list 带宽、cache locality、pair kernel math 和 post-spacing repair 是下一轮单核心优化重点。
2. G2P 未细分成本：100000 粒子下约 `10.2ms/substep` 的 G2P body 尚未拆开计时；在动 G2P 逻辑前应先补 timer。
3. terrain cached projection stress 风险：100000 粒子极限负载出现过 1 次 shadow false skip，需要作为后续 guard-band 风险复查项。
4. water sim 线程化只改善主线程响应性，不降低单核心总 CPU；当前不是主线。

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

## 8192 粒子详细基准

2026-05-19 执行命令：

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 12 --perf --water-profile performance --water-particles 8192
python tools/parse_perf_log.py target/re-flora-logs/re-flora-20260519-152746.304-71234.log
```

运行日志：`target/re-flora-logs/re-flora-20260519-152746.304-71234.log`。本次有 10 个 `[PERF][WATER]` samples；前两个 sample 仍在铺展/沉降，下面的分项表使用最后 5 个稳定 samples。稳定窗口平均 `121.6 substeps/report`，`total=525.54ms/report`，`avg=4.322ms/substep`。

Top-level water solver 成本（非重复计数；`grid` 和 `g2p` 是 inclusive 总项）：

| part | ms/report | ms/substep | share |
|---|---:|---:|---:|
| `spacing_relax` | `266.74` | `2.194` | `50.8%` |
| `g2p` | `189.69` | `1.560` | `36.1%` |
| `grid` | `33.32` | `0.274` | `6.3%` |
| `p2g` | `30.56` | `0.251` | `5.8%` |
| `repair` | `4.15` | `0.034` | `0.8%` |
| `clear` | `1.07` | `0.009` | `0.2%` |
| `shadow_measure` | `0.29` | `0.002` | `0.1%` |
| `residual` | `0.02` | `0.000` | `<0.1%` |
| `diagnostics` | `0.00` | `0.000` | `0.0%` |

Nested breakdowns（子项不额外加到 total）：

| parent | part | ms/report | ms/substep | share of total |
|---|---|---:|---:|---:|
| `grid` | `pressure` | `26.99` | `0.222` | `5.1%` |
| `grid` | `grid_update` | `6.33` | `0.052` | `1.2%` |
| `g2p` | `g2p_gather` | `29.06` | `0.239` | `5.5%` |
| `g2p` | `g2p_terrain` | `24.95` | `0.205` | `4.7%` |
| `g2p` | `g2p_repair` | `16.33` | `0.134` | `3.1%` |
| `g2p` | `g2p_box` | `15.89` | `0.131` | `3.0%` |
| `g2p` | uninstrumented G2P body | `103.45` | `0.851` | `19.7%` |

Stability / workload counters in the same stable window:

| metric | value |
|---|---:|
| `density_pairs/substep` | `60,376` |
| `density_bins/substep` | `2,127` |
| `active_nodes/substep` | `6,380` |
| `terrain_cache_projections/substep` | `5,517` |
| `terrain_cache_skips/substep` | `2,675` |
| `terrain_exact_checks/substep` | `0` |
| `terrain_exact_corrections/substep` | `0` |
| `terrain_shadow_false_skips` | `0` |
| `terrain_shadow_sdf_err_avg` | `0.0011` |
| `terrain_shadow_sdf_err_max` | `0.031` |
| `penetrating` | `~23/report` |
| `no_sdf` | `0` |

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
python tools/parse_perf_log.py target/re-flora-logs/re-flora-20260519-153946.589-81880.log
```

运行日志：`target/re-flora-logs/re-flora-20260519-153946.589-81880.log`。本次有 8 个 `[PERF][WATER]` samples；前两个 sample 处于早期铺展/沉降且 substep/report 不稳定，下面的分项表使用最后 5 个 samples。该场景已经无法实时运行：稳定窗口每个 perf report 只有 `16 substeps`，约两帧；每帧跑满 `MAX_SUBSTEPS_PER_UPDATE=8`，最后 10 帧 `water_update mean=631.1ms`，总帧时间 `633.9ms`，约 `1.5-1.6 fps`。hidden 模式仍保持 audio engine 运行并静音输出，因此 CPU 过载期间出现大量 `Ring buffer underrun` 警告（本次日志 841 条）。渲染 debug snapshot 被上限截断到约 `16k`，但 solver 日志确认 `particles=100000`、`finite=100000`。

稳定窗口平均 `total=1251.06ms/report`，`avg=78.191ms/substep`。

Top-level water solver 成本（非重复计数；`grid` 和 `g2p` 是 inclusive 总项）：

| part | ms/report | ms/substep | share |
|---|---:|---:|---:|
| `spacing_relax` | `900.93` | `56.308` | `72.0%` |
| `g2p` | `294.93` | `18.433` | `23.6%` |
| `p2g` | `41.30` | `2.581` | `3.3%` |
| `grid` | `6.93` | `0.433` | `0.6%` |
| `repair` | `6.49` | `0.406` | `0.5%` |
| `shadow_measure` | `3.23` | `0.202` | `0.3%` |
| `clear` | `0.47` | `0.029` | `<0.1%` |
| `residual` | `0.00` | `0.000` | `0.0%` |
| `diagnostics` | `0.00` | `0.000` | `0.0%` |

Nested breakdowns（子项不额外加到 total）：

| parent | part | ms/report | ms/substep | share of total |
|---|---|---:|---:|---:|
| `grid` | `pressure` | `5.21` | `0.326` | `0.4%` |
| `grid` | `grid_update` | `1.73` | `0.108` | `0.1%` |
| `g2p` | `g2p_gather` | `44.78` | `2.799` | `3.6%` |
| `g2p` | `g2p_terrain` | `36.48` | `2.280` | `2.9%` |
| `g2p` | `g2p_repair` | `25.95` | `1.622` | `2.1%` |
| `g2p` | `g2p_box` | `25.06` | `1.566` | `2.0%` |
| `g2p` | uninstrumented G2P body | `162.66` | `10.166` | `13.0%` |

Stability / workload counters in the same stable window:

| metric | value |
|---|---:|
| `density_pairs/substep` | `1,580,720` |
| `density_bins/substep` | `15,701` |
| `active_nodes/substep` | `8,738` |
| `terrain_cache_projections/substep` | `49,242` |
| `terrain_cache_skips/substep` | `50,758` |
| `terrain_exact_checks/substep` | `0` |
| `terrain_exact_corrections/substep` | `0` |
| `terrain_shadow_samples/substep` | `6,250` |
| `terrain_shadow_false_skips` | `0.2/report`（5 个稳定 samples 中有 1 个 false skip） |
| `terrain_shadow_sdf_err_avg` | `0.0013` |
| `terrain_shadow_sdf_err_max` | `0.027` |
| `penetrating` | `~338/report` |
| `no_sdf` | `0` |

Findings:

1. 100000 粒子下 `spacing_relax` 已经是压倒性瓶颈，约 `56.3ms/substep`、`72%` 总成本；density pair 数稳定在约 `1.58M pairs/substep`。当前单线程 density spacing 不适合 100k 粒子量级。
2. `g2p` 仍是第二热点，约 `18.4ms/substep`、`23.6%`；其中未细分 G2P body 约 `10.2ms/substep`。若继续做 kernel 优化，应先给 G2P 增加更细 timer，再决定是否拆/并行。
3. `p2g` 只有约 `2.58ms/substep`、`3.3%`，`pressure` 只有约 `0.33ms/substep`、`0.4%`。在 100k 场景下 grid-side 优化几乎不是主线。
4. terrain exact fallback 仍完全没有触发（`exact_checks=0`），但 shadow validation 在极限负载下出现过一次 false skip；如果要正式支持 100k 粒子，需要复查 cached terrain projection guard 或把 shadow false-skip 作为 stress-only 风险记录。
5. 主线程已经被 water solver 长时间阻塞，audio pump 被饿死并持续 underrun。线程化 water sim 可以保护主线程/音频/渲染响应性，但不会减少总 CPU。当前用户目标仍是单核心优化，因此下一阶段先集中优化 density `spacing_relax` 本身；worker 线程化后移。

## 当前优先级计划

当前列表只包含未完成目标；已完成内容只在“已完成简记”中保留。

### P0：单核心 density `spacing_relax` 优化

目的：100000 粒子稳定窗口中 `spacing_relax` 约 `56.3ms/substep`、`72%` 总成本，且成本与 `density_pairs/substep` 基本线性。当前 dense linked-cell pair traversal 已消除了 HashMap 主瓶颈，下一轮应针对单核心的内存带宽、pair list、cache locality 和 post-spacing repair 做可测量优化。不要用多线程掩盖该成本。

任务按低风险到高风险执行，每一步单独 benchmark，保留 measured win，回退持平或退化方案：

1. 增加 `spacing_relax` 内部 breakdown timer / counter：
   - dense bin rebuild
   - pair accumulation
   - lambda solve
   - correction accumulation
   - correction apply
   - post-spacing box / terrain / repair
   - active lambdas
   - moved particles
   - candidate pairs vs accepted pairs（如实现成本低）
2. 只 repair 被 spacing correction 实际移动的粒子：
   - correction apply 时记录 moved indices 或 moved bitset。
   - spacing 后的 box / terrain / repair 只处理 moved 粒子。
   - 未移动粒子语义上保持不变；这是首个低风险优化候选。
3. 降低 pair list 带宽：
   - 先尝试 compact pair layout，用 `u32` 存粒子 index（100000 粒子足够），减少 `usize` 带宽。
   - 再尝试只存 `(i, j)`，correction pass 重算 `grad_i`；用更多算术换更少内存读写。
   - 再尝试无 pair list 的双遍 neighbor traversal：第一遍累计 density / gradient，第二遍重走 pairs 并直接累计 correction。该方案风险较高，因为会增加 pair traversal / sqrt 数量，只在 benchmark 支持时保留。
4. 改善 dense bin locality：
   - 用 counting-sort style bins 替代 linked-list `head/next`：先 count，每个 bin prefix sum，再把 particle indices 填入连续数组。
   - 每个 occupied bin 变成 contiguous range，减少 pointer chasing，提高 pair loop 的 cache locality 和 branch predictability。
   - 与 compact pair variants 分开测试，避免一次改动太大。
5. 为 spacing 建 compact position scratch：
   - pair loop 只需要位置，避免反复读取完整 `WaterParticle` struct。
   - 可先用 `Vec<Vec3>`，必要时再评估 SoA（`x/y/z` 分离）。
   - 只在 release hidden 8192 / 100000 两档都不退化时保留。
6. 简化 pair kernel math：
   - 预计算 `inv_support_radius`、gradient coefficient。
   - 在 pair loop 中直接计算 `q`、`q*q`、weight 和 gradient，减少 helper 调用边界和重复除法。
   - 保持零距离 fallback、finite guard、correction cap 不变。
7. 如果 exact density spacing 单核心仍无法接近目标，再单独评估近似方案：
   - cap 每粒子最大 neighbor / pair 数。
   - spacing 每 N 个 substep 执行一次。
   - grid-density / cell-density push 替代 exact pairwise density projection。
   - 这些都属于行为变化，必须配窗口观察、日志指标和文档说明，不能悄悄改默认。

验收：

- 每个候选都至少跑 `8192` 和 `100000` 粒子 release hidden perf，对比 `spacing_relax ms/substep`、`density_pairs/substep`、`density_bins/substep`、`avg/substep`、frame `water_update`。
- 8192 粒子不能出现 marker clump、穿地增加、边界 pinning 或速度异常。
- 100000 粒子重点看单核心总 CPU 是否下降；不要求立即实时，但必须明确记录收益比例。
- `terrain_exact_checks` 保持 0 或解释变化；`terrain_shadow_false_skips` 不应因 spacing 改动系统性增加。

预期收益：exact density spacing 的单核心优化预计主要是常数因子改善，合理目标是 `1.5x-3x` 的 `spacing_relax` 降幅；若要让 100000 粒子真正实时，可能仍需要近似 spacing 算法，但先完成上述可逆 kernel 优化。

### P1：补充 G2P 细分计时

目的：100000 粒子下 `g2p` 约 `18.4ms/substep`、`23.6%` 总成本，其中未细分 G2P body 约 `10.2ms/substep`。先拆 timer，再决定是否优化 position integration、velocity feedback、terrain query setup 或 repair dispatch。

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

预期收益：对主线程 hitch 高，对总 CPU 成本无本质改善。由于当前目标是单核心优化，线程化放在 density `spacing_relax` kernel 优化之后。

## 推荐执行顺序

当前只列未完成目标；已完成目标不再重复列入本列表。

1. `add spacing_relax internal timers and counters`
2. `repair only moved spacing particles`
3. `benchmark compact / recomputed / no-list density pair variants`
4. `prototype contiguous counting-sort density bins`
5. `prototype compact position scratch for spacing`
6. `add finer G2P timers after spacing_relax wins are exhausted or blocked`
7. `recheck terrain false-skip if 100000-particle support remains a goal`
8. `prototype threaded water sim behind flag only after single-core CPU work`

不要默认恢复 `spacing=0`，也不要把 performance profile 的 pressure 默认降到 `8` 以下；旧 pairwise spacing 只作为 fallback。当前 roadmap 优先单核心降低 `spacing_relax` 总 CPU，不把 water sim 线程化作为下一步主线。

## 每步验证要求

性能相关结论以 release-mode app run 为准。推荐验证梯度：

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance
cargo run --release -- --latest-log
python tools/parse_perf_log.py
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
