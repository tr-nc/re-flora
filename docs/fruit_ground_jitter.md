# 果实落地持续抖动诊断与修复

基线 `95851a6f5f0968299f671049797ecb6deb057812`，独立分支 `agent/fruit-ground-jitter`。
本任务 session `01a0948d-443a-7e51-8050-1b4e6e070c51` 的 `turn_context` 实际记录
`model=gpt-6-astra`、`effort=xhigh`；进程启动参数与 worktree 路径也吻合。

## 可重复的失败信号

CPU 命令（正常确定性回归；诊断提交 `cbc9ee2b` 中曾显式 ignored 以保存失败证据）：

```sh
cargo test -p re-flora-physics --test fruit_ground -- --nocapture
```

相同 4-voxel 直径苹果凸包、256 voxel/world-unit、重力 `-2508.8 voxel/s²`、
120 Hz、原摩擦／恢复系数／阻尼／0.15 voxel 接触皮肤，落到单块静态平坦 Voxels 地面。
模拟 30 秒仅需约 0.04 秒；最后 10 秒竖直范围 `0.12323189 voxel`，休眠 `0/1200`，断言失败。
16 块地面缩减到 1 块仍复现；没有地形编辑、外力、渲染或果实生命周期调用。

真实游戏入口（只在显式设置环境变量时运行诊断，复用正常 registry/drop/render）：

```sh
mkdir -p target/fruit-ground-evidence
flock --close /tmp/re-flora-summer-gpu.lock \
  env RE_FLORA_FRUIT_GROUND_TRACE=target/fruit-ground-evidence/baseline.jsonl \
  cargo run --release -- --hidden --mute --auto-exit 75
python3 scripts/check_fruit_ground_trace.py target/fruit-ground-evidence/baseline.jsonl
```

最终诊断入口等待启动碰撞更新队列排空后重新挂果，再等至少 30 帧、确认队列仍为空后请求掉落，不修改保存的 GUI 值。正常游戏的放果时机与碰撞更新逻辑不变。最多 128 个动态果实逐渲染帧记录（初期记录 8 个，最终覆盖默认场景全部 17 个）：
位置、四元数、线／角速度、自然休眠／计时、接触数／距离／法线／冲量、用户力／矩、
实际送往渲染实例的姿态、帧 dt／固定 dt／步数／丢弃时间／余量，以及待更新碰撞砖数量。
正常游戏不启用该入口；CPU 诊断另外逐固定步采样，避免只看单个帧末速度。

第一次 45 秒 release 基线：`target/re-flora-logs/re-flora-20260912-154717.558-460871.log`。
最终 8 个果实最后 10 秒全部未休眠，Y 范围 `0.1183–0.1653 voxel`。
成功退出，`[SHUTDOWN] phase=complete failures=0`，无 error/panic/VUID。
GUI SHA-256 前后均为 `2aeedce8e2d2f3036920313b7aec11589b7e538214d4e3986d1c56e8e3255ae3`。
这证明真实掉落后持续运动，不代替用户视觉验收或性能基准。

## 根因与反证

锁定 Rapier 0.34.0 / Parry 0.29.0。在 CPU 第 2400 步（20 秒）后重复出现：

| 步 | Y | 竖直速度 | 接触流形 | 休眠计时 |
|---|---:|---:|---:|---:|
| 2400 | 4.0088134 | 0.0000157 | 1 | 0.025 |
| 2401 | 3.8999248 | -20.896204 | 0 | 0.0333 |
| 2402 | 3.9362338 | 1.2531691 | 1 | 0 |
| 2406 | 4.0126495 | 0.0000147 | 1 | 0.025 |
| 2407 | 3.9037604 | -20.896206 | 0 | 0.0333 |

Rapier 给接触生成器传入 prediction + 两个 collider 的 contact skin。
但 Parry 的 `contact_manifolds_voxels_shape` 在枚举体素候选时只用形状的原始 AABB，
没有按 prediction 扩展；形状刚离开实际地面就丢失全部预测接触。求解器推动果实维持皮肤
间距，候选枚举又丢失支撑，下一步重力拉回，形成永久循环并持续打断自然休眠。

