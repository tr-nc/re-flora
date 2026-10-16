# 玩家地形通行：跨小障碍不等于沿墙攀升

> 后续实现与验证：[小地面障碍通行与防爬墙闭环](../evidence/walking-ground-detail.md)。下文保留实现前的调研状态；目前已提供可保存的运行时 A/B 开关和地面起伏容许量。

日期：2026-10-17。代码基线：`d984399d78fca9d29ab03d36e35c9148b2d5199e`。

状态：主源调研与行为边界讨论，**不是已确认的故障根因或已批准的实现方案**。用户报告上次阻止贴墙越障后，一些预期可越过的地形也被阻挡；尚无该场景的地形、行走路线或确定性复现。本次不修改运行时代码、参数或保存配置。

## 结论

应把三个问题分开：**身体是否被阻挡、某处能否作为站立支撑、从当前地面能否安全跨到另一个落点**。只看单次接触法线、单帧抬升量或一个统一的落脚宽度，不能完整表达玩家对“跨过小凸起”和“借凸起爬上一面墙”的不同预期。

查到的 FPS、开放世界再实现和可编辑体素引擎，都有显式的台阶处理；它们的碰撞形状、资产约束和允许动作不同，没有一个可直接照搬的万能阈值。建议保留现有碰撞世界和胶囊控制器，先用一对实际地形确定通行契约，再调整已被复现证明有问题的判定。不要先删防爬保护，也不要先提高步高或全面平滑碰撞几何。

## 1. 影响范围与当前证据

### 已确认的实现

- 玩家使用半径4、圆柱半高8体素的胶囊，总高24体素；这不是相机本身与地形做点碰撞。[L1]
- 普通行走跨步上限是完整身高的一半，即12体素。主动向上移动保留独立的既有自动跨步参数，不能把普通行走与跳跃测试混为一谈。[L2]
- 显式跨步路径进行全胶囊的上、前、下扫掠，再检查实际接触高度和 `has_foothold`。[L3]
- `has_foothold` 在接触附近搜索多个方向和侧向偏移的支撑区；每个候选用3×3射线采样，宽和深各为胶囊半径。对玩家约为4×4体素的**采样区域**，并非对连续区域无孔洞的数学证明。[L4]
- 显式跨步未采用、但 KCC 仍产生向上位移时，也执行同一个落脚检查；失败则从原姿态按不可上爬的碰撞重新求解，不是只把已解算位移的 Y 清零。[L2]
- 该底层移动接口也由割草机调用，其体型不同。若改变共用规则，需要覆盖该调用方，不能只在 Camera 端补偿。[L1]

### 本次验证

已运行：

```bash
cargo test --manifest-path crates/re-flora-physics/Cargo.toml \
  --test character_support --test capsule_character
```

输出：`capsule_character` **23 passed**，`character_support` **5 passed**，零失败。覆盖包括宽台阶、宽坡、墙上单体素窄脊、步高边界、部分转角、帧率/体型/砖接缝和地形重建。[L5]

这证明既有用例仍符合当前规则，**不证明用户报告不存在**。目前没有能对用户这次确切症状判红的命令，也没有实际游戏场景对照，故不宣称已复现或已定位根因。上述测试也不是 Release 性能或手感验收。

本次 `cargo build --release` 通过，文档差异空白检查通过。只新增调研文档；未修改 Rust、shader、生成文件或 GUI 配置，未启动可见游戏，也未将启动冒烟测试当成通行复现。

## 2. 根因状态：已知规则，未知具体误判

已确认的是：普通跨步与非跳跃的 KCC 上升共同受落脚区规则约束。待确认的是：用户被挡的位置究竟失败于步高、支撑区、头顶/前方空间、接触几何，还是碰撞发布等其他环节。

一个需要用实景澄清的**行为问题**是：玩家是要站上那块表面，还是只经过它、落到后面的地面？前者要求可用支撑；后者的设计可以允许途中没有稳定站位，但必须验证完整身体路径及最终落点。这是候选产品规则，不是已经证明当前故障由此造成。

此外，如果两个场景在控制器可见的几何范围、角色状态和输入上完全相同，却要求不同结果，单靠调整局部数值不可能分辨。必须选择可观察的区别：更大但有界的地形上下文、明确的表面通行语义，或者不同的玩家动作；这些区别也应让玩家看得出来。

## 3. 何时引入

- `d08deedf`，2026-10-09：将普通行走步高限制为半个完整身高。
- `783526e3`，2026-10-09：增加可用落脚面要求，阻止沿狭窄墙脊上升。
- `8ad5cb8b`，2026-10-11：将两项改动移入当前主线，同时补充步高、窄脊、宽楼梯和跳跃回归。[L6]

这些是**规则的引入时间**；尚不能把这次未复现的通行问题断定为该提交引入。更早的[微体素行走平滑调研](character_microvoxel_smoothing.md)记录的是旧基线，里面的16.05体素普通步高等数值不再描述当前普通行走策略。

## 4. 其他实现如何处理

