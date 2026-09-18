# 整树光栅化：实施记录

用户选择：保留现有体素方块轮廓。每个验证通过的步骤独立提交。

## 当前状态：仅保留平滑风动，移除轴对齐实验

用户近景体验后明确放弃每块刚性平移、保持世界轴对齐的方案：相邻木块错位产生的裂缝/重叠细节不符合期待。现在只有原体素/光栅路径开关 `Raster whole trees`，以及 `Animate raster trees with wind`。同时开启就是原先第三项关闭的平滑蒙皮模式；默认值与材质风格不变。

- 删除 Axis Aligned GUI 声明、分组和生成字段；加载旧保存文件时丢弃退休参数，不改变其他设置，下一次统一保存不会写回它。测试覆盖旧值 true/false 和实际保存、加载、再次保存。
- 删除树网格的全实体木块/隐藏面生成、中心绑定、每块实例绘制、CPU 紧凑盒子表面、专用盒子精确查询，以及着色器轴对齐与叶果锚点补偿分支。保留共享角点蒙皮、GPU 骨架/表面/BVH、精确三角形碰撞和重心坐标挖掘；通用物理库的盒子 API 不属于树专用开关，未改动。
- 附着叶片、阴影与果实交接直接使用原平滑模式的点/向量变换。无降频、冻结姿态或碰撞降级。
- smoke 不再切换退休表示，改为检查风动关闭/恢复；保留强风、逐 GPU 顶点/法线、完整查询 BVH、17 条精确射线、实际挖掘、生长/删除/重建和 resize。

验证：`cargo fmt --check`、`cargo check`、`cargo test` 通过（1016 main + 4 library，2 ignored）；物理库 50 项通过；基准/CLI 五项 Python 测试通过，修改的基准与测试文件通过定向 ruff 检查。`cargo check` 自动更新的生成文件仅 `src/app/generated/gui_adjustables_gen.rs`。release 的 `--hidden --mute --raster-tree-smoke --resize-lifecycle-test` 及默认 0.5 秒运行通过，日志 `target/remove-tree-blocks-{check,tests,physics-tests,smoke,default,tail}.log`，无 ERROR/panic/VUID。所有运行恢复 GUI/相机文件；既有 `Cargo.lock` 改动未纳入。

当前诊断命令：

```sh
python3 scripts/check_raster_tree_static.py --wind --output target/tree-smooth-only-visual
python3 scripts/benchmark_tree_update.py --output target/tree-smooth-only-perf \
  --modes static smooth --repeats 3 --seconds 12
```

基准默认现在为 static/smooth，退休的 `blocks` 与截图 `--axis-aligned` 不再接受，help 提供平滑模式替代用法。截图已检查裸枝和树冠，轮廓、接头和阴影未见明显回归。当前物理窗口 **4096×2560**（早先验收为 5120×2880），不将跨轮帧耗时变化解释为代码提速。本轮三次同场景 static/smooth 的 median/p95 分别为 **17.10/18.04 ms**（1513 样本）和 **17.05/18.21 ms**（1474 样本）；平滑树 CPU 更新中位数约 1.445 ms，整帧细小差异属于噪声范围，不声称负开销、零工作量或密林性能通过。

**以下是按时间保留的历史实施记录。涉及 Axis Aligned 开关、`--axis-aligned` 或 `blocks` 的旧入口与结论已被上述用户决定取代，不是当前操作说明。**

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

## 步骤 3a：共享层级姿态（2026-09-17）

静态 A/B 用户检查未见明显外观或体感性能差异；这不是动态或 release 性能验收。继续保持原静态 A/B 外观，先建立动态消费者共同使用的姿态。

- 每份规范 TreeRecord 拥有对应生成版本的 TreePose；树替换、年龄更新、删除和事务回滚沿用现有 record 生命周期。每帧读取现有统一风场；没有独立噪声风或全树正弦位移。
- 树骨架在构造时从局部体素转换为世界单位。逐枝使用带阻尼的角响应，子枝继承父枝变换；固定根点，保持枝段长度、接点连续。响应参数目前为内部美术初值，不声称是材料力学标定。
- 发布刚性正反变换供后续网格、附着和交互消费。已知 branch 身份下的逆变换不等于已完成摆动网格的拾取；蒙皮后的命中仍需保留静止位置与三角形重心信息。
- 当前此提交尚不把姿态应用到可见几何。不能打开风动却留下静止隐形树的查询行为，然后将它交付为完整功能。

验证：`cargo fmt --check`、`cargo check`、4 个姿态定向测试通过。全量并行测试有一个既有灯光 worker 测试在 1,000 次 yield 预算内未收到结果；该测试单独复跑通过，随后 `cargo test -- --test-threads=1` 全量通过（1003 + 4 passed，2 ignored）。保留首次失败记录，不修改无关测试。日志：`target/tree-pose-{check,tests,retest,tests-serial}.log`。

隐藏静音 release 默认运行与 `--raster-tree-smoke` 均正常退出，`failures=0`，无 ERROR/panic；真实日志分别为 `re-flora-20260917-001835.655-23637.log`、`re-flora-20260917-001848.540-25198.log`。后者在 A/B、年龄重建、删除和替换过程中新增验证姿态已推进、拓扑长度匹配、变换有限。生成文件未改变；不构成可见风动或性能验收。