- 单个静态砖仍失败：排除砖接缝／碰撞体重建是必要条件。
- 没有渲染仍失败：不是物理到渲染的姿态转换导致。
- 用户力、矩始终为零，没有主动唤醒：排除外部扰动与频繁生命周期唤醒。
- 仅关闭 CCD 仍失败：不是 CCD 的充分原因。
- 仅将 skin 设为 0 后自然休眠，但地面穿入约 `0.0365 voxel`：仅作诊断，不采用这个改参数方案。
- 现有 5 秒测试只验水平速度和角速度，在抖动周期的大部分帧末仍可通过，遗漏 Y 与连续状态。

上游已有完全对应的修复：[Parry d299cdca](https://github.com/dimforge/parry/commit/d299cdca86767680285bb838f37c699b59cfa17f)。
其变更是在该候选 AABB 上调用 `.loosened(prediction)`，并新增间隔 0.05 的预测接触测试。
这是支持现有皮肤契约的接触生成修复，不改变果实形状、求解参数或运动状态。
直接升级到含该修复的上游版本还会包含 0.30 系列 BVH、GJK、形状扫掠等 83 个文件的变更，
本任务采用有明确版本来源的窄范围回移。

相关文档：[体素碰撞架构](voxel_collision_architecture.md)、
[行走平滑及方向回归](character_walk_smoothing_validation.md)、[花园存档生命周期](garden_snapshots.md)。

诊断步骤复核：`cargo fmt --check`、`cargo check`、物理 crate 的 15 个单元测试／21 个角色测试／1 个地形更新测试通过；新复现显式 ignored 并手动运行判红。第二次 45 秒运行日志 `re-flora-20260912-155122.608-463386.log` 同样正常退出，记录实际渲染实例字段，检查器判红；结果见 [基线摘要](evidence/fruit-ground-jitter/baseline-summary.json)。


## 第二层缺陷：接触缩减丢失皮肤内的支撑多边形

只回移 Parry 的 AABB 修复后，直接间距测试通过，完整落果测试仍失败：最后 10 秒位移范围
约 `(0.1396, 0.0664, 0.1493) voxel`，仍未休眠。因此没有把单个库回归通过当作修复完成。

进一步缩减的真实苹果用例：水平姿态，底面与地面相隔 `0.1 voxel`（skin 为 `0.15`），
原始流形包含 5 个距离均为 `0.099999905` 的点，Rapier 最终只保留 **1 个 solver contact**。
`apple_inside_contact_skin_keeps_a_support_polygon` 先以此输出失败，再修复通过。

Rapier 0.34.0 的 `narrow_phase.rs` 给 `reduce_manifold_naive` 传入裸 `prediction_distance`，
缩减器对第二到第四个点检查原始 `pt.dist <= prediction_distance`，漏掉了 skin。
后续 solver 构造虽然会减去两侧 skin，但剩余支撑点已被提前删除。
这会在预测接触恢复后引入非对称支撑和反复摇摆，也影响果实之间的凸包接触。

在同一个接触分发入口中提前运行原版四点选择算法，传入已经包含 skin 的完整 prediction；
保留点的几何位置、距离、feature ID 和暖启动数据，按原算法选择顺序交给 Rapier。
Rapier 收到至多 4 点后不再执行那次有缺陷的缩减；原有 per-point 过滤、摩擦、恢复系数、
CCD、积分和自然休眠继续负责运动。没有改位置、强制清零、额外外力或替代碰撞几何。

源文件与上游版本、机械适配和将来升级时的移除步骤见
[兼容模块说明](../crates/re-flora-physics/src/contact_prediction/README.md)。

## 真实斜面姿态的求解收敛

两层接触修复后第一轮 45 秒运行（`re-flora-20260912-155928.248-471873.log`）
7/8 个采样果实已自然休眠；fruit `4355240104586327797` / body 22 仍未休眠：
最后 10 秒范围 `(0.04147, 0.00175, 0.04164) voxel`，最大线速度约 `0.5506 voxel/s`。
所有接触持续存在，地形队列为零，渲染姿态与物理姿态一致，检查器保持失败。

将该果实真实姿态/速度直接重放在单块平坦体素地面上仍失败（正常测试
`tilted_apple_ground_replay`）。初始位置为 `(265.72668, 109.43418, 251.00658)`，
四元数 `(0.35835579, 0.10532805, -0.28461754, -0.88288164)`，对应苹果凸包的斜面着地。
固定形状、坐标、重力、材料、步长、sleep 参数，单独改变 Rapier 的 body additional iterations：

| 额外迭代 | 30 秒后自然休眠 |
|---:|---|
| 0 | 否 |
| 1 | 否 |
| 2 | 是 |
| 4 | 是 |
| 8 | 是 |

单块地面的 2 次迭代不足以代表完整场景：75 秒实机运行中仍只有 16/17 个果实休眠。最终采用 **4 次额外迭代**，仅由 `apple_dynamic_body_desc` 配置，泛用 body 默认为 0。
这是提高该接触岛的求解精度，未改变睡眠门槛或用阻尼耗散运动；全局固定步长仍为 120 Hz。
额外工作会作用于与果实接触的整个 Rapier island；大型堆叠的性能仍需独立 release 基准，
本任务没有宣称性能验收。临时 `FRUIT_DIAG_*` 参数已全部删除。

## 回归边界

- 单块平地、四砖交点、真实尺度且非零世界坐标：每例 30 秒，最后 10 秒位置/旋转完全不变，
  `1200/1200` 步自然休眠；落地前仍有弹跳与旋转/平移，落地后未穿入地面。
- 原地只删除支撑区域，保留砖中其他体素和 collider，走实际增量更新路径；果实立即被唤醒，
  0.2 秒后下降超过 10 vox、向下速度超过 20 voxel/s。包含跨砖支撑编辑。
- 从静止放在真实阶梯体素斜坡上可向下移动并翻滚；两个同形状苹果仍互相碰撞并弹开。
- 采集真实 Contree 占用数据，保留斜面重放实际经过的一块完整砖并检查边界，重放斜面姿态；以 30/60/144 Hz 提交帧时间、仍由 120 Hz 固定步积分，30 秒后完整姿态稳定并自然休眠。
- 复用实际游戏 `apple_dynamic_body_desc` 的 5 秒回归增加自然休眠及随后 1 秒完整状态不变断言。
- 角色的形状扫掠始终委托原版 Parry，现有跨阶/方向/低顶/墙/悬崖等 21 项测试通过。
- 隐藏 trace 验证的是实际 Vulkan 游戏运行与送往渲染实例的姿态，不是用户目视/手动试玩验收。

## 启动地形更新对诊断的干扰

早期诊断固定在第 90 帧重新掉果。最终 4 次迭代的早期 75 秒记录中虽然 17/17 个果实
最后 10 秒都已休眠，但 body 18 最终落到 `Y≈3` 的底层；该记录只能证明停止抖动，不能作为地形碰撞通过证据。
8 次额外迭代也没有消除此现象，因此没有继续提高迭代数来掩盖它。

继续逐帧检查发现，body 18 第 150 帧仍有 **1610 个待更新碰撞砖**；第 154 帧为 1599，
当时位置是 `(262.46,107.04,262.40)`。这些帧的碰撞几何与 20 秒后采集的静态地形并不相同。
启动树发布会排队刷新碰撞砖；正常 pending-drop 只等待初始竖直掉落路径所需砖，
快速横向滚动能够进入尚待更新区域。插入后来的地形还可能与已有动态果实重叠。

对较晚的两个实际姿态进行重放，原版与修复版都能从深度重叠状态继续下落；但逐体素几何检查
证明这个起点已有重叠。因此它**不能证明原版与修复版从无重叠起点走过相同的穿透路径**。
进一步从第 140/150 帧的无重叠姿态重放，原版与修复版均未穿透。已删除把此现象直接命名为
“已确认基线树根挤压穿透”的临时 ignored 测试，保留原始实验日志于 `target/fruit-ground-evidence/`。

修正的是诊断入口的放果前置条件：等待实际碰撞队列排空后再触发既有挂果/掉落流程。
检查器现在要求受控掉落的**全部样本**中待更新碰撞砖为零，并输出最终坐标供核对。
启动期动态果实进入尚未更新区域的生命周期边界未在本任务内重设计；正常游戏实现保持基线行为。

## 最终验证记录

最终日志：[75 秒 release 隐藏静音运行](../target/re-flora-logs/re-flora-20260912-164713.663-501838.log)。
原始逐帧数据：`target/fruit-ground-evidence/ready-terrain.jsonl`；
可归档摘要：[最终结果](evidence/fruit-ground-jitter/final-summary.json)。

| 检查 | 结果 |
|---|---|
| 实际模型与 base | `gpt-6-astra / xhigh`；起点 HEAD 精确等于 `95851a6f5f0968299f671049797ecb6deb057812` |
| `cargo fmt --check` / `git diff --check` | 通过 |
| `cargo check` | 通过；已有 6 项无关 unused/dead-code warning |
| `cargo test --quiet` | 主程序 906 + 工具 4 通过，2 项原有 ignored；ALSA 设备探测输出仍存在 |
| `cargo test -p re-flora-physics` | 16 单元 + 21 角色 + 6 果实 + 1 地形重写，共 44 项通过，无 ignored |
| 隐藏静音短运行 | 0.5 秒正常退出；`re-flora-20260912-162947.919-498102.log` |
| 最终隐藏静音长运行 | 75 秒；累计物理时间 74.762 秒，受控掉落后持续观察 69.595 秒 |
| 地形就绪 | 第 670 帧重新挂果，第 700 帧掉落；该批全部样本 pending terrain = 0 |
| 最终 10 秒 | 17/17 自然休眠；位置范围全为 0，四元数不变（角度计算舍入误差最多 `4.22e-8 rad`） |
| 收敛时间 | 最晚首次休眠为模拟时刻 9.142 秒，即本批掉落后约 3.975 秒 |
| 固定步与渲染 | 120 Hz，无丢弃步时；实际渲染位置／旋转与物理输出误差均为 0 |
| 落点检查 | 最低中心 Y=95.253；其余在地面或树体表面，未出现早期探针的 Y≈3 异常底层落点 |
| 日志与退出 | 无 ERROR/panic/VUID；shutdown failures=0，Application exited successfully |
| GUI / 生成文件 | `config/gui.toml` SHA-256 始终为 `2aeedce8e2d2f3036920313b7aec11589b7e538214d4e3986d1c56e8e3255ae3`；构建派生 Rust、Cargo.lock 均无变更 |

长运行都持有 `flock --close /tmp/re-flora-summer-gpu.lock`，从此 worktree 调用日志助手复核。
另有一个既有 butterfly atlas 重复资源警告，本任务未修改相关资产或功能。

边界：这次验收包含真实游戏的数值与渲染输入同步、CPU 的反弹/滚动/斜坡/果实互撞及编辑唤醒，
没有自动打开可见游戏，也没有声称用户目视、密集果堆性能或启动期碰撞更新竞争已通过。
物理场景受控掉落的统计也不等于任意地形下绝不穿透的证明。

## 改动范围

- `crates/re-flora-physics/src/contact_prediction.rs` 及子目录：版本明确的原生接触修复回移，
  上游授权文件、来源和升级移除说明；`Cargo.toml` 固定审计过的 Rapier 版本。
- `crates/re-flora-physics/src/lib.rs`：安装 dispatcher，增加 body 额外迭代配置及只读诊断数据。
- `crates/re-flora-physics/tests/fruit_ground.rs` 与 `tests/fixtures/fruit_ground_scene.rs`：
  根因失败回归、自然运动/休眠/编辑唤醒及真实地形姿态重放。
- `src/app/core/physics.rs`：仅苹果配置额外迭代，加强实际描述的静止测试，连接 opt-in trace；
  `src/app/core/physics/fruit_ground_trace.rs` 与 `scripts/check_fruit_ground_trace.py` 提供可重复实机证据。
- 本报告、体素碰撞架构补充和 JSON 摘要。没有改动角色行走实现、叶片、蝴蝶或管道功能。

诊断阶段提交为 `cbc9ee2b`。修复及最终验证随后单独提交；未 merge/push，未管理 Worker。
