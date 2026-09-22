# Climbing vine：交互、物理与 CPU 调优

> 历史记录：2026-09-23 用户选择用“失去支撑后截断、断口再生”替代本文的沉降模型。
> 当前行为和验证见 [../climbing_plants.md](../climbing_plants.md)。下列速度对比只对应旧模型。

2026-09-22，工作分支 `grepping-plants`。基线 `d02c43d6`（只增加 profiling，物理与
`9ec8583a` 一致）；本轮实现截至 `19149159`。这是一次已验证的小步改进，不是完整
XPBD／生物力学重写，也不是发布性能验收。

## 已经改变了什么

- **交互集中到 R → Climbing Plants**：可保存的参数、现场状态和动作放在一起。
  新增独立的 Pause vine growth，恢复时保留原速度，不补长暂停期间的枝条。
- **Peel highest / all attachments**：不必精确挖中小附着点就能观察失去墙面支撑的反应。
  不修改地形、节点 ID、静止长度或根部连通性。Disconnect root 仍然是独立动作。
  根仍连接时，后续生长可能产生新附着；观察掉落时可先暂停生长。
- **反馈与保护**：显示节点、尖端、附着／释放数量、断根或等待地形状态；不可用动作禁用。
  重置现有藤蔓必须确认；Focus vine 对准当前骨架，而不是始终对准原墙中心。
- **碰撞导出缓存**：复用覆盖当前查询范围的不可变 Contree 导出块；每次检查所有依赖的
  presence、revision 和 decoded-cache readiness。NotReady 时不使用旧数据顶替。
  16-voxel 对齐范围减少生长时的重复导出，重置和加载清除缓存。
- **解算器精确固定点退出**：完整一轮距离／接触投影后，如果所有可动节点位置完全未变，
  不再重复相同运算。保留 128／512 上限、扫掠检测、整段碰撞和 0.002-voxel 长度检查。
  没有放宽容差、减少困难情况的上限，也没有把近似静止当成休眠。
- **稳定的时间推进**：正常游戏中的沉降量子固定为世界模拟时间 20 Hz，保持原默认
  50-ms world tick 的速度。修改 World Tick Time 不再直接改变下垂速度。
  生长和沉降使用独立计时器；每次最多补算八个量子，不无限积压。

实现提交：

| 提交 | 内容 |
| --- | --- |
| `d02c43d6` | release app 的藤蔓分阶段计时和 voxel-query 计数 |
| `eed578ad` | 带完整依赖／就绪检查的碰撞块缓存 |
| `6166d5ec` | 精确固定点退出，保持原物理结果 |
| `617ad2fc` | 集中交互、剥离动作、保存暂停选项、重置确认 |
| `19149159` | 独立固定物理时钟、有限补算 |

## Release 测量：典型成本下降，但尖峰没有全部解决

环境：Linux x86-64、Intel i5-12600KF、NVIDIA RTX 3060 Ti；四轮日志均为
1440×810 render extent。相同默认画面设置、同一工作目录，不设置 present mode。
没有并发启动另一份游戏。截图运行不参与性能统计。

执行保存的两个 release 二进制，各两次，顺序 candidate、candidate、baseline、baseline；
更早还有一轮 baseline、cache-only 和 solver-only 的诊断记录。每轮使用：

```sh
RE_FLORA_CLIMBING_REVIEW=1 <release-binary> --hidden --mute --perf --auto-exit 16
```

这是现有确定性真实地形编辑场景：生长 → 挖一处支撑 → 回填穿过尖端 → 挖上方多处支撑
→ 断根并移除其余支撑。review 路径固定每个 ready frame 一次生长／沉降，**故意不使用
正常游戏的 20-Hz 计时器**，避免帧率改变工作负载。只取 review tick ≤ 280，逐样本核对
`(phase, tick, nodes, attached, quanta)` 完全相同，再合并每侧两轮；p95 为 nearest rank。
这是固定过渡过程，不删除冷启动／重导出的样本，也不冒充稳定态 GPU benchmark。

下表是每次 **藤蔓 CPU 更新**，单位 ms；不包括创建／挖墙事务和相机操作，不是整帧时间，
更不能直接换算成 FPS。