## 步骤 3b：真实体素表面的蒙皮绑定与命中还原

- 已发布 atlas 网格在重建时绑定到所属规范树的骨架；使用生成器保留的 cone→branch 身份。共享方块角点按同一个静止位置绑定，不从各自体素中心独立选择骨骼。绑定在形状修订时重建，不随风重新选择。
- 父/子刚性变换按枝段纵向平滑混合，根部权重归零；法线用混合矩阵的逆转置变换。允许未来渲染、阴影和查询从相同绑定生成表面。此方案保持网格连接，不声称每个方块在形变后仍为刚性正方体。
- 保留静止顶点及三角形；精确三角形命中通过重心坐标还原静止表面位置。这能处理一个三角形的三个顶点使用不同绑定的情况，不能用命中点所属单根骨骼逆变换代替。当前参考 raycast 为线性遍历，仅用于合同验证，尚未进入玩家编辑或逐像素渲染路径。
- 隐藏 A/B smoke 在真实树、年龄更新和替换后计算蒙皮表面，检查完整绑定、有限位置与单位法线。新增相邻真实体素块测试验证共用的四个角点逐位相同，且弯曲后命中可还原静止面；另外验证叶片使用 authored branch 身份和双面三角形命中。
- GPU 的颜色和投影仍保持用户已检查的静态版。动态遮挡查询、叶果消费姿态与移动后的编辑接线未完成，尚未进入下一轮用户视觉验收。

验证：`cargo fmt --check`、`cargo check`、蒙皮/共角点定向测试、全量串行测试通过（1007 + 4 passed，2 ignored）。日志 `target/tree-skin-{check,tests,seam-tests,tests-full}.log`。`--raster-tree-smoke` 真实运行日志为 `re-flora-20260917-002345.816-28229.log`，A/B、年龄、删除和替换通过，90 次 B color draw，正常退出 `failures=0`；默认 `--hidden --mute --auto-exit 0.5` 也通过，输出 `target/tree-skin-default-run.log`。同工作树日志 helper 已检查。未修改 shader 或生成文件，未做动态视觉/性能验收。

## 步骤 3c：可体验的整树风动候选（2026-09-17）

入口：`R → Debug → Whole Tree Rasterization`。`Raster whole trees (B)` 切换原体素/光栅路径；新增可保存的 `Animate raster trees with wind` 开启 B 的层级风动。两项默认均关闭，取消动画会恢复静止网格和最新编辑过的地形碰撞。风来自现有统一风场，可用现有 Wind 工具制造阵风。

### 接入范围

- 每帧在唯一在途帧完成后发布同一份蒙皮表面；主颜色、太阳阴影、三角形射线查询、玩家/刚体碰撞读取一致的位置。显式声明 CPU 写入及 shader/vertex 消费依赖。
- 三角形 BVH 在拓扑变化时构建，普通风动只 refit 包围盒；GPU stackless 遍历与 terrain 比较最近命中。相机背景继续跳过原树表面，由 raster 深度和颜色接管。CPU 拾取使用同一表面/BVH。
- 主干、分枝均参与层级角响应，根部蒙皮权重归零；没有将树干永久固定到 terrain。木头法线随形变并在面内插值，叶片光学法线、叶方块和挂果一起跟随枝段。
- 叶片按生成器保留的附着枝段绑定，阴影代理按同一 spray anchor 跟随。果实在静止形状修订时绑定一次；脱离时继承枝段姿态与速度，并检查实际释放位置的落地路径碰撞是否就绪。
- DDGI 探针射线、太阳/局部灯射线和普通 voxel/Glass surface query 使用动态三角形；dense media 查询排除旧树木格子。DDGI receiver→probe 的 voxel 可见性检查也排除旧树，并额外查询动态表面，防止保留旧树轮廓的遮挡。探针布局/重定位仍由地形修订驱动，不声称复杂玻璃或所有动态 GI 场景已经视觉验收。
- 碰撞层保留最近收到的 terrain source occupancy，表示切换时排除原树格子，并加入变形三角形表面；退出动画恢复最新 source occupancy，不改写/伪造地形修订。新增测试覆盖期间发生地形编辑、恢复源数据、移动表面后的胶囊阻挡、移除表面后的自由通行。
- 铲子命中摆动后的树，使用三角形重心坐标映射到静止表面，并只移除木材；正常地形命中沿用原编辑。树上铲子刷子的半径在静止形状空间定义，不声称是形变空间中精确的球。其他笔刷未做动态树专项验收。原始体素保留为编辑/持久化的静止形状，不再作为 B 动画模式里的光线遮挡或树碰撞。
- 在接通局部灯消费者时修正 leaf lighting compute 的调用索引：workgroup traversal 使用 `SV_GroupIndex`，不能使用跨工作组的叶实例序号。

### 成本与验收边界

