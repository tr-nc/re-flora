# 逐 voxel 材质颜色迭代

日期：2026-09-20。Worker：`agent/terrain-material-refresh`，起点 `7bdd3640`。

> 以下 A/B 阶段记录对应 `6d55135b`。用户随后接受效果，当前唯一逻辑与最终验证见文末“用户接受后的收尾”。

## 用户反馈与实现

用户试用了从固定 `c47050223d0394f2e36eb3074b8caa646ad5fe7a` 迁移的宏观候选，认为尺度过大，明确希望相邻 voxel 独立变色，而非连续渐变；批准按体素坐标生成稳定颜色并以强度控制。

- Dirt/Sand/Rock 共用一个全局整数体素坐标 + seed 哈希。每 voxel 一个固定值，没有相邻插值、噪声尺度、时间、相机或 chunk-local 输入；岩石不再叠加层理。
- 保留 soil/sand 与 rock 各自强度（默认 0.35/0.32）、冷暗暖亮色带（0.25）、seed（17）；关闭及零强度精确恢复基色。其他材质 ID 不受影响。
- Debug → Terrain Material → **A/B: Per-Voxel Color Variation (off = original)**，默认关闭。勾选候选后，用 **Soil / Sand Color Variation Strength** / **Rock Color Variation Strength** 调整差异，不再需要 scale。
- 旧 GUI 尺度、倾斜参数在加载时移除，标签来自当前声明；已保存的强度、开关、色带、seed 保留。加载不写回，Save 仍走统一配置机制。
- 五项有效控制继续参与不可变 transport identity；live、secondary、Glass fallback、DDGI 共用求值。DDGI 源 tuple、地形编辑光照、湿度近似、存档、碰撞、树木和蝴蝶均不改。
- shader uniform 改为一个 float4 加 seed/enabled；两种 palette 均为 128 字节。生成绑定由 `cargo check` 重建；不保留已无语义的尺度/法向字段。

## 验证

证据：本 worktree `target/terrain-material-per-voxel/`。

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：1032 主程序 + 4 library 通过，2 ignored，0 filtered。
- `python3 scripts/run_slang_tests.py`：17 通过；材质输出 `failures=0 reseeded=128 neighbors_changed=128,128,128`。检查 XYZ 邻居独立变化、负坐标、跨 chunk、cell 内颜色恒定、seed 稳定性、live/frozen 一致性、所有当前材质 ID 的 bypass、零强度及湿度策略。测试的是生产 Slang 函数，不是复制的 CPU 噪声。
- Rust 验证五项参数使 transport identity 更新但不改旧 frozen snapshot，帧输入启用开关映射、GPU 编码和旧配置控件退休/值保留/保存重载。
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`：63 通过。
- 必需 `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过。使用既有 `env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock` 包装，无可见窗口。额外用同一 release 二进制分别测试开启及 `7bdd3640` 的实际旧 GUI 配置，也通过。
- 每次均用本 worktree 的 `--latest-log`、`--tail-latest-log 200` helpers 检查，再扫描完整 canonical log：无 ERROR/panic/VUID，`phase=complete failures=0`、正常退出。
- GUI/camera 文件运行前后 SHA-256 一致；临时配置在 finally 中字节级恢复。

日志位于 `target/re-flora-logs/`：

| 模式 | canonical log |
| --- | --- |
| off | `re-flora-20260920-213638.210-997129.log` |
| on | `re-flora-20260920-213720.163-998385.log` |
| 旧宏观候选 GUI | `re-flora-20260920-213805.562-998888.log` |

首次完整测试只有 RFIRR fixture 模型 hash 过期失败；从 Rust serializer 输出再生成，确认仅 offset 48..56 的 8 字节 identity 改变，其余 payload 不变，最终完整测试通过。GUI/GPU 两个生成文件也从源重新生成。既有编译 warning 和测试 ALSA 诊断没有顺手清理。

## 待复评与历史证据

未自动重启可见游戏；等待用户下一次试用。独立颜色不是保证所有相邻 voxel 肉眼不同：强度为零、暗色、色彩量化都会降低差异。逐 voxel 细节在远处可能混叠，本次没有新的移动相机/抗闪烁接受或交互切换 GPU 捕获。

旧宏观候选的 +2.17% GPU 和失败 gate 保留在 `terrain_materials.md` 的历史部分，不是本实现测量。本次没有性能优化/新阈值或性能通过声明；先供视觉复评。

## 用户接受后的收尾（2026-09-20）

用户确认新效果不错，授权移除 A/B、老模式和看不出区别的冷暖色带控件。新的 per-voxel 求值成为唯一材质路径；不再传 enabled，也不保留运行时旧模式分支。冷暖系数固定为 `shader/slang/terrain_material.slang` 的命名常量 0.25，受现有 compiled model hash 保护。

保留用户刚保存的土壤强度 0.135，Rust fallback 同步；岩石 0.32、seed 17 不变。shader/Rust identity 移除 enabled/color-band，两个 palette 从 128 降为 112 字节。旧配置会移除旧开关、冷暖色带、早期 scale/tilt，保留强度/seed；测试包含旧 false 开关，不能再使新逻辑关闭。通用 base-color helper 与零强度分支仍用于组合材质，不是旧实验模式。

最终证据：`target/terrain-material-final/`。

- fmt/check 通过；完整 Rust 1032 + 4 通过，2 ignored，0 filtered。
- 17 Slang CPU 测试通过，材质 `failures=0 reseeded=128 neighbors_changed=128,128,128`；63 capture analyzer 测试通过。
- 必需 release hidden/muted 0.5s smoke 通过；同一二进制额外加载 `6d55135b` 实际旧 GUI（含 false 开关）也通过。沿用 X11 环境和 GPU lock，没有启动可见游戏。
- 本 worktree 的 `--latest-log` / `--tail-latest-log 200` helpers 和完整日志扫描无 ERROR/panic/VUID，正常退出，`failures=0`。
- Canonical logs：`target/re-flora-logs/re-flora-20260920-221022.165-1011598.log`、`re-flora-20260920-221102.215-1012738.log`。
- GUI/camera SHA-256 在验证前后完全一致；原用户的 0.135 是有意保留的输入，不是测试写回。
- GUI/GPU 生成绑定由 `cargo check` 重建；RFIRR fixture 从 Rust serializer 输出更新，仅 8 字节模型身份变化。首次测试的旧 fixture hash 失败及最终通过日志均保留。

本次用户接受的是视觉效果，仍没有新的性能接受测量或阈值。用户另授权通过 Dispatch 交给 main Worker 集成，只有 main 验证成功、源 HEAD 被包含且当前 Worker settled/clean 后才能移除当前 Worker；旧 `re-flora-terrain-material`、远端分支及其他 Worker 必须保留。
