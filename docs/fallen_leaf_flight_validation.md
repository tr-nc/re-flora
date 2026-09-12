# 落叶薄片飞行 A/B：候选与交接

2026-09-12；独立 worktree `/home/terence/code/re-flora-agent-fallen-leaf-flight`，分支 `agent/fallen-leaf-flight`，确认起点为 `74bf8b049c1f2f7000868af10d5f82d022f4770f`。未 merge、cherry-pick、push，也未操作其他 Worker 或主工作区的游戏进程。

## 后续修正：只约束上屏频率，绝不降低物理频率

用户试玩后指出初版 B 过于流畅，并明确只要求显示遵守 World Tick，不允许改物理模拟频率。`05a52df1` 修复了 `write_snapshots` 直接透传实时物理姿态的问题：新增一份 CPU `LeafDisplayPose`，由既有 `ParticleTickStep` 两桶时钟发布，A/B 共用，不增加独立计时器。默认 `World Tick=0.05 s`，每桶/每片约 `10 Hz`；发布之间位置、速度代理和四元数保持，几何及姿态驱动颜色一致，不插值。相机和环境光仍实时观察这份姿态。

本次只改 `src/particles/system.rs` 的显示缓存、snapshot 选择及回归测试；`src/particles/leaf_flight.rs`、物理积分循环、shader、GUI 配置和 GPU 布局未改。B 每帧推进内部 120 Hz 固定步积分，A 原物理节奏、粒子年龄、寿命/数量权威不变。出生时初始化显示缓存，死亡立即移除；槽位复用不继承旧显示姿态；checkbox 只即时改变模式，不提前发布或回写物理状态。

验证：

- 修复前 `cargo test --quiet leaf_display_holds_between_world_ticks_while_physics_advances` 明确失败：未到 tick 的首个 1/120 秒，显示位置已由 `(0,1,0)` 变为约 `(0.0004967,0.9999965,-0.0002472)`；修复后通过，且内部位置/四元数仍已变化。
- `cargo fmt --check`、`cargo check`、release build 通过；`cargo test --quiet particles::` **31 通过**；完整测试 **940 + 4 通过、2 忽略**，日志 `target/fallen-leaf-flight/cadence-tests.log`。新增用例对相同来流、World Tick `0.025/0.05/0.1 s` 的 A、B 分别逐帧比对，物理位置/速度/年龄/四元数完全一致，只有发布次数改变；同时检查切换、运行时 tick 设置和槽位复用。
- 使用 X11 hidden/mute/release、同一 `flock --close /tmp/re-flora-summer-gpu.lock` 完成 `--auto-exit 0.5` 和 B 的 180 个连续帧捕获。日志分别为 `target/re-flora-logs/re-flora-20260912-134030.456-332732.log`、`re-flora-20260912-134033.255-332785.log`；退出 0、`failures=0`，未发现 ERROR/VUID/panic，并用 `--tail-latest-log 8` 复核。
- 新捕获报告 `target/fallen-leaf-flight/b-world-tick.toml`，原帧目录 `b-world-tick.artifacts-CksgHu`，便捷视频 [b-world-tick.mp4](../target/fallen-leaf-flight/b-world-tick.mp4)。仍以 60 Hz 逐帧捕获，没有把录像本身降帧率。检查帧 60/61/62 可见先保持再更新；对帧 60–77 的固定中央 ROI（x=200..1799, y=100..1149）按 `G>1.25R && G>1.05B` 提取叶片轮廓，旧连续 B 为 18 个不同掩码，新版为 7 个，多数组内连续 3 帧完全一致，符合两个显示桶交错更新。

以下为初版研究与验收历史；旧回放展示初版连续上屏，不代表修正后的显示节奏。本次未自动启动或重启可见游戏，未宣称新节奏已获得用户认可，未重新做性能验收。没有新的已跟踪生成文件差异；GUI/相机配置哈希仍与下文一致。

## 可玩入口与边界

Debug Panel → Flora → Leaves → **B: Fallen leaf plate flight (A/B experiment)**，默认不勾选。未勾选是 A：基线 billboard、原噪声下落和运动方向代理光学法线；勾选是 B：世界空间正方形，模拟姿态同时驱动几何、气动力和共享叶光学。切换不重新发射，不重置位置/速度/年龄/句柄，故不是同轨迹倒带。没有新增游戏 CLI 模式代替 checkbox。