本步优先完成一致性与真实效果：CPU 计算蒙皮、refit 并上传；物理三角形 shape 当前逐帧更新。尚未完成密林性能验收，也未迁移为 GPU 蒙皮。新增固定 GPU 查询/附着缓冲约 17 MiB，连同原 lookup/cache 共约 25 MiB；上限 65,536 三角形、131,072 顶点、32,767 个唯一附着 anchor。超限准备失败回到 A；不是无限树数实现。CPU 增加静止网格、绑定、BVH及碰撞源占据缓存。

已检查真实隐藏截图 `target/tree-wind-evidence/{A,B}-{wood,canopy}.png`：当前机位枝干轮廓发生形变，方块风格和树根接地保留，未见接头裂缝，树冠/地面投影存在。截图/短运行不证明运动观感自然，也不作为性能数据；这一轮交由用户体验幅度、回弹和光影稳定性。没有自动启动可见窗口。

### 自动验证

- `cargo fmt --check`、`cargo check` 通过；物理库新增/修改的 Rust 文件单独通过 rustfmt 检查，未带入该库既有其他测试文件的格式差异。
- 全量串行测试：1008 main + 4 library passed、2 ignored；物理库 46 tests passed（包含 2 个新动态表面/恢复测试）。日志 `target/tree-dynamic-tests-final.log`、`target/tree-dynamic-physics-final.log`。
- `--hidden --mute --raster-tree-smoke --resize-lifecycle-test --auto-exit 60` 通过。真实日志 `target/re-flora-logs/re-flora-20260917-005006.383-41185.log`：强风 fixture 最大位移 9.84 体素；17 条 CPU/GPU 命中匹配；动态表面命中映射后真实移除 6 个木体素；A/B、生长、删除、重建、关闭风动恢复静止均通过。窗口/帧/tracer generation 一致推进到 5，正常退出 `failures=0`，无 ERROR/panic/VUID。未将此等同于启用了 Vulkan validation layer。
- 默认隐藏静音 release 0.5 秒运行通过，输出 `target/tree-dynamic-default-verified.log`；截图脚本在 GPU lock 下运行并原样恢复 GUI/相机文件，输出 `target/tree-wind-captures.log`。
- 改动的生成文件仅 GUI binding，由 `cargo check` 生成；没有手改生成输出。

仍待后续专项：密林/多树重叠的预算和轮廓、极端强风形变、全部 Glass/局部灯组合、动态探针布局、非铲子笔刷，以及生态/声音/落叶发射布局的骨架跟随。当前生态和声音布局仍使用生成态数据。此轮是可交互的风动视觉候选，不是上述所有子系统的最终发布验收。

最终复核：默认模式真实日志 `re-flora-20260917-005142.265-41878.log` 正常退出；最终源码重新生成的 A/B 裸枝与树冠截图均已完成，检查 B 裸枝与 B 树冠未见新的接头或接地问题。工作开始时已有的 `Cargo.lock` registry 元数据差异保持原样，不纳入本功能提交。

## 体验修复：微弱风下的旋转归一化崩溃

用户可见 debug 运行在启动约 6 秒后触发 `axis.is_normalized()`，调用栈定位到 `TreePose::advance`。原来的 scaled-axis 转换先计算向量长度再除以长度；极微弱风或长时间衰减会令长度平方进入次正规数范围，归一化丢失精度。现在对小角度使用四元数指数映射的连续泰勒展开，保留微小旋转，不截断风、不关闭断言，也不改变弹簧响应。

- 回归测试修复前复现同一个断言，修复后通过；覆盖微弱风、300 秒风停衰减及小角度转换的精度和非零运动。
- `cargo fmt --check`、`cargo check` 和全量测试通过（1010 main + 4 library，2 ignored）。
- 保留断言的 debug 隐藏静音运行 30 秒正常退出，无 ERROR/panic/VUID；日志 `target/tree-axis-debug-run.log`。
- release 隐藏静音 0.5 秒启动检查通过，无 ERROR/panic/VUID；日志 `target/tree-axis-release-run.log`。本次没有生成文件变化；既有 `Cargo.lock` 差异保留。

## 视觉 A/B：方块平移、保持世界轴对齐

在 `R → Debug → Whole Tree Rasterization` 新增保存型开关 `Axis-aligned tree blocks (B, requires wind)`，默认关闭。先启用整树光栅化和 wind；新开关关闭时保留原平滑蒙皮，开启时每个木块中心跟随同一骨架蒙皮、八个角共享相同位移，方块不旋转、不缩放、不吸附网格。模式切换重建表示并使太阳阴影历史失效；关闭 wind 恢复原静态外壳。

- 新模式从同一份已发布地形木材生成完整立方体，包括内部木块和六个面。相邻块可以重叠/分离，没有放大方块或补缝伪装。仍用原地形作为编辑权威；CPU/GPU 射线、物理碰撞、颜色和木材阴影共享生成后的动态表面。
- 叶块/挂果保持与原模式相同的变换后中心，角偏移保持世界轴；叶片光学法线仍保留其独立的光学朝向。阴影代理中心同步；挂果脱落的位置、速度和初始朝向与新表示对齐，脱落后的物理运动不受本开关约束。
- 完整木块几何量增加：当前默认树从 3,954 个外壳单元 / 15,992 三角形变为 10,596 个完整木块 / 127,152 三角形。真实运行触及旧的 65,536 三角形上限后，将三角形上限扩至 131,072、BVH 节点扩至 262,144；顶点上限仍为 131,072。固定查询缓冲增加约 5 MiB。本步是外观对照，不是性能优化或密林性能验收。