| 场景 | 每侧样本数 | 原版中位数 → 新版 | 原版 p95 → 新版 |
| --- | ---: | ---: | ---: |
| 初始生长 | 82 | 1.202 → 0.343 | 3.148 → **4.184** |
| 单处支撑删除后继续生长 | 120 | 7.039 → 1.274 | 7.398 → **8.219** |
| 尖端回填 | 80 | 8.288 → 1.310 | 8.583 → 1.380 |
| 多处支撑删除 | 240 | 6.264 → 0.967 | 6.586 → 1.529 |
| 完全脱离，早期沉降 | 120 | 5.192 → 0.347 | 5.446 → 0.391 |

解释：

- 原版多处支撑删除阶段：地形导出中位数 **4.867 ms**、解算 **1.381 ms**。
  新版分别约 **0.002 ms** 和 **0.955 ms**；总更新下降约 **84.6%**。
- 完全脱离阶段解算中位数从 **1.418 ms → 0.337 ms**；总更新下降约 **93.3%**。
  voxel 查询中位数从 **394,022 → 87,719**。这是重复工作减少，不是模拟变慢。
- **不能只看中位数**：生长时缓存扩容会导出更大的对齐块；发生地形编辑时仍需完整重导出。
  本轮初始生长和单处删除阶段的 p95 分别变差约 33% 和 11%。回填阶段单次最大值从
  **8.672 ms → 17.281 ms**，虽然该阶段中位数／p95 大幅下降。
- 因而本轮只能确认典型更新显著便宜，**不能声称已经消除交互卡顿或满足任何帧预算**。
  下一项性能工作应是有界的增量块／分块导出，而不是继续堆大缓存范围。
  在新区域数据齐备前仍必须安全暂停，不能用未知当空气。

本地证据（未提交的构建产物）：

- `target/climbing-validation/tuning/baseline-{2,3}.log`
- `target/climbing-validation/tuning/candidate-{1,2}.log`
- `target/climbing-validation/tuning/comparison.json`：各 scope 的 median/p95/max。
- `target/climbing-validation/tuning/{baseline,candidate}-re-flora`：对应 release 二进制。
- 同一 worktree 的 `--latest-log`／`--tail-latest-log` 可检索原始 app 日志。

## 物理正确性与验证边界

`cargo fmt --check`、`cargo check`、完整 `cargo test` 和
`cargo run --release -- --hidden --mute --auto-exit 0.5` 通过。
最后一次完整测试：1087 + 4 passed，2 ignored。GUI generated Rust 只由构建生成，新增
暂停字段沿用统一保存测试；没有额外每设置 save hook，没有手改生成文件。

新增确定性 guardrails：

- 同范围缓存重用；跨 chunk revision、presence、相同 identity 但缓存未就绪均失效；
  扩大范围不越界重用；无关 chunk 修改不失效。
- 生长／局部脱附／自由沉降／回填恢复与完整迭代预算逐状态精确相等。
  query-count 测试只防止重复工作，不作为性能证据。
- 剥离不改拓扑、根连通、静止长度和生长历史；回填不会复活旧附着。
- egui 真实 pointer press/release 测试验证重置确认／取消、剥离／断根按钮及禁用状态。
- 暂停无补长；有限补算；10／50／100 ms world tick 在一秒模拟时间内产生相同
  20 次沉降，实际 Plant 位置一致。旧版 10 ms 会执行 100 次，测试先失败再修正。

四轮最终 release review 都通过：

```text
stable_ids=true unaffected_supports=true collision_clear=true refill_retained=true
finite=true max_length_error=0.001128 max_motion_voxels=9.6069 nodes=126 attached=2
root_cut=true growth_stopped=true attached=0 root_drop_voxels=2.3969
phase=complete failures=0
```

未出现 ERROR、panic 或 Vulkan VUID；保留既有 11 条 Rust 编译 warning，未顺手清理。
`interaction.00000{0,1,2}.png` 是隐藏 app 的实际 2880×1620 画面，已检查其中生长／挖墙
画面；带截图的 8 秒运行未走完 review，另做无截图的完整运行通过。截图不是 Debug 面板
视觉验收或动效评价。**尚未做玩家手动交互、美观认可、多株大规模或新时钟下的 app 对照实验**；
时钟的 cadence 一致性是实际 Plant 单元测试证据，不能夸大成完整 app 动态验收。

