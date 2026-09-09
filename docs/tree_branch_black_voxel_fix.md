# 树枝近黑体素：Blacky 复现与修复

分支 `agent/tree-branch-lighting`，基线 `29cbeff9`。更新于 2026-09-09。

## 当前结论

用户在 `blacky` 镜头否决了第一次视觉验收。`191c8f9f` 只恢复了原启动镜头中
数学上完全为零的三个体素，不能覆盖 Blacky 中非零但接近黑色的树干斑块。
当前实现替代该次修复，不能沿用其“问题已解决”或性能结论。

Blacky 的主要问题有两层：

1. 平滑着色法线的硬半球筛选提前剔除了有光、几何可见的探针。探针存储的是
   按表面法线查询的方向辐照度，不是来自探针位置的点光源；平滑法线背侧并不
   等同于它位于实体内部或不可用于插值。
2. 原低置信度归一化会把孤立探针的贡献衰减至很暗。这适用于不确定的距离矩
   估计，不能继续用来压暗已由当前体素几何证实可见的样本。

当前地形接收端先检查同一插值单元的 8 个探针。保留 probe 状态、空间范围、
几何 revision 和已有接收点约束，以平滑的 wrap 权重代替硬半球资格筛选；只有
精确体素线段畅通的样本参与该次插值，并用它们的可见权重归一化。如果没有
精确可见样本，则保留原保守距离矩查询及 canonical 零值规则。

没有环境光底值、材质增亮、天空回退或关闭阴影；没有改变太阳/天空权威参数。
wrap 项属于插值权重，不能在没有可见、有能量的探针时制造光照。Raster 消费者、
probe 传输和原诊断 reference 路径保持不变。

**偏移说明修正：** 旧报告把生产默认偏移写成“四分之一体素”有误。当前及先前
保存的 GUI 配置均为 `0.00390625`（一个体素），函数消费配置值。本次保持原值，
没有扩大偏移。`blacky/exact-segments.json` 是使用 quarter-voxel 假设的对照实验，
实际配置的 CPU 交叉检查见 `configured-bias-exact-segments.json`。

## 复现命令与场景

```bash
CARGO_BUILD_JOBS=2 cargo build --release
python3 scripts/check_tree_branch_lighting.py --scene blacky \
  --output target/summer-evidence/blacky/repro
python3 scripts/check_tree_branch_lighting.py --scene blacky --screenshot \
  --output target/summer-evidence/blacky/image
# 兼顾第一次报告的精确零值症状：
python3 scripts/check_tree_branch_lighting.py --output target/summer-evidence/startup-repro
```

脚本从当前 worktree 运行 release app，内部使用 GPU flock、hidden、mute，保存并
逐字节恢复 GUI/相机配置。Blacky 使用用户镜头数值的独立临时 preset，不覆盖
用户保存的 `blacky`：位置 `(0.816733, 0.6137017, 0.8741581)`，yaw `101.928986°`，
pitch `12.935698°`，FOV `60°`，fly mode。树 seed `122`、size `20`、age `1`，
时间 `0.47`、自动昼夜关闭，使用保存的原 GUI 参数。

2026-09-09 的物理输出为 `1920×1080`，tracer `960×540`（与 09-07 的显示缩放不同）。
浮点回归采集首份已发布的 DDGI 场；截图是独立运行 3 秒后的真实稳定画面。
两者不能冒充同一物理帧。最初 `screenshot-delay=0` 的截图早于稳定光照，不能
作为 Blacky 视觉验收依据，回归脚本现已取消这类自动截图。

`--scene blacky` 跟踪用户画面中 10 个实际近黑斑块，不再只找精确零值。固定场景
用邻近正常树干体素 `(264,169,231)` 作对照，若任一指定斑块的环境光 RGB 和中位数
不足对照的 25%，则判红；缺样本、暗对照或非有限数据判 INVALID。这个阈值只约束
此固定场景，不是“所有阴影必须亮”的通用规则，也不进入生产 shader。

第一轮仅放宽半球筛选的诊断实验把主斑块比值提高至 5.84%，但真实截图仍有黑块，
因此未采用；同步收紧了原来不足以代表视觉改善的 5% 回归界限。

## 定点根因证据

主斑块体素 `(265,169,230)`，GPU 法线约 `(-.757,.106,-.645)`；与真实保存体素
邻域计算一致，仅有正常 Oct16 量化差异。木材 albedo 为
`(.590619,.434154,.107023)`，材质及法线均非异常零值。

| Probe | 环境光 RGB 和 | 原半球判定 | 当前配置精确线段 |
| --- | ---: | --- | --- |
| 2405 | 1.759727 | 拒绝，alignment = -0.35478 | 畅通，CPU/GPU 一致 |
| 2116 | 1.772503 | 接受 | 被树枝遮挡，CPU/GPU 一致 |
| 2133 | 1.671560 | 接受 | 被树枝遮挡，CPU/GPU 一致 |

