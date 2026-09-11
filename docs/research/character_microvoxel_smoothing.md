# 微体素地形行走平滑调研

日期：2026-09-11。代码基线：`c577702e28dc625a0546a68ec84c48b6e1c9b1a8`。本次只读代码、历史和外部主源；未修改运动算法，未运行游戏或基准。以下区分代码事实、推断和待验证建议。

## 结论

继续使用现有胶囊控制器是合理的。项目已经有自动跨台阶、向下胶囊扫掠贴地，以及与碰撞位置分离的镜头高度平滑；问题不能概括成“缺一个 smoothing”。最值得先验证的是：**镜头平滑何时被跳过，以及8体素的高度误差限制是否把允许跨越的16体素台阶重新变成了明显跳变**。如果实际横向前进也卡顿，则应在现有碰撞/跨步层定位，镜头滤波无法修复真实的停走。

旧方案确实包含空间平均。迁移前最近版本是9条向下射线做高斯平均，另有9条向上和32条环向射线，总共50条；再对镜头高度做时间插值。这解释了用户记忆中的“几十条射线”和较柔和观感。值得恢复的是空间支撑估计的思想，不必恢复旧的整套碰撞系统。

## 现在实际使用什么

调用链是 `update_camera_for_current_mode` → `prepare_walk_movement` → `TerrainPhysics::move_player_capsule` → `CollisionWorld::move_capsule_character` → `apply_walk_movement`。[输入入口](../../src/app/core/input.rs)、[物理适配](../../src/app/core/physics.rs)、[胶囊控制器](../../crates/re-flora-physics/src/lib.rs)、[相机](../../src/gameplay/camera/controller.rs)

- 当前实现是 Rapier 0.34 的 `KinematicCharacterController`，在静态体素碰撞世界上移动查询胶囊，并非动态刚体自然滚动；树果等动态物体不阻挡该查询。
- `autostep` 开启。Rapier 内建 `snap_to_ground` 关闭，因为项目用同一 terrain-only 查询管线上的显式 downward **capsule shape cast** 做贴地；这是具有体积的扫掠，不能等同于一根脚下射线。
- 移动前向下查询判断初始支撑；移动后满足非向上移动及已有支撑条件时，再向下扫掠并减去命中距离。地面法线要求 `normal.y >= 0.5`。
- `walk_collision_position` 是碰撞权威位置。渲染位置 X/Z 直接跟随它，Y 仅在 `grounded` 时进行平滑。脚步位置取碰撞位置，镜头另叠加 head bob。
- 当前每个渲染帧调用一次角色移动。动态刚体 fixed-step 的存在不意味着角色查询也按该 fixed-step 更新。[实际入口](../../src/app/core/input.rs)

### 尺度必须按角色比例理解

物理查询使用体素单位，渲染世界坐标乘256进入物理、除256返回。不能直接照搬其他引擎示例中的“0.4米”。下表来自上述适配/控制器和 [CameraDesc](../../src/gameplay/camera/desc.rs)，是源码默认值，不代表用户运行时设置一定相同。

| 参数 | 当前数值 | 含义 |
|---|---:|---|
| 胶囊半径 / 圆柱半高 | 4 / 8 vox | 总高度24 vox，宽度8 vox |
| 相机离脚高度 | 0.08 world = 20.48 vox | 16 vox台阶相当于约78%的眼高 |
| 最大自动跨步高度 / 最小台面宽 | 16.05 / 0.5 vox | 非常宽松的通行策略 |
| 贴地最大距离 / 初始接触查询距离 | 16.25 / 1.25 vox | 下台阶也可能产生较大单帧位移 |
| 碰撞间隙 | 0.1 vox | 数值稳定边距，不宜当平滑强度 |
| 最大爬坡角 / 最小滑坡角 | 60° / 60° | 体素棱角接触仍需实测 |
| 镜头指数平滑速率 | 14 s⁻¹ | 半衰期约49.5 ms；60 Hz单帧权重约0.208 |
| 允许平滑的单帧地面Y位移 | ≤16.5 vox | 超限立即对齐碰撞高度 |
| 镜头最大Y滞后 | 8 vox | 对平滑后误差做硬限幅 |
| 默认移动输入速度 | 64 vox/s；跑步倍率2.2 | 加速/drag之后的实际速度另算 |