只有 `Leaf + Falling` 使用新模型。挂树叶子几何与着色公式均未改动，cube 继续 axis-aligned。水、采集、地形和蝴蝶没有采用新运动；主粒子管线改为双面可见，原 billboard 的正面覆盖不变。精确侧视的零厚度薄片会细到亚像素，并可能出现像素覆盖闪断，这不等同于背面剔除。

每叶每帧单一 RGB，不分几何正反材质；B 的 `q·Z` 取代速度代理法线，仍调用 `shadeLeafWithEnvironment` → `leafOpticalColor`，没有复制着色公式。A 的 `Local Flutter = 0` 行为保留；B 光学始终跟随真实姿态，不受挂树叶的该开关限制。

## 研究、模型与参数

[中文研究记录](research/fallen_leaf_flight.md) 保存一手引用、阅读范围与适用条件。Pesavento & Wang 2004 PRL 全文及 Andersen 等 2005 JFM 的模型章节支持连续迎角阻力、平移/旋转环量及小气动力矩的低阶思路；Wang 等 2013 JFM 的期刊摘要用于说明有限展弦比影响，未冒称读过其付费全文。

用户的“竖直快、水平慢、转平时滑移”作为定性目标验证，不是横竖二态规则。实际变量是相对空气速度 `u = v − air`、板法线和角速度；横风下不能仅按世界竖直角度解释。二维、典型 Re 约千的刚板论文不等于真实三维叶种标定，更不保证展弦比 1 正方形复现全部 flutter/tumbling 相图。

参数均为游戏世界标定，非 SI 实测值：

| 项目 | 候选值/方法 |
| --- | --- |
| 重力 | `0.22 × 原 gravity_factor` |
| 风 | 复用 `WindFieldFrame.sample_world`，乘 `0.035 × 原 wind_factor` |
| 二次阻力 | `7 + 63(n·u/|u|)²`，随迎角连续变化 |
| 平移/旋转升力 | 系数 `45 / 0.65`，旋转项耦合板内角速度与相对来流 |
| 有效俯仰力矩 | `900 / size_ratio × (n·u)(n×u)` |
| 角阻尼 | `0.4 + 0.15|ω|`，无负阻尼振荡器 |
| 数值积分 | 固定 `120 Hz`，升力旋转速度、阻力耗散更新、四元数归一化 |
| 保护边界 | `|ω|≤12 rad/s`，映射风速≤`0.75`，相对速度≤`2`，单帧机械追赶≤`0.25 s` |

初态由已有粒子身份/种子确定，不改变发射器 RNG 序列，也不以逐帧位移或颜色噪声代替耦合。省略柔性、附加质量张量、尾迹记忆、翼尖涡和叶间作用。长停顿仍遵守原寿命/销毁权威，但不完整补算机械轨迹；实时变化风场的不同渲染采样率也不承诺逐位相同。

## 已执行的 CPU / Shader 检查

- `cargo fmt --check`、`cargo check`、`cargo build --release`：通过；既有 `terrain_hill` 和 `torus.inner_radius` 未使用警告保留。
- `cargo test --quiet`（最终 `9da3a7f1`）：主程序 **937 通过、2 忽略**，辅助 **4 通过**。日志 `target/fallen-leaf-flight/final-tests.log`。忽略项未宣称运行；测试环境有既有 ALSA 设备提示。
- `cargo test particles:: -- --nocapture`：28 通过。含固定姿态下降差异、镜像滑移、旋转升力、耗散/正反法线一致性、20/30/60/120/144/240 FPS 恒定风 12 秒轨迹、120 秒长落与极端阵风/长帧稳定性，以及生产系统的开关保留、槽位复用、其他粒子隔离、真实风输入和寿命下沉/销毁检查。
- 恒定风跨 FPS 位置/速度误差阈值 `2e-5` 通过；冻结姿态、静风下 edge-on 下降速度大于 broadside 的 2.5 倍。120 秒算例法线 y 覆盖 `−0.9578…0.9956`，峰值角速度 `2.672 rad/s`，峰值侧向速度 `0.1101`，没有靠角速度上限维持运动。
- `python3 scripts/run_slang_tests.py`：**10/10**。新增生产姿态 helper 的 81 姿态等边/正交/几何法线与光学法线一致性检查，原 attached flutter 测试仍通过。
- `cargo test --manifest-path crates/re-flora-vkn/Cargo.toml particle_vertex_shader_reflects_one_compact_mesh_input_before_instances -- --nocapture`：**1 通过**。反射实例 location 包含已有的 6，修正了基线测试遗漏该字段的旧断言；实例仍为 52 字节、`leaf_optics` offset 36。
- 新增捕获时间回归测试后 `cargo test --quiet app::core::denoiser_bench`：**12 通过**。特意验证实际模拟 timeline，不只验证报告写出的帧间隔。

