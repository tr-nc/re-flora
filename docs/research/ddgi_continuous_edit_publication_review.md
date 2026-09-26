# DDGI 连续编辑、发布屏障与测试合理性复核

## 结论

**保留 `5b06e91a` 的 DDGI 修复，保留树综合 smoke；不删除点光源，不放宽 owner 校验。**
实测没有退回“停止编辑后才更新”。这次还发现并修复了一个独立的测试隔离问题，增强了
持续编辑的验收，并补上了持续编辑与点光源变化交叉的真实应用回归。

尚不能据此断言光照数值正确、间接光视觉响应足够快，或所有 GPU 上都没有性能回归。
特别是高密度探针的约 1.25 秒发布间隔，修复前就存在，仍是值得讨论的响应性问题。

## 1. 实测方法

- 工作树：`re-flora-agent-tree-edit-stall`，未合入 main。
- 旧 DDGI：`2f6d9874`；新 DDGI：加上 `5b06e91a` 的发布保护。
- 最终配对实验两侧均使用相同的测试场景隔离和交叉 fixture。旧侧仅移植了
  `src/app/core/climbing_plants.rs`、`src/app/core/environment_lighting_test_scene.rs`、
  `src/cli.rs`，没有移植 `src/ddgi/` 修复。
- 使用独立复制的 Release 可执行文件；临时 detached worktree 构建旧侧。测试不切换当前分支。
- Linux / NVIDIA GeForce RTX 3060 Ti；自动选择 FIFO；2560×1440；隐藏、静音、相同相机和 GUI。
- GUI SHA-256：`c45e9ca4a8bb6fa10b88818721b2ba455532d28c1fddbb47659db5c742367256`。
- 每组 40 次真实地形编辑，相邻操作至少 100 ms，约持续 4.7 秒；不等待 DDGI 完成才编辑。
- spacing 32 和 16 各三组配对，第二组倒转新旧执行顺序。GPU 锁串行运行；并非隔离整机的性能实验。
- 日志记录消费者真实 staging promotion，而不只是调度器完成；统计编辑期间的不同地形版本、
  发布间隔、包含首尾等待的最长无发布时间，以及停手后的最终地形版本追赶时间。
- 原有像素值只作诊断，不设亮度下限。黑暗可以是物理上正确的结果。

主要命令：

```sh
python3 scripts/check_ddgi_sustained_edits.py target/ddgi-review/terrain32
python3 scripts/check_ddgi_sustained_edits.py target/ddgi-review/terrain16 --spacing 16
python3 scripts/check_ddgi_sustained_edits.py target/ddgi-review/lights32 --vary-lights
python3 scripts/check_ddgi_sustained_edits.py target/ddgi-review/lights16 --spacing 16 --vary-lights
# 配对旧/新可执行文件时：
python3 scripts/check_ddgi_sustained_edits.py <output> --binary <release-executable> --spacing 32
```

`--vary-lights` 选择 `terrain-edits-sustained-lights`：第 1、5、9、…、37 次编辑交替加入/删除
一个正常注册的点光源，合计十次变化，最后无该点光源。没有专门等待“安全帧”来绕过问题。

## 2. 隔离后的连续编辑结果

下表区间是三个 run 的最小到最大值，不是统计置信区间。发布间隔为**各 run 的中位数**。
“追赶”指最终版本完成发布，不指迭代已经收敛。

| 探针间距 | DDGI | 编辑期间发布次数，每轮 | 发布间隔中位数 | 最长无发布时间，含首尾 | 停手后最终地形追赶 |
|---|---|---|---|---|---|
| 32 | 旧 | 22 / 22 / 22 | 205–206 ms | 360–367 ms | 316–332 ms |
| 32 | 修复 | 22 / 22 / 22 | 206 ms | 353–361 ms | 298–335 ms |
| 16 | 旧 | 3 / 3 / 3 | 1248.5–1252 ms | 1344–1366 ms | 1582–1615 ms |
| 16 | 修复 | 3 / 3 / 3 | 1252–1256 ms | 1348–1361 ms | 1613–1638 ms |

默认密度修复前后的 run 内 p95 发布间隔均约 207–210 ms。高密度每轮只有两个间隔样本，
不能把它的 p95 当可靠尾延迟估计。高密度追赶时间有约几十毫秒的差别，样本不足以宣称
严格等价或零成本；但没有停止发布，也没有数量级变慢。