从公式可以直接算出：平稳起步跨上16 vox，60 Hz指数插值本想只移动约3.33 vox；剩余12.67 vox超过8 vox滞后上限，因此输出会改成首帧移动8 vox。**这是公式可证的行为，不是已复现的用户症状根因。** 单独减小平滑速率不会去掉这次限幅跳变。

## 旧方案查证

迁移提交 `5f808f5f`（2026-07-19，`use capsule controller for player movement`）的父版本：

- `src/app/core/input.rs::query_player_collision_cpu`：3×3下射线，采样间距1 vox；权重 `exp(-(x²+y²)/(2σ²))`，σ=1。返回距离加权平均。
- 同函数另查3×3天花板和32条水平环向射线；旧 `shader/slang/player_collider.slang` 也能看到同样的50射线布局。
- `src/gameplay/camera/controller.rs::update_transform_walk_mode` 使用 `Y_SMOOTHING_ALPHA=0.2`，每帧把相机移向平均地面高度加眼高。该固定权重依赖帧率，60 Hz恰好与当前14/s相近。
- 下射线调用 `contree_builder.query_terrain_ray_cpu`；本次检查的迁移前路径没有 Recast 导航库。这里应称旧 **raycast** 碰撞方案，不能认定它依赖 Recast。更早版本未穷尽，用户可能指另一阶段。
- 当前镜头分离和平滑来自 `3f342a6f`，之后 `0db31483`、`af3c575c` 放宽了跨步参数。

可复查命令：`git show 5f808f5f^:src/app/core/input.rs`、`git show 5f808f5f^:src/gameplay/camera/controller.rs`、`git show 3f342a6f`。这些历史文件已变化，引用提交比当前行号更可靠。

旧方案把空间平均结果直接用于身体/镜头高度，能削弱小齿面，但不是严格不穿透的支撑约束。尤其旧下射线 miss 以 `1e10` 参与平均，不能原样复用到悬崖边。应借鉴平均思想，保留现有胶囊的碰撞权威。

## 外部实现给出的证据

### Luanti：方块通行和镜头平滑是两个步骤