以下源码和官方文档均在本次直接读取。GitHub `master` 链接会变化；所述为访问时实现。引擎机制不能自动当作某个商业游戏的实际参数或内部算法。

### FPS：Valve Source SDK 的两条移动路径

`CGameMovement::StepMove` 先保存普通滑动的结果，再从原位置尝试“向上不超过角色步高 → 向前移动 → 向下落回”的路径。下降接触法线的向上分量不足0.7时，退回普通移动；之后比较两条路径的水平进展。[S1]

借鉴：**台阶通行是一个单独验证的移动候选，而不是把墙接触直接改成坡。** 它使用玩家包围盒，不能照搬其接触法线结论到我们的圆底胶囊，也不应把源码注释里的旧固定数值当成所有 Source 游戏的统一步高。

### 开放世界：OpenMW 的明确跨步与资产兼容边界

OpenMW 的 `Stepper::step` 也是上、前、下扫掠；落点检查拒绝空气、不可走坡面及其他角色。代码还保留额外固定距离尝试，并明确把它们标为对原版 Morrowind 资产的兼容措施，建议完整改版项目去掉这些措施。[S2]

借鉴：**自由探索不意味着所有微小碰撞细节都能当楼梯。** 通行规则与资产形状需要共同成立。OpenMW 是 Morrowind 的开源引擎再实现，不是 Bethesda 原版源码；不能据此声称 Skyrim 或其他 Bethesda 游戏采用同一实现，也不应把其资产补丁复制给可自由编辑的地形。

### 可编辑体素：Luanti 的几何步高和净空规则

Luanti 的 `collisionMoveSimple` 收集移动范围内的碰撞盒，对侧向碰撞检查障碍顶面相对脚底的高度是否在 `stepheight` 内，并检查抬高后的头顶空间；还会稍向前探测，以避免堆叠台阶带来的错误。不同 `StepUpMode` 对垂直速度有不同要求。[S3]

借鉴：动态地形也可以使用确定的几何通行规则，不必逐个手工标注障碍。但这里使用 AABB/碰撞盒，不是我们的微体素胶囊；“方块引擎这样处理”并不证明把微小体素棱角全部当作合法台阶就有合适手感。

### PhysX：圆底胶囊确实可能绕过名义步高

PhysX 官方专门解释：即使 `stepOffset=0`，圆底胶囊也可能碰到小障碍后向上移动；叠加自动跨步后，最大可越高度还受胶囊半径和位移量影响。因此它区分宽松和受约束的胶囊攀升模式，并单独提供不可走坡面的处理。[S4]

借鉴：上次防止胶囊沿接触上升的动机有明确的同类机制依据。**只关自动跨步、只改步高，未必能阻止擦墙攀升。** 这是 PhysX 的机制说明，不是 Rapier 或本次用户场景已被证明相同的结论。

### Jolt：跨步探测不能只依赖一帧前进距离

`CharacterVirtual::WalkStairs` 的文档区分实际跨步前进距离和额外前探距离：高更新频率下，单帧前进太短，向下扫掠可能仍命中楼梯侧面并得到过陡法线；额外探测用于确认前方是否存在合法坡面。`ExtendedUpdate` 示例分别组合正常移动、贴地和跨台阶。[S5]

借鉴：**探测范围与当前帧应执行的位移是两个概念。** 有界前探可以提供上下文，但不能据此无条件把角色瞬移到探测落点，或把它解释成自动跨越任意深度的沟。这里是候选实现思路，未证明我们当前受帧率影响。

### Unreal：碰撞、可跨上和可站立可以有不同语义

Epic 的 `CanCharacterStepUp` 文档明确区分“碰到组件时可尝试跨上去”与“落到它上面后能否行走”。`Walkable Slope Override` 则允许按物理对象调整坡面规则。Simple/Complex Collision 文档展示视觉网格和用于查询/碰撞的几何可以不同。[S6–S8]

借鉴：对有明确语义的建筑或道具，可采用有视觉依据的对象通行策略；对随意雕刻的地形，仍需要默认一致的几何规则。不能用全局不可见阻挡或任意简化，把玩家看得见的小通道、洞口和台阶封掉。

## 5. 适合 Re: Flora 的干净处理方向

### 先定义可观察行为，不先决定算法

下表是待与用户实际例子对齐的建议，不是新增玩法承诺：

| 情况 | 建议的默认预期 | 关键区别 |
|---|---|---|
| 矮小凸起，前后均有正常地面 | 按前进应顺畅跨过 | 检查从起点到合法落点的有界身体路径；不一定需要在凸起上停住 |
| 可站立台阶、正常坡面 | 连续行走可上去 | 台面/坡面提供合法支撑；满足步高、坡度及净空 |
| 陡墙上的连续小棱脊 | 持续按前进也不能逐帧越爬越高 | 不能让无效支撑反复刷新跨步起点，累计获得高度 |
| 超过普通步高的矮墙 | 普通行走阻挡；跳跃按既有规则处理 | 若以后增加翻越，作为明确动作设计，不能偷偷混进行走 |
| 低顶、薄墙、悬空断层 | 不穿透、不凭空悬浮跨越 | 舒适度不覆盖完整身体碰撞及支撑约束 |

