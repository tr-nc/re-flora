# 蝴蝶方块飞行 A/B 候选交接

2026-09-12；worktree `re-flora-agent-butterfly-block-flight`，分支 `agent/butterfly-block-flight`。
开始时工作树干净，base 为 `9ca488e9ed9b623691596d6dfdfe4a0f096c16db`。

更新：用户试玩后增加第二轮分步/顿挫控件，第三轮明确改为启动默认 B，并加入独立风漂移和较慢自主飞行；最新默认与验证见本文末尾。前面的历史证据保持原记录。

## 入口与实际效果

按 **R → Debug Panel → Butterflies → B: Darting color blocks (A/B experiment)**。

- 未勾选 A（默认）：原蝴蝶贴图、五帧动画、朝向选择与 worm-noise 飞行。
- 勾选 B：原有活体立即变为单色方块，启用短促、非周期加速度机动；再次取消回到 A。切换不重建世界、不补生成、不换 palette、不重置位置/寿命。
- checkbox 是运行时 App 状态，不写 `config/gui.toml`，重启回到 A。普通启动不需要任何 CLI 或环境变量。
- B 复用普通粒子的白色方块层和 `STANDARD_PARTICLE_SIZE = 1/256`（1 voxel）；原 Size 滑条仍控制 A 的 sprite。B 明显小于当前 A 的 0.03 world-unit sprite，远景只有数像素，不应把这解释为渲染丢失。
- 单个方块固定一种颜色：复用该个体原 `texture_variant` 的七色 palette 分布，取 `mid_shade` 并做 sRGB→linear；仅原有出生/死亡 alpha 渐变。B 不采样蝴蝶动画层、不新增动画素材或 shader。

## 审计根因与模型选择

旧模式不是位置正弦：方向来自两层归一化 Perlin worm noise，`WORM_STEP_LEN=0.15` 同时用于相位推进和速度尺度。当前 world tick 0.05 s、交替规划桶，以及 `ParticleUpdateConfig(0.1, 2)`，使单个蝴蝶约每 0.1 s 更新一次位置；Free 模式每次更新还乘 0.92 阻尼。近恒速、平滑方向噪声和低频位置保持，缺少独立的短促加减速/改向事件。A 保留这些行为供对比。

生成权威不变：既有 emitter、spawn hazard/RNG、每秒刷新一次的非草类 flora 基部与 committed 树叶位置、同一 ParticleSystem slot/lifetime/capacity。硬上限仍是 `2 * CHUNK_DIM.x * y * z`，当前 2×2×2 为 16；没有平行生成器。两模式的后续存活数可以因不同飞行/原模式碰撞淘汰而不同，但不改变上限、出生规则或 palette 抽样。

使用 research 技能整理的[一手研究与适用边界](research/butterfly_block_flight_motion_research.md)支持“连续轨迹 + 速度波动 + 快速机动”，不支持所有蝴蝶共有某种随机时序定律。特别是上冲证据部分来自诱发逃逸；下落仍是用户观察驱动的视觉假设。

B 为每个既有个体增加独立种子化运动状态，不消耗原生成 RNG：

- 固定 120 Hz 积分，不受旧 world tick/分桶控制；每次最多追赶 0.25 s，避免卡顿后巨步。
- 巡航 0.10–0.16 world/s；机动持续 0.07–0.20 s，普通间隔 0.12–0.85 s，偏态有界重采样，没有固定节拍。
- 28% 机会形成 2–3 个相关反向子脉冲，额外间隔 0.035–0.11 s；低概率向上/向下分量。角度的 `sin_cos` 仅做转向几何，不是随时间摆动。
- 速度上限 0.30 world/s，垂直速度 0.20；加速度上限 1.8，jerk 上限 14。改变目标加速度，再积分速度和位置，不随机重写位置。
- 从原出生栖息地设置软恢复范围（水平 0.32、垂直 0.20）；结合世界软排斥与下一步硬位移约束、原 CPU terrain ray 前瞻/扫掠约束。接触可截断速度，非接触状态受有限加速度/jerk 约束。