### 连续编辑 + 点光源变化

| 探针间距 | 修复版三轮结果 | 编辑期间发布次数，每轮 | 光照版本推进 | 停手后地形+最终光照追赶 |
|---|---|---|---|---|
| 32 | 全通过 | 22 / 22 / 22 | 三轮都覆盖 revision 2 到 11 | 280–315 ms |
| 16 | 全通过 | 3 / 3 / 3 | 三轮均依次发布 revision 2、4、7 | 1565–1623 ms，最终 revision 11 |

高密度下中间光照状态被合并，这是有限预算下的追赶，不保证每次快速闪烁都进入间接光场。
最终地形 revision 42 与最终光照 revision 11 都有真实发布证据。

同一交叉 fixture 的**旧 DDGI 负对照失败**：第 5 次编辑移除点光源，约第 6 次编辑后出现原错误：

```text
DDGI staging publication lost its owner radiance tuple
```

这说明它不是树 smoke 特有的非法调用，也不是只靠单元测试构造出的失败。

## 3. 测试本身确实存在的问题

### 已修：另一套演示场景抢占相机和地形

最初的配对日志仍显示持续 publication，但实看 `editing.png`，画面是攀爬植物演示，不是天窗。
`update_climbing_plants` 会自动创建 fixture 并移动相机，晚于测试场景和截图相机初始化。
固定像素采样甚至测到天空（约 136.667/255），所以“截图已保存 + 像素有值”不是有效场景证据。

修复：显式 test scene 已拥有 capture scene 时，不运行自动攀爬演示。普通游戏不变。
runner 也会拒绝包含该演示建场日志的证据。原日志在新检查下明确 RED；修复后截图恢复为天窗
内部，采样约 90/255。此数值不是新的亮度目标。

最初带干扰的 `*pair*`、`fixed*-run1` 等数据保留作诊断，**不作为最终配对或视觉验收**。
最终配对只用 `clean-*`，交叉场景用 `cross-fixed*-run*`。

### 已增强：原“至少两次发布”的判据过弱

原 runner 未检查：重复日志是否冒充推进、是否只发布了编辑前的旧场、停止后是否追上最终版本。
现在检查 40 次顺序编辑、严格递增的地形版本、至少两个实际覆盖编辑的不同 publication、
最终版本追赶；输出首个有效发布、间隔中位数/p95/最大值和首尾空窗。
交叉模式另外要求十次有序光源切换、期间光照版本推进、最终地形与最终光照同时追上。
十一项快速 Python 单元测试包含负例，不把 GPU 压测加入 `cargo test`。

**尚未设置普适的最大等待阈值。** 两次发布只是防饥饿底线；响应性预算应按探针密度、目标设备
和玩家预期制定，不能把本机这次结果自动当成标准。

### 应保留但准确命名：树 smoke 的光照检查

`RasterTreeSmoke` 的点光源走正常 light registry，操作合理，能发现子系统交叉问题。
但帧 21 加灯、帧 25 校验不是物理光照响应的完整断言。当前
`RasterTreeMesh::validate_lighting_cache`（`src/tracer/raster_tree.rs`）只要求：

- RGB/置信度有限、非负；
- 置信度与 hybrid 模式的 metadata 相符。

即使缓存 RGB 全是零，也可能通过。这适合作为 GPU 数据/生命周期 smoke，**不能宣称已验证
点光源正确照亮树，或间接光在某个时限内变化**。不应删掉它；应另设定量光照响应测试。
测试按固定帧触发也不保证每个设备都会撞上同一 DDGI 交错，因此保留快速调度回归和新交叉场景。

## 4. 为什么发布保护不等于等待编辑停止

当前保留的两项原则：

1. `terrain_refresh.rs::token_can_promote` 允许旧的完整地形候选在新编辑排队时发布，不坚持
   “必须等于最新请求”。`mark_promoted` 保留之后的编辑请求。
2. `runtime.rs::claim_transport_work` 仅在 `completed_staging_publication` 存在时不再领取下一轮。
   下一帧 `Tracer::update_buffers` 开头完成物理 promotion；发布或 obsolete retirement 清除此状态。

它是在同一 builder 被复用前保住已完成结果的 owner tuple，不是等待鼠标释放、所有工作排空或
光照收敛。没有添加 sleep、CPU fence wait 或 device-idle 等待。