### 验证

- `cargo fmt --check`、`cargo check`、全量测试通过：1011 main + 4 library，2 ignored。容量调整后的 tracer 专项 91 项通过。新增测试覆盖内部面、统一位移、不变尺寸/法线及动态命中到静止坐标的映射；现有设置保存测试覆盖新增声明字段。
- 真实隐藏静音 `--raster-tree-smoke` 通过：轴对齐模式最大位移 17.56 体素，16 条 CPU/GPU 命中一致，实际移除 6 个木体素；模式往返、生长、删除、重建、关闭 wind 均通过。日志 `target/tree-block-smoke-verified.log`，对应 `re-flora-20260917-013750.196-55947.log`。自动 fixture 在切换完成后重新施加风响应，避免断言采样时恰好回到近静止状态。
- `python3 scripts/check_raster_tree_static.py --axis-aligned --delay 2.5 --output target/tree-axis-aligned-evidence` 完成四张真实截图并原样恢复 GUI/相机配置。该参数下 A 是原平滑风动，B 是轴对齐风动。检查 A/B 裸枝与 B 树冠，方块轮廓和地面阴影存在；当前截图风动差异较小，强风运动观感与细枝缝隙仍交由用户运行时比较。
- 生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，由构建生成。原先的 `Cargo.lock` 元数据差异保留，不纳入提交。
- 默认 release `--hidden --mute --auto-exit 0.5` 通过，无 ERROR/panic/VUID；日志 `target/tree-block-default-run.log` / `re-flora-20260917-013930.025-56237.log`。

## 用户复核后的诊断：轴对齐生效，但完整木块代价过高

用户报告新开关看不出变化、似乎仍旋转且掉帧严重。实际可见运行日志 `target/tree-block-visible.log` 确认切换到 `axis_aligned=true`；不将用户看到的差异不明显等同于开关未接通。先前轻风远景截图不足以验收这一视觉差异。

使用临时诊断补丁固定 TreePose 为 8/2 方向均匀风演化 240 步后的姿态（固定后不再演化），同一近景相机、同一光照、release 隐藏静音，分别捕获裸枝与树叶 A/B。截图 `target/tree-axis-review/{A,B}-{wood,leaves}.png` 明确显示：A 的木块随枝干倾斜；B 的木块和叶块仍保持世界轴方向，中心连续移动、相邻木块错开。未复现几何仍旋转。用户是否满意该 B 外观仍未确认，不能把技术轴对齐等同于视觉验收。另检查实际 SPIR-V 输入：木材三个 vec3（36-byte 顶点），叶片一个 uint（4-byte 顶点），叶片反射包含 tree_scene_info binding 49，未发现布局漂移。

这次单场景诊断的 CPU 中位数（毫秒；排除未激活帧与初始样本）：

| 场景 | 蒙皮 A / B | 碰撞更新 A / B | 查询结构和表面发布 A / B | 帧总耗时 A / B |
| --- | --- | --- | --- | --- |
| 裸枝近景 | 1.46 / 2.81 | 3.17 / 25.28 | 2.85 / 12.36 | 19.49 / 60.66 |
| 有叶近景 | 1.47 / 2.79 | 3.18 / 25.16 | 2.86 / 12.33 | 19.95 / 61.80 |

这些是固定强风姿态、逐帧诊断日志开启下的 release 测量，证明此候选存在重大成本，不是动态风/密林通用基准或优化后的承诺。每帧 `SharedShape::trimesh` 重建物理网格是已确认的大头；127,152 三角形的 refit/上传也很重。不能再只说有优化空间而不给实际成本。后续应研究适合轴对齐方块的查询/碰撞表示与更新方式，保留相同几何/编辑语义；这次没有擅自冻结碰撞或减少内部几何。

临时诊断源代码已移除；补丁和测量脚本仅留在 `target/tree-axis-review/diagnostic.patch`、`target/tree_axis_review.py`，标记为诊断复现材料；结果 `metrics.json` 与四份运行日志在同目录。本轮只记录诊断，没有声称修好了用户的视觉观感或性能问题。

## 后续优化设计：复用姿态和更新协议，显式区分几何表示

视觉已由用户确认，开始独立性能阶段。保留现有地形编辑源与 TreePose；外观编译决定几何表示，普通帧只更新该表示的数据。当前两个真实表示是平滑三角面与轴对齐方块，不提前引入任意外观插件框架。

