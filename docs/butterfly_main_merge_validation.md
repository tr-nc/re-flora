# 主分支合入蝴蝶 Worker：语义合并与验证

2026-09-12，在 `agent/butterfly-block-flight` 的独立 worktree 执行。
合并前工作区干净，HEAD 为 `2b37f03181ca9eba0f94645f300181ab1625cfdb`；
按用户授权执行普通 `git merge --no-commit 57c5f999da9726da56d3eca92a5e792060f770bc`，
共同祖先为 `9ca488e9ed9b623691596d6dfdfe4a0f096c16db`。
没有更新 main、操作其他 Worker、push 或启动可见游戏。

## 合并决策

按 `resolving-merge-conflicts` 流程对照双方提交和落叶验证记录，逐处处理四个冲突文件：

- `src/particles/system.rs`：保留 `Leaf + Falling` 的固定步物理和 World Tick held pose；
  `GuidedFlight` 仍由蝴蝶 emitter 独立推进，避免共享更新再次积分或累计年龄。
  snapshot 先选择落叶 held pose，仅 ButterflyBlock 才调用蝴蝶共享节拍的显示位置。
- `src/app/core/particles.rs`：同一份 `WindFieldFrame` 传给共享粒子系统与蝴蝶更新；
  保留两个模块的生命周期、采集、观察和上传顺序，不新增生成器或数量权威。
- `src/tracer/mod.rs`：落叶保留 `leaf_optics` 和 texture bit 30；蝴蝶 B 只复用普通白色方块层，
  不置落叶标记；A 仍用原素材、动画帧、视角与 bit 31 翻转。
- `src/app/core/denoiser_bench.rs`：落叶夹具逐帧捕获、蝴蝶每三帧存一张的既有诊断能力都保留。

另在 `src/tracer/leaf_particle_pose.rs` 的非叶光学隔离测试中覆盖 ButterflyBlock；新增混合系统测试，
逐帧比较 B→A→B 切换时的落叶与纯落叶参照，断言没有重复推进、错误显示回调或光学姿态泄漏。

蝴蝶 `butterfly_flight.rs` 和 `emitters.rs` 与合并前逐字节一致。Debug Panel → Butterflies →
`B: Darting color blocks (A/B experiment)` 默认勾选，取消为原 A。
`Shared flight frequency (Hz)` 默认 **10 Hz / 100 ms**，自主速度 **0.35**、上下强度 **4**，
水平机动／转向／风漂移均为 1；仍可实时调节，未恢复独立上下时钟。

主分支的 shader、`ParticleInstanceGpu`、行走和果实物理源码均与指定提交一致。
实例仍 **52 字节**，`leaf_optics` offset **36**；落叶几何固定 screen-facing，不恢复落叶 A/B。
V 管道代码、文档及图标的删除直接来自主提交，可从 Git 历史恢复，不另做清理。

## 本工作区重新执行的验证

- `cargo fmt --check`、`cargo check`、`git diff --check`：通过。
- `cargo test`：主程序 **946 通过、2 ignored**，工具 **4 通过**。
  包括 Debug checkbox 的实际 egui 指针点击、共享 100 ms 节拍、跨 FPS、风与数量隔离测试。
- `cargo test -p re-flora-physics`：**44 通过**（16 单元、21 角色、6 果实、1 地形更新）。
- `python3 scripts/run_slang_tests.py`：**10/10**，包括落叶姿态／方块几何／整叶 RGB 契约。
- `cargo test -p re-flora-vkn particle_vertex_shader_reflects_one_compact_mesh_input_before_instances -- --nocapture`：
  **1 通过**；反射输入 locations 为 `0,2,3,4,5,6`，光学输入为 float4；CPU stride/offset 断言也通过。
- `cargo build --release`：通过，生产 shader 由构建编译；保留已有 6 个 Rust warning，未借机清理。

CPU 日志位于 `target/butterfly-merge-{fmt,check-final,main-tests,physics-tests,slang,abi-reflection,release-build}.log`。
测试仍有已有的 ALSA 设备探测输出／egui deprecated warning；没有失败测试被放宽或移除。

## 隐藏静音真实 GPU 与画面

每次均使用 `env -u WAYLAND_DISPLAY` 选择真正隐藏的 X11 窗口，并用
`flock --close /tmp/re-flora-summer-gpu.lock` 串行运行 release 游戏。
同 worktree 的 `--latest-log` / `--tail-latest-log 8` 用于核对日志。