以上数值、事件概率与等待分布是待视觉评审的工程参数，不是论文测出的游戏比例常量。

## 验证与可复查证据

已通过 `cargo fmt --check`、`cargo check`、`cargo test`：**931 passed / 0 failed / 2 ignored**。最终全量输出为 `target/butterfly-all-tests.log`；另有 12 项 butterfly 过滤测试全通过。新增覆盖 A→B→A checkbox 的 egui 指针点击、slot/palette/寿命/生成 RNG 保持、禁用与寿命回收、20/30/60/120 fps 与不同 world tick 的相同轨迹、12 个固定种子各 30 s 的连续/有界/非周期检查、世界/地形接触。测试发现并修正了 f32 固定步长转 f64 时的累计少一步问题，未放宽帧率等价断言。

真实 GPU 运行全部使用 `flock --close /tmp/re-flora-summer-gpu.lock`；没有指定 present-mode、没有启动可见游戏或结束其他进程。

```sh
flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_BUTTERFLY_BLOCK_FLIGHT_SMOKE=1 flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5

env -u WAYLAND_DISPLAY RE_FLORA_BUTTERFLY_REVIEW=switch flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --windowed --denoiser-bench blacky target/butterfly-review/candidate.toml --denoiser-bench-warmup-frames 600 --denoiser-bench-frames 480
env -u WAYLAND_DISPLAY RE_FLORA_BUTTERFLY_BLOCK_FLIGHT_SMOKE=1 RE_FLORA_BUTTERFLY_REVIEW=block flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --windowed --denoiser-bench blacky target/butterfly-review/block.toml --denoiser-bench-warmup-frames 650 --denoiser-bench-frames 300
```

前两次默认 Wayland 的 `--hidden` 按现有程序行为最小化；后两次移除进程内 WAYLAND_DISPLAY，使用 X11 真隐藏。所有运行退出码 0，`SHUTDOWN phase=complete failures=0`，日志无 ERROR/panic/VUID；保留了既有 6 条编译 warning，测试输出仍有 ALSA 设备枚举告警，不宣称音频验收。

用本 worktree 的 `--latest-log` / `--tail-latest-log` 核对过日志：

| 运行 | `target/re-flora-logs/` 中日志 |
| --- | --- |
| A smoke | `re-flora-20260912-131355.099-305210.log` |
| B smoke | `re-flora-20260912-131405.447-305368.log` |
| A→B→A candidate | `re-flora-20260912-131645.301-307474.log` |
| B 连续飞行 | `re-flora-20260912-131742.899-308694.log` |

首轮 `switch.toml` 在固定第 240 帧选镜头时尚无自然蝴蝶，未作飞行画面验收。修正诊断观察器为等待第一只完成淡入的自然个体，之后固定镜头；没有调整自然生成率、数量或个体位置。该诊断环境变量仅辅助隐藏运行，不能替代 checkbox。

有效画面均为 2560×1440 真实游戏帧，每 3 个渲染帧保存一张 PNG：

- [A→B→A 视频](../target/butterfly-review/candidate.mp4)：160 张源 PNG；第 634 simulation frame 选中主体，754 后切 B、874 后回 A。检查了 `candidate.artifacts-9NoRBh/frame-0135.png`（A）、`0165/0270`（B）、`0285`（A 恢复）。该段 B 一直是同一 palette=3 的存活个体，切换处数量 1、alpha=1。
- [B 连续飞行视频](../target/butterfly-review/block.mp4)：100 张源 PNG；检查了 `block.artifacts-vnFKQf/frame-0060/0090/0120/0180.png` 以及 `0120/0123/0126/0129/0132/0135` 连续采样。可见紫、白、蓝方块，主体连续移动、改向，没有翅膀帧动画；绿色方块是原落叶，不是新增蝴蝶。
- 两个 `.toml` 中的 `keyframe_paths` 枚举全部原 PNG。视频由本地 `target/butterfly-review/encode_review.py` 依据日志 dt 合成，不补间、不生成画面；播放时序量化到 40 ms。视频约 18.76 / 11.72 s；报告 capture_seconds 还包含截图编码/发布开销，不能作飞行时间或性能数据。