1. 物理库内封装可更新复合形状：三角面和轴对齐盒均为精确子形状，拓扑不变时只更新顶点/中心和 BVH 包围盒；拓扑/表示变化才重建。通过库公开的 Shape、CompositeShape、BVH 接口实现，不修改依赖源码，不降低碰撞刷新频率。
2. 渲染查询层让轴对齐木块直接作为盒子参加 BVH 查询，避免每个盒子展开为 12 个查询三角形；颜色和阴影仍保留已验收的几何。CPU/GPU 最近命中及静止坐标编辑必须一致。
3. 测量后精简重复蒙皮/上传工作。外观适配负责从同一姿态产生所需几何；缓存重建条件包含拓扑/表示，不能只比较地形 revision 或数量。

每个可独立验证的步骤提交。物理必须覆盖移动、删除、同数量拓扑变化、无效更新保持旧状态、角色与刚体接触；实际运行覆盖模式往返、挖掘、生长和重建。性能用相同 release 场景前后测量；不以 debug FPS 或静止帧跳过更新冒充优化效果。

### 优化步骤 1：可原位更新的精确碰撞几何

物理库新增 `DeformingGeometry::{Triangles, Boxes}`，实现藏在私有 `DeformingShape` 中。两种表示共用 collider 生命周期、输入校验、包围盒 refit、角色/刚体查询。只有表示、索引内容、顶点数量或盒尺寸改变才重建；相同数量的新索引不能复用旧拓扑。无效更新在改动旧状态前拒绝。没有修改 Rapier/Parry 源码、冻结碰撞或降低更新频率。三角面模式仍是原来的双面三角面；轴对齐模式用每个实际木块对应的精确盒子，不是整枝的近似盒。

外观模块现在显式发布可选的精确木块中心，物理适配无需读取 GUI 开关或从三角形顺序猜外观。这些中心与渲染顶点来自相同蒙皮结果。今后修改外观时，可继续复用树姿态、编辑源、发布流程以及适合该外观的三角面/盒子适配。

新增 `scripts/benchmark_tree_update.py`，锁定 GPU、相机与配置，运行真正的 release app 并原样恢复配置；支持保存的前版本二进制。相同固定单树场景、真实风场、8 秒运行且去掉前约 3 秒样本：

| 模式 | 优化前帧中位数 / p95 | 物理更新优化后帧中位数 / p95 |
| --- | --- | --- |
| 平滑风动 | 18.69 / 19.73 ms | 17.21 / 18.21 ms |
| 轴对齐方块 | 64.245 / 66.75 ms | 29.19 / 30.72 ms |

日志在 `target/tree-update-perf-before`、`target/tree-update-perf-physics`。这是该场景的 release 测量，不是密林通用性能承诺。保持前后每帧完整更新，没有以静止帧跳过冒充收益。

验证：root 全量 1011 + 4 passed / 2 ignored；物理库 49 tests passed。新测试覆盖原位移动、表示切换、相同数量但不同拓扑、无效更新保留旧状态、删除/移走后旧碰撞消失，以及持续更新期间的刚体落地。生成文件未变化。

最终应用验证：`cargo check`、格式检查通过；`--raster-tree-smoke` 通过（16 条 CPU/GPU 命中一致、实际移除 6 个木体素、模式往返/生长/删除/重建），默认 hidden/mute 0.5 秒正常退出。日志 `target/tree-update-step1-{smoke,default}.log`，无 ERROR/panic/VUID。

### 优化步骤 2：查询几何直接使用方块

查询层现在接收三角面或显式 min/max 顶点索引表示的盒子，CPU/GPU 共用 primitive metadata 和 refit-only 层级。视觉网格的顶点排列知识留在外观适配中；查询层无需读取 GUI，也不展开盒子的六个面。CPU 编辑命中按盒子的平移映射回静止空间；GPU 保留原木材着色法线。方块外部进入和内部退出均返回最近前向表面。

默认树仍绘制 127,152 个三角形，查询数量从 127,152 降为 10,596。相同 release 脚本测得：平滑模式帧中位数/p95 为 17.22/18.02 ms，轴对齐为 20.50/21.52 ms（物理优化后原为 29.19/30.72 ms；最初为 64.245/66.75 ms）。日志 `target/tree-update-perf-query-verified`。既有几何容量上限保留。

验证：root 1012 + 4 passed / 2 ignored；tracer 92 项通过，包括直接盒求交与完整三角面在 96 条内外射线上的对照、平移后的编辑坐标。114 个生成 SPIR-V 均通过 spirv-val。真实 `--raster-tree-smoke --resize-lifecycle-test` 通过，16 条 CPU/GPU 命中一致、真实移除 6 个木体素、模式往返/生长/删除/重建、窗口 generation 一致；日志 `target/tree-query-stable-smoke.log`，对应 `re-flora-20260917-023246.697-75330.log`。

本步期间额外尝试按拓扑重分配 GPU 查询缓冲，GPU 回归出现 DEVICE_LOST，内核记录 Xid 109。该试验已完整撤回；只保留方块求交并恢复既有固定容量/稳定资源身份后，同一回归和后续基准均正常。尚未证明重分配失败的完整资源链路根因，不将其包装为已修复，也不声称显存按需缩放已完成。本步收益来自查询 primitive/节点数量与实际上传量减少，不来自缩小缓冲分配。

默认 release hidden/mute 0.5 秒运行亦通过（`target/tree-query-default.log`），格式检查与 `cargo check` 通过；生成 Rust 文件未变化。