Luanti 的 `collisionMoveSimple` 判断障碍顶面是否在 stepheight 内，并检查抬高后头顶空间；相机则单独检测向上台阶运动，以 `exp(-23*frametime)` 跟随角色高度。它支持“碰撞先正确跨过去，眼睛随后柔和跟上”的拆分，未证明脚下必须取很多射线平均。[碰撞源码](https://github.com/luanti-org/luanti/blob/master/src/collision.cpp)、[相机源码 Camera::update](https://github.com/luanti-org/luanti/blob/master/src/client/camera.cpp#L289-L319)

### Rapier：跨步、贴地和碰撞间隙各有职责

官方指南分别定义自动跨步的高度/宽度约束、下坡贴地条件和稳定用的 offset。项目依赖版本源码可核查 `move_shape` 行为；在线指南当前标注0.32，与本项目0.34存在版本差异，具体集成应以锁定版本为准。现有显式贴地不能仅因看到 `snap_to_ground: None` 就判定缺失。[官方指南](https://rapier.rs/docs/user_guides/rust/character_controller/)、[Rapier v0.34.0 源码](https://github.com/dimforge/rapier/blob/v0.34.0/src/control/character_controller.rs)

### Jolt：成熟控制器同样显式处理上台阶和下坡

`CharacterVirtual::ExtendedUpdate` 组合正常移动、`StickToFloor` 与 `WalkStairs`；参数分别描述抬升、前探、下探。说明“贴地/跨步”是可独立管理的角色策略，不能期待碰撞解算器自动提供舒适的视点轨迹。它是设计参考，不建议更换物理引擎。[官方源码](https://github.com/jrouwe/JoltPhysics/blob/master/Jolt/Physics/Character/CharacterVirtual.h)

### Teardown：微体素游戏的公开资料边界

官方 API 区分玩家底部 transform 与相机 transform，也提供场景射线、最近点等查询。但所查公开接口不展示内置行走的支撑平均或台阶平滑实现。因此无法据此声称“Teardown 使用N条射线平均”或承诺复制其手感；微体素领域本次得到的是接口证据，未获得可验证的内置算法细节。[玩家 transform](https://teardowngame.com/modding/api.html#GetPlayerTransform)、[场景射线](https://teardowngame.com/modding/api.html#QueryRaycast)、[玩家参数](https://teardowngame.com/modding/api.html#SetPlayerParam)

上述主源访问于2026-09-11；除 Rapier 版本固定链接外，master/API页面后续可能更新。

## 如何在当前算法上做小而好维护的改动

| 层 | 解决的问题 | 建议 |
|---|---|---|
| 跨步/滑动 | 身体被微小立面卡住，横向停走 | 保留KCC，先查实际位移和接触；不要盲目再加大16 vox上限 |
| 支撑/贴地 | 下坡失去接触、落下又抬起 | 保留当前胶囊扫掠；确认失败是否来自棱角法线、接缝、地形更新时序 |
| 空间支撑估计 | 单个齿尖令观察高度突变 | 如果时间滤波不足，局部少量采样只生成观察目标 |
| 镜头时间滤波 | 身体跨步后的眼睛突然跳变 | 首选修改现有平滑模块的触发/限制语义，避免再叠一个平滑器 |

### 推荐的第一步：先把现有平滑变成可诊断的完整链路

以同一路线记录碰撞Y、镜头Y、请求/实际XZ位移、grounded、地面法线、贴地距离、平滑被跳过原因及限幅次数。物理层已经返回碰撞信息，适配层目前只传 `translation/grounded`；未来可用一个紧凑的可选诊断结果传出必要数据，避免新增另一套角色状态。

如果身体XZ匀速、碰撞Y合理，但画面仍跳，优先在现有 `smoothed_grounded_camera_y` 中解决：把“单次台阶允许的观察误差”和“持续上下坡不应无限落后”分开约束，按有效步高/眼高审视8 vox上限；跳跃、传送、真实悬空仍走明确状态切换。不能只延迟 grounded=false 来掩盖真实离地，也不能用无限镜头滞后换柔和。

如果KCC实际XZ停走，镜头层保持不动，先从真实半径4/半高8的复现路径查 autostep 与接触。当前胶囊单测默认半径0.5/半高1，与游戏不同；即使既有台阶测试通过，也不足以证明游戏尺度下的连续粗糙坡面手感。[现有测试](../../crates/re-flora-physics/tests/capsule_character.rs)

### 第二步候选：有界空间支撑估计，复用同一碰撞世界

若单纯修正时间跟随仍不理想，再试9个足底附近下探点（3×3、间距先从1 vox开始），半径和高度窗口都限定在现有胶囊脚部周围。优先在 `re-flora-physics` 内复用角色 terrain-only 查询管线；无需重建导航网格、复制地形缓存，也不应给相机引入一套与碰撞版本不同步的地形查询。

这是待试验设计，尚非经过验收的参数：仅接受可行走法线及脚底附近的有效命中，按相对当前支撑高度筛除悬崖下层/洞穴另一层，再对有效值高斯加权；miss不作为巨大距离或零高度加入。样本覆盖不足就回退当前碰撞眼高。估计只给镜头目标，不回写胶囊位置、grounded、跳跃或脚步事件。已有时间滤波统一负责收敛。

均值会低于局部最高齿尖，镜头位移必须保持合理眼高与头顶空间；狭窄台沿、低顶通道需要保守回退。是否必须新增眼部碰撞查询，应由这些场景证据决定。如果需要大量例外才能成立，应止步于第一步，而非扩成另一套角色控制器。少量射线不是免费性能保证；进入实现阶段再测每帧查询成本。

## 后续验收路径（本次未执行）

1. 使用游戏真实胶囊尺寸，固定路线覆盖平地、1/2/4 vox齿面、连续16 vox上下台阶、17 vox阻挡、斜向跨砖接缝、狭窄台沿、低天花板、悬崖、跳起落地和现场地形编辑。
2. 先关 head bob 的诊断对照，区分步态摆动与地面阶跃，再在原 head bob 设置下验收，不能把永久关闭它当作修复。
3. 比较30/60/120 Hz：XZ实际速度损失、地面状态切换次数、Y峰值速度/加速度、镜头滞后分布；确定性路径检查负责穿透/通行，真实游戏录像与手动步行负责舒适度。
4. 视觉候选可先展示；性能另用release实际运行测查询耗时和帧时间。相机公式单测或隐藏运行不构成舒适度验收。

建议首个实现只覆盖“现有镜头平滑触发和限幅”的一条证实原因；空间采样作为下一步候选保留。这样可以回到旧方案的舒适方向，同时让地形碰撞与角色移动继续只有一个权威实现。
