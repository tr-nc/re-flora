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

## 步骤 2：静止整树光栅化 A/B，可进行第一轮视觉验收

入口：`R → Debug → Whole Tree Rasterization → Raster whole trees (B: static comparison)`。未勾选 A 是原路径，勾选 B 是候选；设置由 `config/gui.toml` 声明，走统一持久化。树叶继续走原有光栅化路径，新增的是全部可见木质枝干表面，所以 B 的整树主画面均由光栅化绘制。

### 最终实现

- 从已发布 atlas 读取规范树边界及两体素法线 halo，只选择规范枝干圆锥覆盖的现有 cherry-wood 表面体素。使用实际占据、六邻域剔除内部面、与 terrain 相同的半径二法线估计及 oct16 量化。其他木制物体不会因材质相同被全局隐藏；重叠树表面去重，挖掉的体素不会由树描述重新生成。
- 同一份网格生成颜色几何与仅含位置的投影几何；不通过微小深度扰动维持 shader 顶点 ABI。相机和太阳的体素遍历跳过已转换表面，由真实 raster color/depth 和 shadow pass 接管。
- 木头复用 terrain 调色板、太阳 cosine/阴影接收规则及 terrain DDGI 平滑查询。局部灯使用现有 voxel/Glass 可见性算法，在 compute 中按表面体素缓存；fragment 读取缓存，不在 fragment 阶段调用依赖 workgroup 共享栈的遍历。局部灯缓存取沿法线的体素表面位置，和 terrain 的逐像素局部灯采样仍有粒度差别。
- 静止体素继续作为精确的二次射线和碰撞表示。此步骤没有枝干形变，因而代理与可见几何处于同一姿态；这不是动态树最终方案，后续不能直接让网格运动而保留静止遮挡。
- 可见 terrain revision 变化后重新读取并生成网格，提交前等待在途帧完成；删除、年龄变化、替换通过实际发布状态刷新。准备失败会回到 A。新增管线接入 DDGI 消费者注册和 resize descriptor 发布，避免探针或窗口重建后持有旧资源。

### 验证与证据（2026-09-16）

- `cargo fmt --check`、`cargo check`、`cargo test` 通过：999 + 4 passed，2 ignored。日志 `target/raster-tree-check.log`、`target/raster-tree-tests.log`。生成文件由 check 更新：`src/app/generated/gui_adjustables_gen.rs`、`src/auto-generated/gpu_structs.rs`。
- 默认 A 的 `cargo run --release -- --hidden --mute --auto-exit 0.5` 同样通过，日志 `target/re-flora-logs/re-flora-20260916-023322.412-107972.log`，正常退出且 `failures=0`。
- 新增网格测试覆盖相邻体素内部面剔除、重复区域去重、实际 atlas 编辑、周围地形遮挡和不属于树的同材质物体。
- `env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 60` 通过。日志 `target/re-flora-logs/re-flora-20260916-023005.056-106251.log`：A→B→A→B、年龄重建、删除和重建均通过，90 次 B 颜色 draw。成熟树 3,954 个表面体素、15,992 个三角形，年龄 0.5 时为 665 个表面体素、2,760 个三角形。
- 同一命令加 `--resize-lifecycle-test` 通过。日志 `target/re-flora-logs/re-flora-20260916-023248.781-107460.log`：frame/swapchain/tracer generation 一致推进到 4，A/B 与生命周期检查再次通过。两次真实运行均正常退出、`failures=0`，未出现 ERROR/panic/VUID。通过同工作树的 `--latest-log`、`--tail-latest-log 200` 检查日志；不将未出现 VUID 等同于启用了外部 Vulkan validation layer。
- `python3 scripts/check_raster_tree_static.py` 完成四张真实隐藏渲染截图，并恢复 GUI/相机文件。固定时刻 0.47；裸枝图保留太阳阴影，另外提供带树叶图。已人工检查 A/B 裸枝和 B 树冠：当前机位未见整体亮度突变、枝干断口或树根悬空，地面投影存在。这是有限机位检查，不代替用户视觉验收。
- 截图：`target/raster-tree-evidence/A-wood.png`、`B-wood.png`、`A-canopy.png`、`B-canopy.png`；对应同名 `.log`。脚本为独立真实应用验证，不加入普通 unit tests；再次运行会清理本次目标截图，避免接受旧文件。
- 自动化最初使用的“清除程序化树”接口保留调试树，导致删除断言失败。已修正测试明确删除调试树，再验证真正的删除和重建；没有据此更改生产清除语义。

### 此轮请用户检查什么

只验收静止树的方块轮廓、枝干明暗、树根与 terrain 的融合、树自阴影和地面投影。运行当前工作树的 `cargo run` 即可体验；不会自动启动可见窗口。

尚未完成：枝干风动、叶果跟随骨架的新姿态、动态查询/碰撞代理、密林性能和完整玻璃/局部灯场景验收。固定 lookup 和局部灯 cache 合计 8 MiB；候选容量上限为 131,071 个表面体素，超过容量或 lookup 探测预算会拒绝 B 并回到 A。当前 terrain 编辑会同步重建静止网格，尚未做局部增量优化。截图里的 FPS 和单次生成耗时不是性能验收结论。

## 后续实施边界

静止 A/B 通过用户检查后，再接层级风动及枝叶果实的姿态绑定。动态姿态参与阴影、GI/其他射线查询和交互的工作仍按调研报告逐项验证；不以保留静止隐形树干的方式宣称动态光影已完整。