## 隐藏实机与真实连续帧

所有 GPU app 运行均经 `flock --close /tmp/re-flora-summer-gpu.lock`，均 release、hidden、mute；遇到一次锁繁忙后排队等待，没有绕锁。没有主动启动可见游戏或结束用户游戏。

首先执行标准烟测：

```sh
flock --close --wait 1 /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5
```

退出成功，日志 `target/re-flora-logs/re-flora-20260912-131218.941-301342.log`。该首次 Wayland 路径提示不支持真正 hidden，而使用 minimized；后续所有验收捕获都通过移除 Wayland 环境变量走 X11 hidden，没有这个提示。

修正时钟后的捕获命令（`a-fixed` 将 `b` 换为 `a`；同进程切换用 `switch`、300 帧；正常发射用 `natural-b`、预热 1800 帧、记录 180 帧）：

```sh
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET RE_FLORA_FALLEN_LEAF_REVIEW=b \
  flock --close --wait 60 /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --windowed \
  --denoiser-bench blacky target/fallen-leaf-flight/b-fixed.toml \
  --denoiser-bench-warmup-frames 0 --denoiser-bench-frames 240
```

`RE_FLORA_FALLEN_LEAF_REVIEW` 是 opt-in 验证夹具：覆盖同一个 GUI 布尔值，并在 A/B/switch 场景注入 8 个标准大小生产粒子、固定相机；不写配置。`natural-b` 不注入、不改相机、不改风场或正常发射率，只启用 B。正常游玩没有该环境变量，不进入夹具。

硬件为 RTX 3060 Ti，X11 实际图像 2560×1440。模拟固定 60 Hz，每帧保存真实 RGB，无合成/补帧。捕获有读回和 PNG 成本，不应从截图 FPS 或以下墙钟耗时推断游戏性能。

| 捕获 | 连续帧/相邻过渡 | 模拟时长 | 捕获墙钟耗时 | 正常退出日志（`target/re-flora-logs/` 下） |
| --- | --- | --- | --- | --- |
| A `a-fixed.toml` | 240 / 239 | 4 秒 | 17.54 秒 | `re-flora-20260912-131722.005-308483.log` |
| B `b-fixed.toml` | 240 / 239 | 4 秒 | 19.26 秒 | `re-flora-20260912-131621.287-306699.log` |
| switch `switch-fixed.toml` | 300 / 299 | 5 秒 | 21.11 秒 | `re-flora-20260912-131859.645-309767.log` |
| 正常树冠 `natural-b.toml` | 180 / 179 | 预热 30 秒 + 3 秒 | 13.00 秒（仅捕获段） | `re-flora-20260912-131929.535-309912.log` |

四次均退出 0，`[SHUTDOWN] phase=complete failures=0`；检查未见 `ERROR`、Vulkan `VUID` / `Validation Error`、panic。用同 worktree 的 `--latest-log` / `--tail-latest-log 12` 复核退出链。水粒子为 0 时已有遥测的 `min_sdf=NaN` 是空采样哨兵，不是新飞行状态，未把它掩盖或当作“所有 NaN 均不存在”。

真实帧自查结论：A 仍是 screen-facing 方块；B 连续帧 80–91 与 60/90/150 等时点呈现连续旋转、斜投影和细边，翻到反向法线后仍可见。对应日志 sample 0 在约 1 秒时 `vy=−0.1132`，约 1.5 秒法线 y≈0.968 时 `vy=−0.0651`、侧向速度约 0.1035，随后转向减速；这是一个耦合轨迹的证据，不是全体叶片横竖通则。切换在第 120/240 个零基模拟帧发生，日志两次 `particles_retained=8`。正常树冠预热后约 60–63 个落叶，树冠和旋转落叶同屏；原 attached cube 没有改成薄板。