日志量化：candidate 中 A 有效相邻样本 635 次，381 次位置保持；B 119 次全有位移，最大帧间位移 0.01250 world，速度约 0.105–0.300。B 独立运行日志 711 帧，最高 3 只、palette indices 1/3/5，位置/速度有限且在世界边界内；垂直速度 -0.127 到 +0.172 world/s。这是本次样本证据，不是总体分布统计。

尚未验收：用户主观的“活力/离散感”和远近尺寸、可见游戏人工点选、16 只满负载/更大世界性能、长期复杂地形下的碰撞质量。地形约束是点粒子的 CPU ray，不是翼展或完整体积碰撞；软栖息地范围也不是硬球壳。现有照片/日志和确定性测试足以交付可玩候选，不代表视觉或性能最终批准。

## 文件、生成物与集成重叠

- `src/particles/butterfly_flight.rs`：B 运动状态/积分/纯逻辑测试。
- `src/particles/emitters.rs`：同一 emitter 上的模式同步、固定步进、人口契约测试。
- `src/particles/system.rs`、`src/particles/mod.rs`：共享 slot 生命周期与 B 方块快照/颜色、导出模式。
- `src/app/core/particles.rs`、`src/app/core/mod.rs`：Debug checkbox、App 接线、隐藏截图观察器与 GUI 点击测试。
- `src/app/core/denoiser_bench.rs`：复用已有逐 3 帧截图路径。
- `src/tracer/mod.rs`：仅新增 ButterflyBlock 的白色方块层/粒子批次分支（4 行）。
- 本报告与 `docs/research/butterfly_block_flight_motion_research.md`。

生成文件没有差异：`gpu_structs.rs`、`gui_adjustables_gen.rs` 均只由 cargo 构建生成，没有手改；`config/gui.toml`、素材、shader、`resources.rs` 未修改。PNG/视频/日志/编码辅助脚本均留在忽略的本地 `target/` 下，未作为源码提交。

**与主工作区落叶共享着色重叠仅在 `src/tracer/mod.rs::upload_particles`。** 此分支仍是旧的 36-byte ParticleInstanceGpu ABI，没有覆盖或重做 `leaf_optics`、52-byte ABI、`particle_lod_textured.vert.slang` 或 `leaf_lighting.slang`。将来集成时保留主分支 leaf_optics 上传，新 ButterflyBlock 走普通非落叶参数（不应用 leaf optics），再由 cargo 重生成结构；不能把 ButterflyBlock 因复用白层而当成落叶。未 merge/cherry-pick/push，未管理其他 Worker。

提交：`b8625310` 研究；`13a52203` 功能/测试；`400b2881` 自然主体等待与采集验证。报告作为后续独立提交。

模型审计边界：任务中途发生模型切换；本次没有可用的会话级证据确认全程是用户指定的 `gpt-5.6-sol / xhigh`，因此不把该项列为已满足。

## 用户调参（第二轮）

仍在原 B checkbox 正下方，实时作用于现有蝴蝶；A 时禁用，不影响 World Tick、落叶或其他粒子。所有值是临时 App 状态，重启恢复默认，不写 GUI 配置；修改会写 `[BUTTERFLY_AB][TUNING]` 日志，便于保留试玩反馈。

| 控件 | 范围 / 默认 | 含义 |
| --- | --- | --- |
| Position step interval (ms) | 0–160 / 60 | B 显示位置的采样保持间隔。0 为首轮连续显示，越大分步越明显。每个体间隔有 ±20% 变化。 |
| Maneuver tempo (x) | 0.25–4 / 1 | 越高，机动事件更短、更频繁；不缩放物理时间或寿命。 |
| Vertical movement (x) | 0–4 / 1 | 上下自主机动强度；0 仍保留地形避让和栖息地恢复。 |
| Turn sharpness (x) | 0.25–4 / 1 | 越高，加速度响应越快；越低，转向更柔和。 |
| Flight speed (x) | 0.25–2.5 / 1 | 巡航、机动力与速度上限倍率；不改生成率/寿命。 |

