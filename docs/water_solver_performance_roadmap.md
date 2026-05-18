# Water Solver Performance Roadmap

本文档记录 2026-05-19 讨论后的水模拟性能优化计划。它聚焦 `crates/re-flora-water` 里的不可压 MLS-MPM solver 本身；terrain-water cache / SDF source / visible terrain rebuild 的历史计划仍见 [`water_mls_mpm_performance_roadmap.md`](water_mls_mpm_performance_roadmap.md)。

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

从代码结构看，近期最值得验证的性能嫌疑点是：

1. 当前 `relax_incompressible_particle_spacing()` 是 ad-hoc 粒子防聚团 pass：每个 substep 建 HashMap、分配 correction buffer、做邻域 pair 查询，并在 spacing 后重复碰撞/repair；它还会阻断 G2P 后直接准备 next P2G 的流水/融合优化。优先验证移除或替换，而不是继续优化这个实现。
2. incompressible APIC affine：即使 `j` 已经在不可压路径中固定为 `1.0`，`c: Mat3` 仍参与 P2G/G2P 的 27-node 热循环。P2 已加入 no-APIC path，performance profile 默认使用 `incompressible_apic_blend=0.0`。
3. pressure projection：固定 Jacobi iteration，且每步 pressure 从 0 开始。
4. P2G/G2P 重复计算 particle stencil：base coord、weights、node indices 在同一 substep 中重复计算。
5. terrain exact fallback：贴地粒子可能持续触发精确 SDF 查询。

## 性能优先级计划

### P0：移除或替换当前 spacing relaxation

目的：当前 `relax_incompressible_particle_spacing()` 不是经典 MLS-MPM 不可压步骤，而是额外的 pairwise 防聚团补丁。它成本高、物理含义弱，并且会阻断后续 `G2P -> next P2G` 融合/流水。优先把它从 performance path 中移除；如果视觉粒子分布不可接受，再用更 principled 的 PBF-like density projection 替代。

2026-05-19 初次基准（release hidden，`--water-profile performance --water-particles 1024`，每组约 5 个 `[PERF][WATER]` samples）：

| spacing iterations | avg/substep | spacing_relax / 120 substeps | 结论                                         |
| -----------------: | ----------: | ---------------------------: | -------------------------------------------- |
|                  0 |    `1.26ms` |                    `~0.01ms` | 不做 spacing 的 baseline                     |
|                  1 |    `2.23ms` |                   `~72.32ms` | spacing 单次 pass 约 `0.60ms/substep`        |
|                  2 |    `2.84ms` |                  `~136.42ms` | 第二次 iteration 继续增加约 `0.53ms/substep` |

结论：spacing 是高粒子数性能大头之一。直接复用 scratch buffer / sort-based binning 的初步尝试未带来收益，反而变慢；继续优化当前 pairwise spacing 不再作为优先路线。

2026-05-19 P0 执行结果（no-APIC performance profile，1024 粒子）：`spacing=0` hidden run 明显快于 `spacing=1`（8s run mean `1.16ms/substep` vs `2.28ms/substep`；16s stability run mean `0.89ms/substep`），窗口观察视觉完全正常。performance profile 已改为默认 `spacing=0`，旧 spacing 仅保留为 CLI/debug fallback（`--water-spacing-iterations <N>`）。

P0 任务：

1. 用当前 no-APIC performance profile 跑 `--water-spacing-iterations 0` 的 release hidden 和窗口观察，对比当前临时默认 `spacing=1`。
2. 如果视觉表现可接受：把 performance profile 默认改为 `spacing=0`，并将旧 `relax_incompressible_particle_spacing()` 降级为 debug/legacy 开关，后续考虑删除。
3. 如果去掉 spacing 后出现不可接受的粒子聚团：用 PBF-like、compression-only density projection 替代当前 pairwise min-distance spacing：
   - 在 G2P 得到 predicted position 后做位置约束。
   - 约束目标是局部密度/压缩，例如 `C_i = max(rho_i / rho0 - 1, 0)`，而不是简单“粒子太近就互推”。
   - 只防压缩，不强行把自由表面/稀疏区域拉满，避免和 grid pressure projection 做双重不可压求解。
   - correction 后重新做 box / terrain / repair，并用 `(x_corrected - x_previous) / dt` 回写速度。
   - 继续保留 correction cap 和有限性检查。
4. 记录 `spacing_relax`、water avg/substep、terrain penetration、粒子聚团/边界 pinning、速度饱和和视觉表现。