该保护的潜在代价是不能在待发布 staging 上提前启动下一轮。配对场景没有观测到有意义的发布
频率下降，但这不是所有工作负载的零成本证明。不能单纯把 promotion 移到当前帧更晚的位置：
还须核对该帧已准备的 shading constants、descriptor generation 和资源退休的一致性。

## 5. 其他实现如何处理动态更新

以下为实际阅读的一手来源，不把外部实现的做法等同于对本项目的直接验证。

### NVIDIA DDGI / RTXGI：有预算的持续更新 + 旧场反馈

DDGI 作者的实现说明明确使用**前一帧 probes**计算 ray hit 的间接光，跨帧传播多次反弹；
新旧结果通过 hysteresis 混合。高历史权重更稳定但响应更慢。[1]

RTXGI 的 UE4 集成按 weighted round-robin 更新 volume，有每帧 ray budget；文档明确把预算和
priority 与 light lag 联系起来，并非先等待整个世界停止变化。[2]

RTXGI `ProbeBlendingCS.hlsl` 检测大幅照度降低时减小 hysteresis，对大幅增亮限制单次变化；
全黑/清空历史首次更新可设零历史权重。[3] 这是响应性与噪声的取舍，不是删除版本/同步约束。

Vulkan `UpdateDDGIVolumeProbes` 的 dispatch 和 shader write→read barrier 保持更新阶段的顺序。[4]
**本次错误是 CPU 发布所有权错序，不是证据表明少了一道 GPU barrier。** 不能靠照搬 barrier 修复。
RTXGI 也没有与本项目完全相同的 terrain-version + immutable publication 合约。

### Unreal Lumen：缓存更新与可见响应是两个问题

Epic 文档说明 Lumen 使用多级缓存，局部变化通常较快，全局变化可能耗时数秒；
提高 Scene Lighting / Final Gather Update Speed 会增加 GPU 成本。[5]
借鉴点是分别衡量更新频率、可见响应、收敛与成本，而不是用“不崩溃”代替全部验收。
这不意味着本项目任何秒级局部延迟都可以接受。

### ADGI：把更多预算给发生变化的区域

《Adaptive Dynamic Global Illumination》通过检测 lighting/visibility 的时变性，向相关区域分配
更多采样资源，也讨论了探测成本、快速变化时的跟踪困难等限制。[6]
这是改善高密度动态响应的研究方向，不是现在应立即照搬完整 MCMC 系统的理由。

## 6. 当前实现可改进之处，按优先级讨论

### A. 优先补“真实间接光响应”的可证伪测试

保留现在的三层测试：纯逻辑 owner guard、树综合 smoke、真实持续编辑交叉测试。
另加稳定接收面/间接-only 或拆分直接与间接的读回：开灯和关灯两个方向都测，记录首次有效
变化、达到稳定参考范围的时间和关闭后的残留能量。使用匹配静态控制与负对照，避免阳光、
曝光、错误相机或直接光掩盖 DDGI 故障。先定义允许的暂态近似，不要求动态过程中逐帧等于静态真值。
当前测试还只有约 4.7 秒的手势和两档密度，不是长时间 soak、多区域刷动或所有时序的证明。

### B. 高密度瓶颈主要在整场完成粒度，而不是这一帧保护

当前每帧固定 32,768 rays、64 rays/probe，即 512 probes/frame（`src/ddgi/config.rs`）。
实测 volume：

- spacing 32：4,913 probes，至少 10 个 trace batches，单 volume 约 40.47 MiB；
- spacing 16：35,937 probes，至少 71 个 trace batches，单 volume 约 290.79 MiB。

在 FIFO 运行下，多帧整场扫完再发布本来就会产生百毫秒到秒级间隔。
现有 priority 选择批次起点，但不会减少本轮需要完成的批次数；局部优先不自动等于局部提前可见。

先测 publication 年龄、局部脏区范围和 source/候选 GPU 成本；可研究小的近场 volume、分区完整
publication、或有公平性保底的自适应预算。**不要直接把半更新 atlas 暴露出去**：分区发布需把
geometry/radiance/visibility/sky/descriptor 归属和过渡行为一起定义清楚。
简单加倍 ray budget 可能减小延迟，也可能损害帧率，应以可见效果和 Release 实测分阶段决策。

### C. 把“等待发布”变成更明确的状态，降低调用顺序负担

