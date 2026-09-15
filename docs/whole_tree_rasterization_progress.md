# 整树光栅化：实施记录

用户选择：保留现有体素方块轮廓。每个验证通过的步骤独立提交。

## 研究基线

- `25d17975`：提交 `docs/research/whole_tree_rasterization.md`。
- 实施起点为 `c9b9d0f9` 的当前工作树，不替换其他 worktree 的实现。

## 步骤 1：保留树的拓扑与附着身份

原生成过程会在输出 RoundCone 和叶片位置后丢弃骨架。现在每个枝段保留生成时确定的父段索引；父段先于子段，不通过坐标猜测连接。Tree 保留完整拓扑、每个细分/裁剪后枝干圆锥的所属枝段，以及每片叶的附着枝段。索引针对同一 authored tree，跨年龄显示层级、细分和细枝裁剪保持身份；更改 seed/拓扑应重建。

规范 TreeRecord 持有一份 `Arc<Tree>`，替换之前为声音单独保留的两份几何数组。删除、替换、回滚和批量发布沿用现有事务；声音重采样读取同一份树。这里尚未引入动态姿态、网格、GPU 绑定、果实骨架绑定或 A/B 控件，画面保持原路径。

### 2026-09-16 验证

- 手动改动前/后几何指纹：`025ae0ae02c7e8fc`，48 组 seed/年龄/细分/裁剪组合完全相同。指纹遍历枝干端点/半径、叶片位置/锚点的 f32 字节；这是本机前后回归证据，不是跨平台 golden test。临时测试已移除，日志：`target/tree-geometry-before.log`、`target/tree-geometry-after.log`。
- 新增确定性测试：父子拓扑（包含全部坐标重合的零长度骨架）、年龄/细分/裁剪后的附着绑定、年龄缩放后的枝段身份。既有声音重采样测试现在检查整棵静止树的 Arc 身份未改变。
- `cargo fmt --check`、`cargo check`、`cargo test` 通过：997 + 4 passed，2 ignored。完整日志：`target/tree-topology-check.log`、`target/tree-topology-tests.log`。测试设备探测有 ALSA 诊断输出，无测试失败。
- 持锁隐藏静音运行：`env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5`。使用同一工作树的 `--latest-log` 与 `--tail-latest-log 200` 检查。
- 真实运行日志：`target/re-flora-logs/re-flora-20260916-020216.131-85386.log`。树发布 200 个枝干圆锥、15,196 个叶实例；正常退出，`failures=0`，无 ERROR/panic/VUID。
- 未修改 shader、生成绑定或 GUI 配置。Cargo 首次运行补写了已有 petalsonic registry 来源元数据，已撤去这个无关 lockfile 差异。
- 拓扑访问器尚未接入生产渲染消费者，因此当前普通构建额外报告两项 dead-code 警告；不以此宣称光栅化完成。没有做新的视觉、风动或性能验收。

## 后续实施边界

接下来从同一份树生成保留方块轮廓的表面网格，接入静止整树的光栅化 A/B 与地形光照融合，再接层级风动。动态姿态参与阴影、GI/其他射线查询、附着物和交互的工作仍按调研报告逐项验证；不以保留静止隐形树干的方式宣称动态光影已完整。