如果用户实际例子确认属于第一类、且当前拒绝确实由中途支撑要求造成，最小候选是调整**现有移动路径的可接受条件**：保留全身体扫掠和有效落点检查，让短距离经过与持续上升不再机械共用同一条件。具体是否需要前探、跨帧过渡或更明确的支撑状态，要由最小复现决定；不要为了猜测的问题先建设另一套角色状态机。

此候选必须同时证明不能沿窄脊累计爬墙。单纯放松 `has_foothold`、删掉 KCC 上升复核、平均掉真实障碍，均不是完成的根因修复。

### 唯一权威与兼容约束

- 通行正确性继续归 `CollisionWorld::move_capsule_character` 及其步进/支撑模块；Camera 的视觉平滑不能修改通行资格。
- 复用已发布的体素/固定表面碰撞世界，不引入第二份地形权威；保留悬崖、低顶、角落、薄墙及编辑后版本一致性。
- 通用算法改变须验证玩家与割草机。若产品需求确实不同，显式表达角色通行策略，而不是在调用方伪造位移或用体型暗中区分。
- 若需要可视 A/B，按项目要求在 Debug 面板提供运行时原版/候选复选框；本次没有实施或声称已完成 A/B。
- 单独评估视觉手感与 Release 性能；本次未做性能测量，不给查询数量或耗时保证。

### 最有价值的下一步

1. 收集一对用户认可的地形：**这里应当能过、那里绝对不能过**，并确认是按前进、跳跃还是期待翻越；记录起点和方向，而不只记录障碍高度。
2. 用真实4/8胶囊、实际移动调用模式复现；区分步高、净空、前方阻挡和支撑拒绝，建立可以稳定判红的一条命令。
3. 最小修改之后，同时检查矮凸起、正常台阶、长时间顶墙窄脊、斜向接近、30/60/120 Hz、低顶、悬崖、跳跃以及现场地形编辑；验证不能通过帧率或方向改变得到截然不同的可通行类别。
4. 再用原生 Release 游戏验收通行预期和手感，不能以合成用例通过替代用户场景确认。

## 来源

### 项目内证据

- [L1] [surface_motion.rs](../../src/app/core/physics/surface_motion.rs)：`PLAYER`、`MOWER`、`SurfaceMover::move_by`。
- [L2] [re-flora-physics/src/lib.rs](../../crates/re-flora-physics/src/lib.rs)：`CollisionWorld::move_capsule_character`，基线约610–780行。
- [L3] [character_step.rs](../../crates/re-flora-physics/src/character_step.rs)：`try_step`。
- [L4] [character_support.rs](../../crates/re-flora-physics/src/character_support.rs)：`has_foothold`、`patch_is_supported`。
- [L5] [capsule_character.rs](../../crates/re-flora-physics/tests/capsule_character.rs)、[character_support.rs](../../crates/re-flora-physics/tests/character_support.rs)。本次运行结果见第1节。
- [L6] `git show 8ad5cb8b`、`git show -s --format=fuller d08deedf 783526e3`；[主线移植记录](../evidence/stone-tools-integration.md)。

### 外部主源

- [S1] [Valve Source SDK：gamemovement.cpp](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/shared/gamemovement.cpp)，`CGameMovement::StepMove`。
- [S2] [OpenMW：stepper.cpp](https://github.com/OpenMW/openmw/blob/master/apps/openmw/mwphysics/stepper.cpp)、[movementsolver.cpp](https://github.com/OpenMW/openmw/blob/master/apps/openmw/mwphysics/movementsolver.cpp)、[constants.hpp](https://github.com/OpenMW/openmw/blob/master/apps/openmw/mwphysics/constants.hpp)。
- [S3] [Luanti：collision.cpp](https://github.com/luanti-org/luanti/blob/master/src/collision.cpp)，`collisionMoveSimple`、`wouldCollideWithCeiling`。
- [S4] [PhysX 5.6：Character Controllers](https://nvidia-omniverse.github.io/PhysX/physx/5.6.0/docs/CharacterControllers.html)，Auto Stepping、Climbing Mode、Walkable Parts & Invisible Walls。
- [S5] [Jolt：CharacterVirtual.h](https://github.com/jrouwe/JoltPhysics/blob/master/Jolt/Physics/Character/CharacterVirtual.h)，`WalkStairs`、`ExtendedUpdateSettings`、`ExtendedUpdate`。
- [S6] [Epic：Can Character Step Up](https://dev.epicgames.com/documentation/en-us/unreal-engine/BlueprintAPI/Collision/CanCharacterStepUp)。
- [S7] [Epic：Walkable Slope](https://dev.epicgames.com/documentation/en-us/unreal-engine/walkable-slope-in-unreal-engine)。
- [S8] [Epic：Simple versus Complex Collision](https://dev.epicgames.com/documentation/en-us/unreal-engine/simple-versus-complex-collision-in-unreal-engine)。