目前 runtime 已拥有 `completed_staging_publication` 和 publication permit，保护也放在正确的
runtime 领取任务入口，而非只在树 smoke 特判。后续可以让 builder 的 `Building / AwaitingPromotion /
Active` 转移更明确，或提供一个封装 completion→publication→schedule 的帧推进 interface，减少
tracer 调用顺序知识。不要再加一个独立 bool，造成第二份状态真相。

只有实测表明这一帧交接值得优化，才考虑让 completed publication 自持不可变 radiance snapshot、
sky/atlas 等资源，并允许下一轮独立构建；那会增加资源生命周期与内存复杂度。
当前 active/staging 高密度资源已较大，不建议为了消除未量出的瓶颈立即增加第三份资源。

### D. 文档与注释需要继续消除过期合约

`src/ddgi/config.rs` 中部分 local-recovery 常量注释仍写“私有候选稳定后才可见”，与现在
`promotion_is_ready = published.is_some()`、完整 e0 逐步发布的政策不一致。旧 acceptance 脚本
也有历史合约，不能未经检查就当成回归标准。此次未扩大为全仓迁移。

已有 `DdgiRadianceHistoryPolicy` 会根据来源到目标的实际变化和 elapsed time 调整/重置历史；
已有 terrain/light/camera probe priority。后续改进应在这些真实策略上做证据驱动优化，
不应声称目前完全没有变化检测、历史控制或优先级。

## 7. 验证、改动与证据

本轮提交：

- `cfbaa702`：持续编辑日志断言、延迟报告、`--binary` 和纯逻辑负例。
- `b22fa14c`：隔离自动攀爬演示；拒绝已知场景污染。
- `51021889`：持久化交叉场景与 `--vary-lights` 验收。

验证：`cargo fmt --check`、`cargo check`、`cargo test`（1172 main + 4 library 通过，4 ignored）、
十一项新增 Python 测试、定向 ruff、CLI help、`git diff --check` 通过。
系统没有 `pyright`，类型检查未运行。第一次长配对工具调用超时，未完成的最后一轮单独重跑；
最终表格使用完整、隔离后的新矩阵，不使用部分运行。

原完整树 smoke + resize 和默认 hidden/mute Release 运行通过，检查 per-worktree log helper，
无 ERROR/panic/VUID，`failures=0`。没有启用 Vulkan validation layer 的额外声明。
未改 shader、生成文件或保存的 GUI/相机；未启动可见游戏；未进一步修改 DDGI 算法。

证据目录：`target/ddgi-publication-review/`

- `clean-{baseline,fixed}{32,16}-pair{1,2,3}/`：最终十二轮配对；
- `cross-fixed{32,16}-run{1,2,3}/`：六轮交叉验证；
- `cross-baseline32-red/`：旧 DDGI 的原错误负对照；
- `baseline-harness.patch`：旧侧移植的测试/隔离改动；
- 每个目录都有 `report.json`、完整 console/canonical log、真实 log 路径，成功运行有 `editing.png`；
- `report.json` 保存精确命令、二进制 SHA-256、GUI SHA-256 与所有统计；`bin/` 保存对应二进制。

最终原树 smoke：`target/re-flora-logs/re-flora-20260926-204246.323-524879.log`。
最终默认运行：`target/re-flora-logs/re-flora-20260926-204251.923-527173.log`。
临时 baseline worktree 清理，证据和独立二进制保留。

## 一手来源

1. Morgan McGuire 等 DDGI 作者，[Dynamic Diffuse Global Illumination overview](https://morgan3d.github.io/articles/2019-04-01-ddgi/overview.html)，Implementation 的三个阶段、previous-frame feedback 与 hysteresis。
2. NVIDIA，[RTXGI UE4 plugin README](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/ue4-plugin/4.27/RTXGI/README.md)，Update Priority、Probe Update Ray Budget、Probe History Weight。
3. NVIDIA，[ProbeBlendingCS.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl)，历史权重、irradiance/brightness threshold 与 lerp。
4. NVIDIA，[DDGIVolume_VK.cpp](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/gfx/DDGIVolume_VK.cpp)，`UpdateDDGIVolumeProbes`。读取 main 时 SHA 经 `git ls-remote` 固定如上。
5. Epic Games，[Lumen Global Illumination and Reflections](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine)，Lumen Lighting Update Speed。
6. [Adaptive Dynamic Global Illumination](https://arxiv.org/html/2301.05125)，§3、§4 的动态检测/资源分配与 §6 的限制。