建议验证命令：

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 6 --perf --water-profile performance --water-particles 1024 --water-spacing-iterations 0
cargo run --release -- --hidden --auto-exit 6 --perf --water-profile performance --water-particles 1024 --water-spacing-iterations 1
cargo run --release -- --windowed --perf --water-profile performance --water-particles 1024 --water-spacing-iterations 0
cargo run --release -- --latest-log
python tools/parse_perf_log.py
```

验收：performance profile 可以在 `spacing=0` 下保持可接受视觉和稳定日志；若必须保留位置约束，则 PBF-like density projection 比旧 spacing 更正统且成本不高于旧 spacing。

预期收益：高。直接移除 spacing 可省掉当前最大的非 pressure 热点，并为后续 `G2P -> next P2G` 融合创造条件。

### P1（取消）：优化当前 pairwise spacing 实现

原计划中的 scratch buffer 复用、spacing 降频、局部执行等只是在优化一个 ad-hoc pass。由于初步 scratch / sort-based 尝试没有收益，并且当前方向改为“移除或 PBF-like 替代”，该 P1 不再作为计划执行。只有在短期必须保留旧 spacing 作为 fallback 时，才做最小维护。

### P2：加入 pure PIC / no-APIC incompressible path

任务：

1. 增加配置或内部开关，让 incompressible path 可以跳过 APIC affine。
2. 当 APIC blend 为 0 时：
   - P2G 不使用 `particle.c * mass` affine term。
   - G2P 不计算 `new_c += outer_product(...)`。
   - `particle.c` 保持 `Mat3::ZERO`。
3. 对比纯 PIC 是否过粘、过耗散，是否仍满足视觉需求。

验收：P2G/G2P 时间下降；水体不出现不可接受的粘滞或体积退化。

预期收益：中高。

2026-05-19 实现：增加 `PondWaterConfig::incompressible_apic_blend` 和 CLI `--water-apic-blend`。当 blend 为 `0.0` 时，incompressible P2G 跳过 `particle.c * mass`，G2P 跳过 `outer_product(...)`，并强制 `particle.c = Mat3::ZERO`。`performance` profile 默认使用 no-APIC / pure-PIC path。

1024 粒子 release hidden 对照（`--water-profile performance --water-particles 1024`，spacing=1，9 samples）：

| incompressible APIC blend | avg/substep |       P2G | G2P gather | G2P total |
| ------------------------: | ----------: | --------: | ---------: | --------: |
|                    `0.10` |    `2.28ms` | `13.53ms` |  `14.25ms` | `41.77ms` |
|                    `0.00` |    `2.23ms` | `11.86ms` |   `8.96ms` | `36.76ms` |

隔离 spacing 的对照（spacing=0，5 samples）也显示 transfer 下降：P2G `12.54ms -> 11.62ms`，G2P gather `14.16ms -> 9.08ms`。hidden diagnostics 保持 `finite=1024`、`non_finite=0`、`j=1.000..1.000`，no-APIC run 的 `affine_max=0.00`；2026-05-19 窗口观察表现正常。

### P3：pressure projection 自适应化

任务：

1. 增加 divergence residual / max divergence 统计。
2. 尝试 pressure warm start，不再每个 substep 全部清零。
3. 支持 early exit：残差低于阈值时提前结束 Jacobi。
4. 做 iteration 对照：

```bash
--water-pressure-iterations 4
--water-pressure-iterations 8
--water-pressure-iterations 16
```

验收：projection 时间下降，且水体不可压表现、边界稳定性、terrain penetration 不恶化。

预期收益：中等，取决于 projection 在当前 profile 中的占比。

### P4：复用 P2G / G2P particle stencil

任务：

1. 每个 substep 为粒子临时生成一次 stencil：base coord、quadratic weights、可选 27 个 node index/weight。
2. P2G 和 G2P 共用 stencil，减少重复坐标、权重、索引计算。
3. stencil 是 substep 临时数据；G2P 推进位置后，下一个 substep 重新生成。

验收：P2G/G2P 时间稳定下降；行为 bit-for-bit 不强求，但统计行为应接近。

预期收益：中等偏小，但物理风险低。

### P5：减少 terrain exact fallback

任务：

1. 分析 `g2p_exact_checks/substep` 和 `g2p_exact_corr/substep` 在高粒子数、贴地水体中的占比。
2. 扩大 cached normal / near-surface band 或放宽 `terrain_grid_particle_query()` 的 fallback 条件。
3. 优先使用 cached trilinear SDF + gradient；只在不确定区域走 exact SDF。
4. 保持 shadow sample 验证，避免 cache false skip。

验收：exact checks/corrections 降低；`terrain_shadow_false_skips` 保持 0；穿地不增加。

预期收益：中等或偏小，依赖场景。

### P6：清理不可压路径中的 legacy EOS 状态

任务：

1. 把 `j / stiffness / gamma` 明确限制在 legacy EOS 模式。
2. 不可压路径中避免无意义的 `j` clamp / log / debug 热路径开销。
3. 后续可考虑把 legacy 和 incompressible 粒子状态拆分。

验收：代码更清晰；热路径有小幅改善或至少不退化。

预期收益：小，但有利于后续 position-primary 重构。

### P7：water sim 线程化

任务：

1. 定义主线程 command queue 和 render snapshot 边界。
2. water worker 独立推进 solver，主线程最多渲染一帧旧 snapshot。
3. terrain collider/cache 消息全部带 revision，禁止旧消息覆盖新状态。
4. 增加 shutdown、backpressure、lag 日志。

验收：主线程 frame time 降低；water 延迟可接受；退出无 hang；terrain/cache revision 不回退。

预期收益：对主线程 hitch 高，对总 CPU 成本无本质改善。因此放在 kernel 优化之后。

## 推荐执行顺序

已完成：

- `benchmark spacing iterations`
- `add incompressible no-apic path`
- `validate performance profile with --water-spacing-iterations 0`
- `make performance profile spacing=0`

下一步：

1. `benchmark pressure projection iterations`
2. `add projection residual early exit`
3. `reuse water particle stencils / explore G2P -> next P2G fusion after spacing is removed or replaced`
4. `reduce terrain sdf exact fallback`
5. `clean incompressible legacy eos state`
6. `prototype threaded water sim behind flag`

若后续场景暴露 spacing=0 的视觉问题，再回到 `prototype PBF-like compression-only density projection`，不要恢复优化旧 pairwise spacing。

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