### 优化步骤 3：每个木块只计算一次中心蒙皮

轴对齐木块的八个角共享同一个骨架绑定，现在每块只计算一次变换与中心位移，再平移八个角。平滑模式继续逐顶点变换。新增对照断言确认每个输出顶点与原逐顶点路径逐值相等，不改变方块尺寸、朝向、编辑映射或更新频率。

相同 release 单树动态风场基准最终结果（帧中位数 / p95）：

| 模式 | 优化前 | 最终 |
| --- | --- | --- |
| 平滑风动 | 18.69 / 19.73 ms | 17.185 / 18.07 ms |
| 轴对齐方块 | 64.245 / 66.75 ms | 18.34 / 19.71 ms |

最终日志与样本汇总在 `target/tree-update-perf-final`。本场景轴对齐帧耗时下降约 71%；这不代表密林或全部玩法性能已经验收。

验证：`cargo check`、`cargo fmt --check` 通过；4 项 raster tree 测试通过，包含逐顶点等价检查。真实 `--raster-tree-smoke --resize-lifecycle-test` 通过，16 条 CPU/GPU 射线一致、实际移除 6 个木体素及模式往返/生长/删除/重建通过，日志 `target/tree-skin-smoke.log`。最终默认 `cargo run --release -- --hidden --mute --auto-exit 0.5` 成功退出，无 ERROR/panic/VUID（`target/tree-update-final-default.log`，运行日志 `re-flora-20260917-024127.386-77160.log`）。

真实 A/B 裸枝和树冠截图保存在 `target/tree-update-final-visual`；已查看 B 裸枝，方块轮廓与地面阴影正常。截图用于回归留证，最终运动观感仍以用户体验为准。脚本已原样恢复 GUI/相机配置；生成文件未变化，原有 Cargo.lock 差异未纳入提交。

复用边界：树姿态与编辑源保持统一；外观适配发布对应几何，精确三角面/盒子碰撞和查询结构负责原位更新。未来调整网格外观可复用这些基础能力；引入新的几何种类时再增加对应适配，无需为每个外观复制树状态或缓存生命周期。原平滑模式与轴对齐 A/B 开关均保留。

## GPU 迁移步骤 1：常驻几何、GPU 蒙皮和木块实例化

用户澄清先前 5–6 FPS 来自 debug build，不将它作为 release 基线。此次保持现有外观和精确交互，先建立 GPU 表面生产者，不统一木材和草花的材质响应。

- 静止位置/中心/法线、骨架绑定仅在拓扑或表示重建时上传。轴对齐模式每块一条记录和绑定；骨架 palette 按 `(tree_id, branch)` 去重，0 号为根父级的恒等变换。
- compute shader 产生 GPU-only 位置/法线；颜色、阴影、射线查询共用这份表面，不再每帧上传三份展开顶点。轴对齐绘制使用一个 36-index 立方体和实例 ID；平滑模式保留原外露面索引。平滑法线使用混合线性变换的逆转置，轴对齐模式保留原静止着色法线。
- 普通帧与同步射线查询均显式记录幂等的表面生产者，由资源反射处理 compute 写与后续读之间的依赖。没有更改草花 shader 或木材 fragment 着色。
- 仍然保留 CPU 骨架求解、CPU 精确碰撞表面和查询 BVH refit；这只是 GPU 迁移第一步，**不是全 GPU 风动或最终“几乎无额外开销”验收**。尚需解决 GPU 骨架发布与 CPU 精确物理消费的时序，不能通过同步下载整份顶点或偷偷简化碰撞完成后续阶段。

release 固定单树动态风场，8 秒、排除前约 3 秒，帧中位数 / p95：

| 模式 | 本轮基线 | GPU 蒙皮后 |
| --- | --- | --- |
| 平滑 | 16.94 / 18.09 ms | 17.02 / 18.12 ms |
| 轴对齐木块 | 18.73 / 20.05 ms | 17.18 / 18.09 ms |

日志 `target/tree-gpu-baseline`、`target/tree-gpu-skin-perf`。单次场景测量，不代表密林预算或静态/风动最终增量。

验证：`cargo fmt --check`、`cargo check`、`cargo test`（1013 + 4 passed，2 ignored）。增强 `--raster-tree-smoke --resize-lifecycle-test`：显式 smoke-only 下载 GPU 表面，逐顶点对照 CPU 精确位置和法线，覆盖静态、平滑、轴对齐、年龄重建、删除重建、关闭风；最大位置误差 3.77e-7 世界单位，法线误差 2.55e-7，16 条 CPU/GPU 射线一致。所有正常帧均不下载表面。默认 release hidden/mute 0.5 秒运行正常，最终日志无 ERROR/panic/VUID。早期试运行发现 Slang 的通用 SV_InstanceID 引入未启用的 DrawParameters capability；改为项目现有的 Vulkan 原生 ID 语义后重新验证通过，没有扩大设备特性需求。

截图 `target/tree-gpu-skin-visual/{A,B}-{wood,canopy}.png`；已检查轴对齐裸枝与地面阴影，未发现明显缺面。动态截图不是逐像素等价证明，外观仍保留原模式。GUI/相机文件已恢复，生成文件没有变化，原有 Cargo.lock 差异不纳入提交。