`Reset B flight controls` 一键恢复上表默认。优先只调位置步进间隔来找顿挫节奏，再调其余项；这些数值是操作范围，不是已经通过用户视觉验收的最佳值。

实现把 120 Hz 连续物理积分与 B 的显示位置采样分离。显示位置只能取真实轨迹点，不随机偏移；轨迹偏离上次显示位置达到 0.025 world 时提前发布（最多再加一个物理子步的距离），限制长间隔/高速组合的大跳。显示采样使用独立 RNG，不消耗飞行/出生 RNG。降低速度滑条会立即收紧速度上限，但不改写位置。切回 A 使用当前真实位置，不使用 B 的旧显示缓存。

验证：`cargo fmt --check`、`cargo check`、`cargo test` 通过，**936 passed / 0 failed / 2 ignored**。新增测试覆盖五个滑条与 reset 的 egui 指针点击、运动参数实际效果、极端值/非有限输入、变化间隔、显示采样不改变真实轨迹/出生 RNG/存活数、A 不调用 B 的显示覆盖函数。输出 `target/butterfly-tuning-all-tests.log`。

两次 GPU 验证仍使用 `env -u WAYLAND_DISPLAY ... flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute ...`，没有启动可见游戏：

- B 0.5 s smoke：`target/re-flora-logs/re-flora-20260912-133918.306-330644.log`。
- 自然主体实时调参：`RE_FLORA_BUTTERFLY_REVIEW=tuning` 与 `RE_FLORA_BUTTERFLY_BLOCK_FLIGHT_SMOKE=1`，`--windowed --denoiser-bench blacky target/butterfly-review/tuning.toml --denoiser-bench-warmup-frames 650 --denoiser-bench-frames 360`。日志 `re-flora-20260912-133951.067-331260.log`，120 张 2560×1440 PNG 在 `target/butterfly-review/tuning.artifacts-GV0ueb/`，检查了连续采样 `0180/0183/0186`；未将截图检查等同于人眼动态体验。

第二次运行在 726/816/906 simulation frame 后切为连续、160 ms（同时增强机动）、恢复 60 ms。排除实际速度近零与数量改变的帧间样本，显示位置保持次数分别为初始 60 ms 的 41/178、连续的 0/190、160 ms 的 157/232、恢复后的 157/396。不同段个体与运动状态不同，不作严格性能或主观自然度比较；只确认控件切换实际生效。最高 6 只，状态有限；两次退出码均 0、shutdown failures=0，日志无 ERROR/panic/VUID。

本轮仅改 `src/particles/{butterfly_flight,emitters,mod,system}.rs`、`src/app/core/{mod,particles}.rs` 和本报告。没有生成文件/config/shader/GPU ABI 的改动，也没有扩大与主工作区落叶着色的重叠。动态效果由用户继续调参验收。

## 默认 B 与独立风漂移（第三轮）

用户明确“more option”指 B 作为默认，而不是增加另一套菜单。现在普通启动即勾选 **R → Debug Panel → Butterflies → B: Darting color blocks (A/B experiment)**；取消勾选仍立即恢复原 A 的素材和运动（不添加风漂移）。这些临时选项仍不写配置，重启回到 B。

- 原 `Flight speed (x)` 改名 `Self-flight speed (x)`，范围仍 0.25–2.5，默认从 1 降为 **0.65**。只缩放自主巡航、机动力及相对空气的速度限制。
- 新增 `Wind drift (x)`，范围 **0–3、默认 1**。0 会让已有漂移平顺衰减；不改变全局风强度、植物或其他粒子。
- 其余四个控件及默认值不变。Reset 恢复自主速度 0.65、风漂移 1 和其余默认值。

