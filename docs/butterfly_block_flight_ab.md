# 蝴蝶方块飞行 A/B 候选交接

2026-09-12；worktree `re-flora-agent-butterfly-block-flight`，分支 `agent/butterfly-block-flight`。
开始时工作树干净，base 为 `9ca488e9ed9b623691596d6dfdfe4a0f096c16db`。

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