## GPU 迁移步骤 2：分层姿态求解与精确物理发布

`tree_pose.comp.slang` 使用与草共享的 `wind_field.slang` 采样风场，在 GPU 中执行原弹簧积分、子步和父子姿态组合。一棵树由一个 invocation 顺序遍历其拓扑，不同树并行；没有深度截断或跨 workgroup 的隐式同步。静态关节与初始状态只在代次/诊断状态变化时上传，普通帧状态留在 GPU。CPU 实现仅保留为 smoke/单元测试参考与明确的强风诊断输入，不再是普通帧求解器。

提前提交独立 managed GPU job，与 GUI/CPU 工作重叠，在表面/物理发布前完成。只读回每关节 64 字节的姿态和积分状态，不读回体素顶点；CPU 精确物理和 GPU 蒙皮使用相同发布。树的代次与 revision 验证拒绝删除、同数量重建和诊断覆盖后的陈旧结果。resize 跳过帧时先收取旧作业再提交，shutdown 显式消费作业；独立求解资源不与图形描述符共享可变身份。

验证：`cargo fmt --check`、`cargo check`、全量测试 1014 + 4 passed / 2 ignored。真实 hidden/mute `--raster-tree-smoke --resize-lifecycle-test` 逐帧比较 GPU 积分与 CPU 参考（119 次，最大分量误差 4.06e-6），GPU 表面逐顶点/法线及 16 条射线对照通过，编辑/年龄/删除/重建/关闭风通过。默认 hidden/mute 0.5 秒通过，最终日志无 ERROR/panic/VUID。日志 `target/tree-gpu-pose-{check,tests,smoke,default,tail}.log`。生成文件未变化。

本次单树 release 8 秒帧中位数 / p95：平滑 17.76 / 20.04 ms，轴对齐 17.41 / 20.02 ms（`target/tree-gpu-pose-perf`）。它没有证明比步骤 1 更快，也不作为最终性能验收；smoke 中收取骨架等待中位数 100 us / p95 1649 us，包含编辑重建与诊断，不可当作稳定帧的 GPU 求解耗时。下一步必须增加明确的分段测量，继续消除 CPU 精确物理/查询侧的重复展开与更新。

## GPU 迁移步骤 3：分层并行与紧凑 CPU 交互几何

增加 `--perf` 下的 `[PERF][TREE_UPDATE]` 分段耗时和独立 GPU job timestamp，主帧增加 `tree_skin.pass` GPU scope。先测量发现单 invocation 的姿态求解约 459 us；改为每树一个 workgroup，同深度关节并行，每层/子步通过显式 `AllMemoryBarrierWithGroupSync` 保证父级发布，无固定树深/关节数限制。相同 release 场景的 pose job GPU 中位数降至约 75 us。

CPU 交互表面现在显式区分 `Blocks { centers }` 与 `Triangles { positions }`：轴对齐只存中心，不展开八角；平滑只算物理需要的位置，不再计算无人消费的逆转置法线。射线与 smoke 通过位置访问器按需取得角点；GPU 法线对照使用该表面实际上传的骨架 palette，仍逐顶点验证，不依赖当前树可能已被诊断改写的状态。

同一 release 脚本，稳定帧 CPU 中位数（us）：

| 模式 | CPU skin 前/后 | 物理发布 | CPU 查询层发布 | 全树更新 CPU 合计 |
| --- | --- | --- | --- | --- |
| 平滑 | 1492 / 942 | 321 | 888 | 2293 |
| 轴对齐 | 579 / 340 | 153 | 620 | 1254 |

姿态提交约 74–77 us、收取等待约 33–36 us，GPU skin 约 15–19 us。所有关节/物理依然每帧更新。整帧中位数 / p95 为平滑 17.08 / 19.49 ms、轴对齐 17.31 / 19.83 ms。日志 `target/tree-gpu-{update-profile,parallel-perf,compact-perf}`；分段数据说明下一处大头是重复 CPU 查询 BVH，不以整帧的小变化冒充最终验收。

验证：全量 1014 + 4 passed / 2 ignored，`cargo check`、格式检查通过；`--raster-tree-smoke --resize-lifecycle-test` 的 GPU 积分/表面/法线/射线对照、编辑和生命周期全部通过；默认 release hidden/mute 0.5 秒及日志检查通过，无 ERROR/panic/VUID。日志 `target/tree-gpu-compact-{smoke,default,tail}.log`。生成文件未变化，既有 Cargo.lock 差异保留。

## GPU 迁移步骤 4：GPU BVH 更新与共享 CPU 精确查询

GPU 查询 BVH 使用拓扑编译时生成的按深度调度，compute 每层并行 refit，反射资源依赖保证子节点写入完成后父级才读取；节点缓冲现在是 GPU-only，不再每帧上传 CPU refit 结果。普通渲染与即时 GPU 射线共用同一幂等表面/BVH 生产者。静态关闭风时不运行动态 BVH 更新。

