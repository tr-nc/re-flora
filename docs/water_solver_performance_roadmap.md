# Water Solver Performance Roadmap

本文档记录 2026-05-19 讨论后的水模拟性能优化计划。它聚焦 `crates/re-flora-water` 里的不可压 MLS-MPM solver 本身；terrain-water cache / SDF source / visible terrain rebuild 的历史计划仍见 [`water_mls_mpm_performance_roadmap.md`](water_mls_mpm_performance_roadmap.md)。

## 目标

- 优先降低高粒子数下的 CPU water sim 成本。
- 优先做可测量、可回退、物理行为风险低的优化。
- 保持当前不可压路线：主不可压约束仍由 MPM grid pressure projection 解决，粒子 spacing / PBF-like pass 只作为辅助。
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

1. `relax_incompressible_particle_spacing()`：每个 substep 建 HashMap、分配 correction buffer、做邻域 pair 查询，并在 spacing 后重复碰撞/repair。
2. incompressible APIC affine：即使 `j` 已经在不可压路径中固定为 `1.0`，`c: Mat3` 仍参与 P2G/G2P 的 27-node 热循环。
3. pressure projection：固定 Jacobi iteration，且每步 pressure 从 0 开始。
4. P2G/G2P 重复计算 particle stencil：base coord、weights、node indices 在同一 substep 中重复计算。
5. terrain exact fallback：贴地粒子可能持续触发精确 SDF 查询。

## 性能优先级计划

### P0：建立基准与 spacing 开关对照

目的：先确认最大嫌疑点，避免盲改。

验证命令建议使用 release hidden run，并检查 latest log：

```bash
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance --water-spacing-iterations 0
cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance --water-spacing-iterations 1
cargo run --release -- --hidden --auto-exit 4 --perf --water-profile performance --water-spacing-iterations 2
cargo run --release -- --latest-log
python tools/parse_perf_log.py
```

验收：得到 `spacing_relax`、water avg/substep、frame CPU/other、粒子稳定性和 terrain penetration 的对照表。

预期收益：如果 spacing 是大头，后续 P1 收益高；如果不是，则下调 P1 优先级。

### P1：优化 `relax_incompressible_particle_spacing()`

任务：

1. 复用 `bins` / `corrections` scratch buffer，避免每个 substep / iteration 反复分配。
2. 支持 spacing 降频，例如每 2-4 个 substep 执行一次。
3. 只对局部密集区域或移动粒子附近执行 spacing。
4. spacing 后只对实际移动过的粒子做 box / terrain / repair。
5. 保持 correction cap 和 terrain collision 安全逻辑，避免为了性能引入穿地。

验收：高粒子数 release run 的 `spacing_relax_ms`、water avg/substep 和 frame CPU/other 明显下降；`terrain_penetrating` 不恶化。

预期收益：高。

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

1. `benchmark spacing iterations`
2. `reuse water spacing scratch buffers`
3. `throttle water spacing relaxation`
4. `add incompressible no-apic path`
5. `benchmark pressure projection iterations`
6. `add projection residual early exit`
7. `reuse water particle stencils`
8. `reduce terrain sdf exact fallback`
9. `clean incompressible legacy eos state`
10. `prototype threaded water sim behind flag`

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