| 验证 | 本次日志，均位于 `target/re-flora-logs/` |
| --- | --- |
| 默认 B，`--hidden --mute --auto-exit 0.5` | `re-flora-20260912-174813.369-520369.log` |
| 原 A，相同烟测，设置 `RE_FLORA_BUTTERFLY_ORIGINAL_FLIGHT_SMOKE=1` | `re-flora-20260912-174831.148-520702.log` |
| 自然蝴蝶 A→B→A，`RE_FLORA_BUTTERFLY_REVIEW=switch` | `re-flora-20260912-174857.525-520792.log` |
| 定稿落叶夹具，`RE_FLORA_FALLEN_LEAF_REVIEW=fixture` | `re-flora-20260912-174943.114-520960.log` |

以上均退出 0、`phase=complete failures=0`，无 ERROR/VUID/Validation Error/panic。
运行 warning 只有既有的多个蝴蝶 atlas 提示。

蝴蝶捕获使用 `--denoiser-bench blacky target/butterfly-review/merged-ab.toml`
加 `--denoiser-bench-warmup-frames 500 --denoiser-bench-frames 720`。
981 条观察记录，最多 6 只自然蝴蝶；第 577 帧锁定自然个体，第 697/817 帧请求切 B/回 A，
下一帧生效。两个切换边界均为 2 只、palette `[3,5]`；B 的 120 帧约 4.415 秒，首个体显示位置变化 44 次。
没有为了验证注入蝴蝶。报告及 **240 张 2560×1440 原始 PNG** 在
`target/butterfly-review/merged-ab.artifacts-r2qGAf/`。
实看 `0195/0198/0201`、`0315/0318/0321`，原素材→单色小方块→原素材均可见，场景无明显损坏。
这不是逐帧轨迹倒带，也不等同于用户手动试玩认可。

落叶报告 `target/butterfly-review/merged-leaves.toml`，夹具使用相同捕获命令、warmup 0、frames 180。
**180 张连续原始 PNG** 位于 `merged-leaves.artifacts-yvSID6/`。
实看 `0060/0061/0062`：方块始终朝镜头，60→61 保持，62 部分个体的位置与整叶颜色随桶更新。
捕获读回及 PNG 写盘影响帧时，不作为性能基准。

## 附加果实 trace 边界

75 秒正常 release hidden/mute 运行，`RE_FLORA_FRUIT_GROUND_TRACE=target/butterfly-merge-fruit-trace.jsonl`，
日志 `re-flora-20260912-174958.718-521061.log`，正常退出且无渲染错误。
`python3 scripts/check_fruit_ground_trace.py target/butterfly-merge-fruit-trace.jsonl` **返回 1**：
17 个最终果实中 16 个通过；body 29 在模拟第 74.451 秒才首次自然休眠，末尾只有 2/584 个样本休眠，
因此未满足连续末 10 秒休眠的原断言。该窗口最大位移范围 0.039825 voxel、角度范围 0.014324 rad；
所有果实渲染位置／四元数同步误差均为 0，掉落期间碰撞砖队列为 0。
失败摘要保存在 `target/butterfly-merge-fruit-summary.json`，不隐藏这次失败。

采用 `diagnosing-bugs` 的捕获回放与单变量延长采样检查；未修改物理、求解参数、trace 或断言。
100 秒独立复验采用同一命令、只将 `--auto-exit` 改为 100 并更换输出文件；
日志 `re-flora-20260912-175254.951-521390.log`，仍退出 0、`failures=0`、无上述渲染错误。
`python3 scripts/check_fruit_ground_trace.py target/butterfly-merge-fruit-100s-trace.jsonl` **返回 0**：
模拟 99.490 秒，**17/17** 末尾 10 秒全部自然休眠，最大位置范围 0、角度范围约 `4.21e-8 rad`，
渲染位置／四元数误差均为 0。摘要为 `target/butterfly-merge-fruit-100s-summary.json`。
本次最晚首次休眠约 15.45 秒，而非前次的 74.45 秒，说明运行间存在时序／轨迹变异；
延长运行并非已证实的根因修复，不能承诺固定 75 秒总能 17/17 通过。
由于果实／角色物理及 trace 源码与固定主提交完全一致，此处仅记录合并验收边界，不扩展修改果实模型。

## 文件与验收范围

人工合并／补测文件为上述四个冲突文件、`src/tracer/leaf_particle_pose.rs` 和本文；其余新增／删除来自固定主提交。
`cargo check` 已再生构建输出，两个已跟踪生成文件
`src/app/generated/gui_adjustables_gen.rs`、`src/auto-generated/gpu_structs.rs` 均无 diff，未手改。
GUI 与相机配置全程无 diff，SHA-256 分别为
`2aeedce8e2d2f3036920313b7aec11589b7e538214d4e3986d1c56e8e3255ae3`、
`2e81cc790ed2e02a9b881aac670cda93f5de428951da626638f8e8ff4d2a5b40`。
本次不宣称通过用户手动点击／主观动态自然度、满数量复杂场景或专项性能验收。
