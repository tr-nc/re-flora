# 材质候选迁移：视觉复评入口

> 本文是 `7bdd3640` 宏观斑驳候选的迁移历史。用户复评后已改为
> [逐 voxel 独立颜色变化](terrain_material_per_voxel.md)；本文旧控件及布局不代表当前实现。

## 版本与边界

- Worker：`agent/terrain-material-refresh`，仅修改本 worktree。
- main 基线：`d7bbee241c6880df00d71300bffa77eef1262bce`。
- 固定合并源：`c47050223d0394f2e36eb3074b8caa646ad5fe7a`；非 cherry-pick，保留候选历史。
- 旧 `re-flora-terrain-material` worktree 原样保留；未管理其他 Worker，未合入 main、发布或启动可见游戏。
- 只迁移 Dirt/Sand 连续宏观斑驳、Rock 层理及其参数/身份契约；无存档、碰撞、体素结构、树木/蝴蝶行为修改。

## A/B 操作

Debug → **Terrain Material** → **A/B: Procedural Terrain Material (off = original)**。

- unchecked（仓库默认）：main 原始调色板和现有湿度响应。
- checked：候选斑驳/层理；可调整尺度、强度、倾斜、冷暖色带、种子。
- 开关和参数走声明式控件及统一保存流程；旧 GUI 文件缺少此 section 或部分参数时补入编译时声明，不覆盖已有值、不在加载时写回。
- 同世界、同相机比较；可见材质即时切换，DDGI 保持不可变 in-flight 快照，等待完整新 field 发布后再比较间接光。关闭不是清空 DDGI 的捷径。
- 用户明确试用后才从此 worktree 执行 `cargo run`；本次止步于视觉复评入口。

## 合并与当前 main 兼容

冲突处理保留 main 的天空强度、当前 Glass 开关和测试；palette 大小变为 144 字节，radiance sun 仍为 48 字节。没有恢复旧 Kochia/GUI 默认值。

地形编辑仍直接使用 sampled physical irradiance，不恢复固定色兜底。递归 source readiness、source irradiance/visibility/placement owner tuple、当前 terrain occupancy 可见性 revision、渐进完整场发布和最新树木路径均保留。实际 shader 差异仅为材质函数/参数和各入口调用。

live uniform 和 DDGI frozen palette 来自同一 Authored Environment Lighting 输入。八个控件均进入 radiance identity；切换产生 Transport Input Step，重置适用 irradiance history，不改已冻结 source。compiled model identity 增加材质源码，并继续哈希当前 main 的 transport shader。DDGI 原有 dry-material 近似和 Glass off-screen 简化光照保留，没有引入动态湿度 transport。

## 本次验证（2026-09-19）

证据目录：本 worktree 的 `target/terrain-material-refresh/`。

- `cargo fmt --check`、`cargo check`：通过。
- `cargo test`：1031 主程序 + 4 library 通过，2 ignored，0 filtered；未沿用旧候选 PATT skip。
- `python3 scripts/run_slang_tests.py`：17 通过。材质 contract：`failures=0 reseeded=128 neighbor_delta=0.027605 seam_delta=0.00000000`。覆盖确定性、种子、连续性、chunk 边界、live/frozen 相等、关闭/零强度精确回退、湿度响应；扩展至 main 新增 Limestone/Ivy/Petal ID，验证不受候选影响。
- Rust 针对性覆盖：八项材质控制使 frozen transport 失效但不修改旧快照；参数归一化/GPU 编码；帧输入映射；旧/部分 GUI 配置迁移及保存重载。完整测试还通过现有 recursive source tuple、terrain publication、capture identity 回归。
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`：63 通过。
- 必需 `cargo run --release -- --hidden --mute --auto-exit 0.5`（off）：通过。按现有仓库验证惯例使用 `env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock`，走隐藏 X11/Vulkan 窗口路径。
- 同一个 release 二进制额外运行 on 和实际 main 旧 GUI 配置：均通过；临时配置在 finally 中字节级恢复，不重编译或修改默认值。
- 每次均用本 worktree 的 `cargo run --release -- --latest-log`、`--tail-latest-log 200` 读取日志；并检查完整 canonical log：无 ERROR/panic/VUID，`phase=complete failures=0`，正常退出。

Canonical logs（位于 `target/re-flora-logs/`）：

| 模式 | 日志 |
| --- | --- |
| off | `re-flora-20260919-211523.583-424750.log` |
| on | `re-flora-20260919-211714.604-441178.log` |
| main 旧 GUI | `re-flora-20260919-211833.348-441993.log` |

运行前后 `config/gui.toml` 与 `config/camera_snapshots.toml` SHA-256 完全一致。GUI 的最终 diff 只有新增 Terrain Material section，没有运行时意外写回或旧默认值回退。

首次测试编译暴露新增测试缺少 import，已修正；随后完整测试只因 RFIRR golden fixture 的编译模型身份过期失败。fixture 从 Rust serializer 的真实输出重新生成，仅 offset 48..56 的八字节模型身份变化，格式、lineage、sample payload 不变。保留 first/second 测试日志，不将中间失败算作最终通过。

## 生成文件与剩余限制

- `src/app/generated/gui_adjustables_gen.rs` 与 `src/auto-generated/gpu_structs.rs` 从当前 config/Slang 经 `cargo check` 再生成，未手改生成绑定。
- `scripts/tests/fixtures/ddgi_filter_evidence_v10.hex` 从 Rust serializer 输出再生成，而非选择冲突任一侧的过期 hash。
- 构建仍有现有未使用/弃用 API 警告；Rust 音频测试输出 ALSA 设备诊断，测试最终通过。未做无关清理。
- 本次是基础正确性和隐藏启动验证，不是新视觉、GPU 性能、完整 RFIRR GPU/lighting-mode 或移动相机抗闪烁接受。没有交互式 checkbox 点击或实时切换 GPU 捕获；两种模式分别启动验证，运行时参数接线由帧输入与身份测试覆盖，实际观感留待用户复评。
- [旧候选记录](terrain_materials.md)的 **+2.17% GPU、失败 performance gate、旧 lighting-mode readback 失败**完整保留为历史信息，不当作当前结果。本阶段未开展大型性能优化、未设置新阈值；用户认可视觉后另行做 release 性能接受。