CPU 编辑射线改为从物理库获取当前精确碰撞 BVH 的候选 primitive，再使用原来的表面求交和静止坐标映射；没有重复维护第二棵 CPU 动态树，也没有简化碰撞、降低刷新频率。物理接口隐藏 Parry 实现，仅返回当前 geometry 的 primitive ID。新增测试覆盖移动、内部起点、同数量拓扑替换、三角面/盒切换、无效发布不污染旧结果、删除和无效射线。

release 单树动态风场（`target/tree-gpu-refit-perf`），CPU 中位数：平滑 skin 944 us、物理 329 us、查询发布 27 us、更新合计 1455 us；轴对齐 skin 344 us、物理 153 us、查询发布 9 us、更新合计 660 us。GPU 姿态约 74 us，蒙皮+BVH 全部更新约 68 us。整帧中位数 / p95 平滑 17.355 / 19.35 ms、轴对齐 17.46 / 19.13 ms；仍需重复静态对照，不从单次整帧测量声称最终达标。

验证：root 全量 1015 + 4 passed / 2 ignored；物理库 50 项通过；`cargo check`、root 格式检查、diff 检查通过。真实 smoke/resize 在平滑 31,983 节点、轴对齐 21,191 节点和幼树 2,141 节点逐节点对照 CPU refit，通过精确位置/法线、16 条 CPU/GPU 射线、编辑和生命周期检查。默认 release hidden/mute 0.5 秒及运行日志无 ERROR/panic/VUID。日志 `target/tree-gpu-refit-{smoke,default,tail}.log`。生成文件未变化；物理 crate 中既有无关格式差异未混入，原 Cargo.lock 改动保留。

## GPU 迁移验收候选：重复 release 基准与人工检查入口

扩展 `scripts/benchmark_tree_update.py`，保留旧的默认 smooth/blocks 与 `--binary` 用法，增加 static 对照、重复次数、每轮轮换模式顺序和 wall-clock warmup。报告同时保存整帧、树 CPU 分段与 GPU timestamp 的中位数/p95/样本数。三项快速解析/CLI 测试覆盖午夜时间、按帧关联、旧日志兼容与错误恢复；定向 ruff 检查通过。配置修改全程持 GPU 锁并在 finally 原样恢复。

最终命令：

```sh
python3 scripts/benchmark_tree_update.py --output target/tree-gpu-final-perf \
  --modes static smooth blocks --repeats 3 --seconds 12
```

每模式三次、每次 12 秒，排除首个帧日志后的 3 秒。固定单树（轴对齐 10,596 木块）、同相机/光照、真实风场、hidden/mute、默认自动 present mode。没有冻结姿态或降低更新频率。

| 模式 | 稳定样本 | 帧中位数 / p95 | 主帧 GPU 中位数 |
| --- | --- | --- | --- |
| 静态木材 | 1288 | 17.28 / 19.62 ms | 7.910 ms |
| 平滑风动 | 1290 | 17.35 / 19.55 ms | 8.224 ms |
| 轴对齐风动 | 1303 | 17.33 / 19.31 ms | 8.398 ms |

轴对齐相对静态的整帧中位数差约 0.05 ms（约 0.3%），该场景基本不影响整帧吞吐。**这不代表工作量为零**：主帧 GPU 差约 0.49 ms，受更多面、变形后的遮挡与查询等影响；树 CPU 更新合计约 0.648 ms（平滑约 1.442 ms），CPU 精确物理仍计算紧凑中心/必要位置、更新碰撞 BVH。独立骨架 GPU job 约 0.074 ms，GPU 蒙皮+查询 BVH 约 0.067 ms，骨架收取等待中位数约 0.042 ms。CPU、GPU 与帧获取存在重叠，不能把各列简单相加当作整帧差值。

对照边界：静态模式关闭的是木材动态表面；为保持既有开关语义，骨架状态仍持续演化，故静态对照同样含约 0.074 ms 的 GPU 骨架工作与约 0.140 ms 的 CPU 提交/收取。静态采用外露面、轴对齐采用完整木块，所以表中是整条路径对比，不是相同三角形数下的纯动画核对比。完整报告 `target/tree-gpu-final-perf/summary.json`。没有把单树结论扩展为密林、其他 GPU 或无 present 等待的通用预算。

最终视觉留证：`target/tree-gpu-final-visual/{A,B}-{wood,canopy}.png`，已检查轴对齐裸枝与有叶截图，树干、树冠、地面阴影无明显缺失；材质/着色风格未更改。GPU/CPU 积分、逐顶点法线、逐 BVH 节点、精确射线、挖掘、生长/删除/重建、resize、物理角色/刚体测试以及默认隐藏运行均已完成。现在剩下的是人工验收运动观感、枝叶跟随、移动树干的碰撞/挖掘手感；不自动启动可见游戏。

人工入口仍为 `R → Debug → Whole Tree Rasterization`：开启整树光栅化和 wind，使用已有 axis-aligned 开关对比平滑/轴对齐。性能判断用 release，不拿 debug FPS 判准出。用户确认后再讨论树梢/草花的材质风格统一；本轮没有新增材质 A/B 或偷偷改变交互语义。