2405 位于 `(.998046875,.623046875,.998046875)`。旧修复对不可信 contribution
提前返回，根本到不了这颗 probe 的可见性恢复。保留的正面 probe 距离矩可见度仅
`2.45e-5` 和 `0.01044`，再经低置信度归一化，导致目标几乎无光。

GPU 逐 probe 读回在 `blacky/diagnostic-probes/probes.json`；CPU 使用实际游戏保存的
`target/summer-evidence/tree.rflterrain`，其树编译统计、边界与本次一致。法线读回
在 `blacky/diagnostic-normal/`。这些 RFIRR 平面被诊断性改写，**不是正常辐照度验收**。
所有 `[BLACKY_DIAG]` shader 插桩已移除。

## 结果与边界

| 实际生产路径 | 修改前 | 当前候选 |
| --- | ---: | ---: |
| Blacky 指定近黑斑块 | 10 / 10 判红 | 0 / 10 判红 |
| 主斑块 RGB 和中位数（65 样本） | 0.004667 | 1.759727 |
| 邻近正常体素（80 样本） | 1.602070 | 1.597947 |
| 主斑块 / 邻近体素 | 0.002913 | 1.101243 |
| 原启动镜头精确零值回归 | 旧基线曾有 103 样本 | 当前 0（49,625 个树区域采样） |

额外检查中央区域原来很暗的 15 个体素，14 个恢复到有明确光照的范围；
`(256,163,239)` 没有精确可见探针，仍保持原来的 0.005748，没有强行补光。
`central-patch-comparison.json` 保存逐体素前后值。真实画面仍保留叶片投影、
背光面和局部暗缝。用户对当前候选的再次视觉验收仍需单独记录。

## 验证

- `CARGO_BUILD_JOBS=2 cargo fmt --check`、`cargo check`：通过，生成文件无 diff。
- `python3 scripts/run_slang_tests.py`：8/8 通过。
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`：63/63 通过。
- `CARGO_BUILD_JOBS=2 cargo test`：915 passed、1 failed、1 ignored。唯一失败为已有
  `app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`，
  其断言仅有一个 `snapshot`，用户配置现在有 5 个镜头（包含 blacky），`left:5/right:1`。
  不修改或丢弃用户镜头来规避这个既有 fixture 假设。
- `CARGO_BUILD_JOBS=2 cargo test -- --skip app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`：915 passed、0 failed、1 ignored、1 filtered out。
- `flock --close /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5`：通过；`--latest-log` / `--tail-latest-log 200` 确认 shutdown failures=0、正常退出，无 ERROR/panic/VUID/device-lost。
- `sealed`、`portal`、`walls` 的生产 published 捕获均通过非负/有限检查；sealed 的
  518,400 个环境光采样全部精确为零，portal 的 p99 亮度高于 0.1。命令和分析结果
  分别在 `blacky/guard-{sealed,portal,walls}/`。这是基础遮挡回归，不等同于完整
  DDGI 多密度、多 epoch correctness/reference 矩阵通过。

所有 app/GPU 运行都使用 `/tmp/re-flora-summer-gpu.lock` 串行保护。背景验证均 hidden，
没有再次打开可见游戏。`cargo` 统一 `CARGO_BUILD_JOBS=2`，target 仅用当前 worktree。
标准 smoke 使用 `flock --close` 避免 cargo 启动的 sccache 服务继承 GPU 锁。

## 实际证据路径

以下均相对当前 worktree，未提交的大体积运行证据保留在 target：

- `target/summer-evidence/blacky/before-image/final.png`：旧修复在 Blacky 的稳定黑块截图。
- `target/summer-evidence/blacky/exact-supported-image-full/final.png`：新候选，同镜头、完整叶片和特效。
- `target/summer-evidence/blacky/final-repro-{red,green}/`：同一回归入口的实际生产浮点捕获与日志。
- `target/summer-evidence/blacky/final-startup-green/`：原启动镜头回归。
- `target/summer-evidence/blacky/{final-fmt,final-check,cargo-test,cargo-test-filtered,slang-tests,capture-tests,hidden-smoke}.log`。
- `target/summer-evidence/blacky/hidden-smoke-{latest.txt,tail.log}`：当前 worktree 的日志助手输出。

## 运行成本与交接

当前算法每个接收点只执行一轮最多 8 个 probe 的精确验证。替代了先前 canonical /
平滑两轮验证；没有增加缓存、异步模块或改变 probe 密度。当前候选的 release 成本
对照单独补充，09-07 的 1.447 → 2.448 ms 属于旧镜头、旧分辨率、旧修复，不能用于
描述本次候选。

生产改动限于 `shader/slang/ddgi_query.slang` 和共享文件 `src/tracer/mod.rs` 的一条
能力日志，另更新回归脚本及本报告。没有修改 `src/gui_adjustables.rs`、环境光权威
结构或生成文件。用户新增 `blacky` 的相机配置改动保持原样、未混入实现提交。

尚未覆盖所有树 seed、镜头、编辑操作、probe 密度、其它 GPU，以及完整 DDGI
correctness suite。几何证据使用现有配置的偏移起点；本次没有重新设计偏移策略。