## 研究：下一步怎样让效果更自然

下述是设计建议，**未实现**。不把论文的 GPU 数量／旧硬件性能当成我们的预算。

### 1. 惯性、弯曲与阻尼：XPBD 比“再加迭代”更值得做

当前模型是准静态下垂加距离投影：没有速度状态、弯曲能量或真实受力估计。它可以安全地
释放和沉降，但不会自然摆动，也不能基于张力模拟逐级剥离。

Macklin、Müller、Chentanez 的 [XPBD 论文][1]（读了作者版 Abstract、Algorithm 1、
§4–5）明确处理传统 PBD 刚度依赖迭代次数／时间步的问题。关键不是改名字，而是：

- 用位置、速度预测新位置；最后由位移回写速度。
- 为每个约束保存步内累计 Lagrange multiplier，compliance 按 `alpha / dt²` 缩放。
- 约束力估计可用于断裂／剥离；阻尼有单独的能量模型，不应靠“少迭代”伪装。

项目适配建议：低分辨率茎骨架增加速度和弯曲约束，保留硬长度约束与稀疏附着；叶片先做
骨架跟随与便宜的视觉响应，不做每片叶子的刚体。先看释放一处支撑后的小幅回摆、长自由端
的弯曲、完全脱落后的落地，避免为了明显效果把几乎绷直的茎硬拉长。

### 2. 小步长与休眠：正确性先于少算

[Small Steps in Physics Simulation][2] 的出版方摘要明确比较：一次大步加 n 次迭代，
对比 n 个小步、每步一次 XPBD 迭代；作者报告后者降低约束误差与人工阻尼。本轮查阅该摘要，
不声称复现论文。**不能据此直接把本项目的 128 次投影改成一次**：本项目还没有该论文的
速度积分、compliance 和接触处理路径。

[Box2D 官方 simulation 文档][3] 的 fixed-step/substep 和 sleep 说明提供了另一个实践参照：
休眠应在系统静止后触发，接触／关节被破坏会唤醒；运动事件可以避免无变化对象重复更新。
对藤蔓可以评估按机械支撑分隔的活动段休眠，但生长、剥离、地形 revision/readiness 变化及
新接触必须可靠唤醒。**不动也可能意味着约束不可解或数据未就绪，不能只数几帧零位移就睡眠。**

### 3. 更好的操控：抓取／引导，而不是只有调试按钮

Hädrich 等 [2017 作者项目页][4] 展示连接的各向异性粒子、可拖拽／修剪／播种、动态环境
及枝条弯曲／折断。它证明这种交互方向可行，但不是当前项目已经实现这些能力的证据。

适合下一轮的小闭环：点击选中一个茎节点 → 显示合法拖拽范围 → 用临时抓取约束缓慢引导
→ 松手后保留真实历史与回弹。抓取不应该直接改节点位置穿过墙；必须通过同一碰撞和
长度约束。其次才是引导尖端沿缺口探索、成熟后重新形成新附着 ID，以及更自然的叶形。
平墙之外的转角生长需要局部表面法线和可靠搜索，不能简单把失败尖端吸到墙另一边。

如果实现有不同观感的新动力学，按仓库约定提供**Debug 运行时 A/B checkbox**：未选中为
现有模式，选中为实验模式；先让玩家比较形态、回摆和操控，再单独测 release 性能。
本轮的精确固定点优化没有新增视觉实验模式。

## 来源

[1]: https://matthias-research.github.io/pages/publications/XPBD.pdf
[2]: https://diglib.eg.org/items/322424d5-a24f-490a-a6bb-542233852032
[3]: https://box2d.org/documentation/md_simulation.html
[4]: https://storage.googleapis.com/pirk.io/projects/climbing_plants/index.html

更早的攀墙模型／植物学研究见 [procedural_climbing_plants.md](procedural_climbing_plants.md)。
实际操作说明见 [../climbing_plants.md](../climbing_plants.md)。