此前 B 完全由自主速度积分，没有读取现有风场。现在每个 120 Hz 子步在个体实际物理位置调用同一个 `WindFieldFrame::sample_world`；该快照来自现有 `WindPrototype`，同时包含自然背景风与 Wind 工具释放的局部风。没有新增风生成器、蝴蝶生成器或改变 World Tick/数量/栖息地权威。现有风场只有水平分量，不虚构垂直阵风。

模型将自主空速与风漂移速度分开：先对自主分量施加速度限制，再叠加风分量，避免降低自主速度也把随风位移一起截小。风场强度是工程单位，映射系数为 `0.06 world/s/strength`，漂移速度上限 0.45 world/s、响应时间常数 0.25 s、加速度上限 0.60 world/s²；强风变化不会直接传送位置。世界/地形接触同时裁剪两部分速度，原栖息地恢复仍作用于实际位置，因此遇障/离巢后最终轨迹会自然耦合，不宣称任意场景下两条轨迹数学独立。这些参数是可玩候选调校，不是新的生物学定律。

验证通过 `cargo fmt --check`、`cargo check`、`cargo test` 和 release 构建：**939 passed / 0 failed / 2 ignored**。新增三个确定性风测试覆盖局部采样、零风倍率等价于静风且不消耗出生 RNG、自主速度不截断风速、独立倍率、强风反转/非有限输入/衰减/世界边界；更新六个滑条与 reset 指针点击测试，以及默认 B → A → B checkbox 点击测试。完整测试输出 `target/butterfly-wind-all-tests.log`。仍有既有编译警告及测试 ALSA 枚举告警，未作音频验收。

所有 GPU 运行均使用 `env -u WAYLAND_DISPLAY ... flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute ...`，本轮未启动可见游戏、未结束其他进程：

| 运行 | 日志（位于 `target/re-flora-logs/`） |
| --- | --- |
| 普通启动 B，`--auto-exit 0.5` | `re-flora-20260912-135520.866-339125.log` |
| 原 A，`RE_FLORA_BUTTERFLY_ORIGINAL_FLIGHT_SMOKE=1`、`--auto-exit 0.5` | `re-flora-20260912-135538.396-339226.log` |
| 自然主体与风调参，`RE_FLORA_BUTTERFLY_REVIEW=wind` | `re-flora-20260912-135547.388-339302.log` |

前两次日志分别确认 `startup_variant=DartingBlock` / `OriginalSprite`。第三次参数为 `--windowed --denoiser-bench blacky target/butterfly-review/wind.toml --denoiser-bench-warmup-frames 650 --denoiser-bench-frames 360`；自然主体在 simulation frame 655 选定，745/835/925 帧之后风倍率依次 0/2/1，自主速度保持 0.65。771 个逐帧记录中最高 6 只；各段记录到非零局部风样本。滑杆生效的因果隔离由确定性测试证明，不能从不同时间段的自然飞行截图推出定量倍率关系。

`wind.toml` 枚举 120 张 2560×1440 原始 PNG，目录 `target/butterfly-review/wind.artifacts-O5Q0wQ/`；检查了 `frame-0210/0213/0216.png` 连续采样，能见紫/蓝/白方块位移，没有蝴蝶纹理帧动画。三次运行均退出 0、shutdown failures=0，无 ERROR/panic/VUID。保留既有多蝴蝶 atlas 告警；采集运行另记录一次 `[COLLISION][FRUIT] physics hitch dropped 19.655 ms`，未隔离其原因，不把截图运行视为性能通过。

本轮功能提交 **`250e01cd`**；仅改 `src/particles/{butterfly_flight,emitters}.rs`、`src/app/core/{mod,particles}.rs` 四个源文件，另提交本报告。没有生成文件、配置、shader、素材或 GPU ABI 差异；本地截图/日志仍在忽略的 `target/`。前述与主工作区 `upload_particles` 落叶共享着色的集成提醒不变。本轮未 merge/push、未管理 Worker；用户手动 Wind 工具操作、最新动态自然度、满负载性能与长期复杂地形仍待验收。