初次 `b.toml` 捕获曾暴露验证夹具 bug：报告为固定 60 Hz，但相机捕获分支仍按墙钟推进。修复实际 timeline 并添加回归测试后重跑了以上四组；**旧 `b.toml` 不作定时运动证据**，仅保留排查现场。

本地回放入口：[`target/fallen-leaf-flight/review.html`](../target/fallen-leaf-flight/review.html)，包括 A/B 视频、原始 PNG 逐帧滑块、切换和正常树冠视频。所有原 PNG 路径在各 TOML 的 `keyframe_paths`；MP4 是由这些帧按 60 FPS 编码的便捷预览，原始 PNG 才是无视频编码的画面证据。原始帧与运行日志是本地 ignored 产物，不进入 Git。

## 配置、文件与整合重叠点

源文件修改仅限落叶接线、小型模型/验证模块、共享粒子管线和文档。没有编辑挂树叶的几何 shader、共享叶光学公式、行走功能或蝴蝶运动/发射器。

| 文件/区域 | 修改与以后语义整合注意事项 |
| --- | --- |
| `src/particles/leaf_flight.rs`（新增） | 低阶角状态与积分、7 个确定性测试 |
| `src/particles/mod.rs`, `system.rs` | 模块注册、槽位姿态、仅 Leaf+Falling 分支、snapshot optional quaternion、现有寿命和数量权威；与蝴蝶 Worker 的状态/更新/snapshot 改动可能重叠 |
| `src/app/core/particles.rs` | 读取 GUI bool、传入既有风场；保留其他 emitters，water debug snapshot 显式 None |
| `src/tracer/mod.rs`, `resources.rs` | 原 `leaf_optics: vec4` 在 B 打包四元数，A 保留 normal+flag；纹理索引 bit 30 新用途，bit 31 原 sprite flip 保留。蝴蝶若新增姿态字段或纹理 flags，必须统一这套打包而非文字拼接 |
| `shader/slang/particle_lod_textured.vert.slang` | bit 30 解码、q 旋转局部方形及法线；leaf lighting 复用。蝴蝶若也改顶点定位，保持按 kind/flag 分流 |
| `shader/slang/leaf_particle_pose.slang`, `shader/tests/leaf_particle_pose_contract.slang`（新增） | 小姿态 helper 与生产函数契约测试 |
| `src/tracer/pipeline_builder.rs` | 主 particle pipeline 明确 cull NONE；water 原独立 pipeline 不变 |
| `crates/re-flora-vkn/src/shader/shader_module.rs` | 修正现有 particle reflection 测试 location 6 断言；未来两分支新增 ABI 后一起重验 |
| `config/gui.toml` | 仅新增默认 false 的 checkbox，未改变已有配置 |
| `src/app/generated/gui_adjustables_gen.rs` | **唯一已跟踪生成文件差异**，由 `cargo check` 更新；`gpu_structs.rs` 无 diff |
| `src/app/core/fallen_leaf_review.rs`（新增）, `core/mod.rs`, `core/denoiser_bench.rs` | opt-in 捕获夹具、固定时间与连续帧；不替代正常 GUI A/B |
| 本文、`docs/research/fallen_leaf_flight.md`、`docs/leaf_flutter.md` | 中文依据、模型、验收与边界 |

每槽位 CPU 姿态状态 32 字节，16,384 槽共 512 KiB；无新增 GPU 实例字段，仍 52 字节。没有据此宣称速度回归或性能通过。

运行前后配置 SHA-256 一致（`gui.toml` 哈希包含本次有意增加的 false checkbox）：

```text
a877f6ea6aae199d83ee7c65c36af0d1b7be12ee74b60983b2e34fc7b59d1707  config/gui.toml
2e81cc790ed2e02a9b881aac670cda93f5de428951da626638f8e8ff4d2a5b40  config/camera_snapshots.toml
```

## 提交与尚未验收

已按验证步骤自动提交：`728aa27c` 研究；`e2b10f08` 模型、GUI 与渲染；`9da3a7f1` 固定时钟连续帧夹具与回归测试。本文及旧文档的 A/B 表述澄清随后单独提交。

可玩候选、确定性行为、shader 契约、隐藏 Vulkan 实机及真实帧自查已完成。**尚未完成**用户可见游戏中的手动 checkbox 点击/主观自然度认可、复杂风况和不同叶种的物理标定、专项性能预算/大粒子量基准，以及与蝴蝶/行走独立分支的整合验收。静音运行不是听感验收；本次没有启动可见游戏。
