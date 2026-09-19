# 小尺寸蝴蝶动画、Debug 折叠层级与细枝 A/B

2026-09-13，`agent/butterfly-block-flight`。

## 已完成

- `9d05a3df`：默认 `DartingSprite`，恢复现有蝴蝶动画图集、配色及朝向选择，保留当前方块使用的 `STANDARD_PARTICLE_SIZE`（1/256 世界单位）。checkbox 改为只切外观：未勾选动画蝴蝶，勾选单色方块；两边共享现有顿挫飞行、风、地形相对高度、数量与寿命。外观变化不重置运动或显示节拍。旧 OriginalSprite 飞行动作仅留给旧配置／诊断，不再是 GUI 的未勾选选项。
- `703469e2`：Terrain & Plants、Environment Probes、Camera Snapshots、Flora Growth 各自改为默认收起的二级折叠菜单；存档、重建和相机应用行为不改。
- 细枝实验：Debug Panel → Flora → Tree → `B: Hide branches thinner than minimum (A/B)`。默认未勾选 A，保留原加粗；勾选 B，不生成低于原最小半径 1.05 体素的木质部分，渐细跨阈值处截断。通过现有调参树重建流程即时生效，并通过统一 Save 持久化，没有新增保存接线。

## 模型与范围

原枝条在基础圆锥和细分圆锥处都做了半径下限钳制。B 根据钳制前的半径过滤／裁短输出圆锥，而不是在 shader 中隐藏，所以体素木材和后续实体查询一起更新。
仍执行原细分随机序列，再决定哪些输出保留；切回 A 不会换一棵随机树。叶片／果实锚点保持原权威，实验只改木质几何。由于细枝不再托住原来的叶片，局部叶片可能显得悬空，这是此次隔离木材 A/B 的已知表现边界。

## 验证

- 最终 `cargo fmt --check`、`cargo check`、`cargo test` 通过：**961 + 4，2 ignored**；物理 crate **44** 通过。
- 蝴蝶逐步对照测试覆盖动画↔方块切换前后的同一位置、速度、显示节拍、尺寸、配色编号、alpha 和动画帧；动画帧持续变化。保存全字段测试覆盖新的默认枚举。
- 细枝纯逻辑测试覆盖完全过细、跨阈值、恰好等于阈值、带／不带细分、叶片锚点不变、A→B→A 可重复。实际 egui 鼠标点击测试确认 checkbox 会返回树重建请求，并可保存重载；通用自动遍历测试也覆盖新增 bool。
- 全部 GPU 验证使用 `env -u WAYLAND_DISPLAY ... flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute ...`。
- 蝴蝶实机切换：`target/butterfly-sprite-capture.log`，frame 779 动画→方块、899 方块→动画；截图在 `target/butterfly-review/compact-sprite.artifacts-yqPtnj/`，检查 0000/0015 帧。默认全景下小蝴蝶辨识度有限，动画持续推进由轨迹／帧回归确认，主观大小与动画手感仍需试玩。
- 菜单实机截图：`target/butterfly-review/debug-folders-visible.artifacts-BSl9MY/frame-0000.png`，确认顶层 Terrain & Plants 已收起；下面的其它工具菜单需滚动查看。
- 真实树替换：`RE_FLORA_THIN_BRANCH_REVIEW=1` 配合 `--tree-bench`。日志 `target/thin-branches-runtime.log`：10 次 A/B 交替，木质小段 **1941 ↔ 200**，叶锚点始终 **29**，每次完成 visible publication。此为正确性验证，不作正式性能结论。（首次命令多带了无效位置参数 `3`，程序实际按默认 10 次执行；可重复命令应使用 `--tree-bench --tree-bench-samples 10`。）
- 枝干外观截图使用 opt-in `RE_FLORA_THIN_BRANCH_REVIEW=A/B` 暂时隐藏叶片，并保持同一默认相机；不写回配置。已检查 `target/butterfly-review/branches-a.artifacts-mX0Faj/frame-0000.png` 与 `branches-b.artifacts-8dwqvm/frame-0000.png`，B 明显去掉末端细枝，保留主干和较粗分支。
- 最终 A/B 截图日志：`re-flora-20260913-130619.638-659711.log`、`re-flora-20260913-130623.901-660251.log`；正常退出、`failures=0`，无 ERROR/panic/VUID。保留原有 6 项 release 编译 warning。

## 交接边界

没有改 shader、粒子 GPU ABI 或生成文件；没有 merge/push、管理其他 Worker 或自动启动新的可见游戏。保留飞行调参 5.5 Hz、速度 0.35、上下强度 2、离地高度 0.08，仅切换默认外观。

`config/camera_snapshots.toml` 在用户关闭前一轮游戏附近被改成空列表，未由本次实现修改、恢复或提交。先前 `blacky` 视角因此不可用，捕获改用内置 `player-default`。工作区剩余这项无关未提交改动，不宣称干净。

实现涉及 `src/particles/{butterfly_flight,emitters,system}.rs`、`src/app/gui_config{.rs,/butterfly_flight.rs}`、`src/app/core/{particles,mod,camera_snapshot_ui,tree_bench}.rs`、`src/tree_gen/tree.rs` 与 `config/gui.toml`。审美、复杂树形下的叶片支撑关系及性能仍待人工验收。
