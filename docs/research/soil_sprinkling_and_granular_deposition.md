# 缓慢撒土：游戏交互、颗粒堆积与地形沉积调研

日期：2026-10-07

调查基线：`adde77ac`，`terrain-magic` 分支。

范围：调研与讨论；不修改 Rust、shader、配置或游戏行为。本文不是已批准的实现规格。

后续讨论更新（2026-10-07）：用户确认铺土只负责增加土量，土壤改良另由工具负责；当前阶段只实现渐进覆盖与堆高，空中撒落效果推迟。已实现版本与验证边界见 [渐进铺土与堆高](../soil_deposition.md)。以下保留为调研记录。

## 结论先行

用户描述的是一种**持续投放材料**的动作，不只是一个不规则的地形笔刷：

```text
少量土不断出现 → 落下或贴近地面落定 → 少量滚动／滑落 → 逐渐成为地形
```

现有 edit tool 的规则、均匀增土符合其几何编辑语义，不是 bug。可以保留它的精确造型能力，再讨论独立的“撒土／倒土”动作。

本次调研支持三个判断，但不支持宣称所有游戏都采用同一种实现：

1. 2D falling-sand 游戏提供了明确的“投放材料，让局部运动规则决定结果”的参照。[S1][S2]
2. 工业土方模拟有明确的“动态土颗粒 ↔ 稳定地形”混合表示实例，而不是必须让整个世界永远逐粒运动。[S5]
3. 学术方法直接相关，但当前表达需求不自动要求 DEM、PBD 或完整的 MPM 沙土本构。先确定玩家看见什么、土落定后能做什么，再选择模拟层级。[S7][S8][S9]

**初步讨论方向，不是实施决定**：有少量可见落土、小土团和局部滑落，稳定后成为可继续编辑的地形。它可以使用简化规则，也可以使用局部颗粒求解；现阶段没有理由直接升级整个世界的物理模型。

## 1. 先区分三种动作

| 动作 | 玩家在控制什么 | 停留在同一点 | 沿平板移动 |
| --- | --- | --- | --- |
| 地形笔刷／造型 | 一个空间范围及其增减操作 | 在范围内按规则增厚 | 得到宽度、边缘比较稳定的笔划 |
| 撒土／撒播 | 单位时间释放的量、散布范围 | 零散覆盖逐渐增加，之后可能成小堆 | 留下疏密变化的小颗粒和小土团 |
| 倒土／倾倒 | 较集中的物料流及出料口 | 形成更集中的堆，局部向外滑落 | 留下较连续的土脊 |

这里的区别是产品语义推导，不是某个引擎提供的标准工具分类。

撒土的“缓慢”主要应是**出料慢**，而不一定是每颗土都以慢动作落下。近地面少量释放、正常重力下很快落定，也可能比长距离悬浮粒子更像园艺动作。

“自然”也不能简单等同于“随机”。独立随机采样在持续投放后，平均覆盖反而趋向均匀。需要讨论的还包括：土团的空间相关性、出料节奏、落点受到地形影响、落定后的摩擦和有限滑动。固定出料口在理想平面上形成比较对称的堆，也完全可能符合真实行为。

## 2. 游戏界的参照

### Noita：材料行为，而非画出最终轮廓

官方 FAQ 明确说明：每个像素有材料，水表现为液体、气体上升、沙子会堆起来；其引擎名为 Falling Everything。[S1]

对本题的启发是：新增材料不是最终形状，新增后还有运动和落定的过程。这是 2D 撒土最直接的交互参照。

**证据边界**：官方描述足以支持材料会堆积，但不足以确认其精确邻域规则、扫描顺序或粒子／网格转换方式。GDC 演讲《Exploring the Tech and Design of 'Noita'》是后续观看入口；本次未成功取得完整演讲，因此不把网络转述的 chunk 尺寸、线程策略作为已核实事实。[S12]

### The Powder Toy：可直接查验的粉末类别

开源 `SAND.cpp` 把沙子配置为 `TYPE_PART`，`Falldown = 1`，`Gravity = 0.3f`，并置于粉末菜单。它具有明确的粉末运动属性，而不只是沙色笔刷。[S2]

一般 falling-sand 思路可以概括为：有空位时向下移动，受阻时允许有限的侧向／斜向移动，无法移动时稳定。但这是解释局部堆积规则的概念示意，不是声称这几句话完整复述了 Powder Toy 或 Noita 的运动代码。

2D 网格很适合直接展示粒子落下和堆积。推广到 3D 时，还要面对网格方向偏好、邻域选择、碰撞以及表面呈现，不能只给 2D 规则增加一个坐标轴就认为视觉问题已经解决。

### Minecraft：重力下落与自然堆积不是同一层要求

官方《Block of the Week: Sand》说明沙块受重力影响，移除下方支撑后会落入空缺。[S3]

它说明“可以把静止材料变成下落材料”是玩家容易理解的行为。**但单凭垂直落下不能推出会有颗粒级侧向滚落、散布和安息角**；因此这种粗块重力参照不足以单独满足本题想要的细腻撒土表达。

证据获取说明：该官网正文抓取失败，重力事实来自搜索返回的官方页面摘录；这里不对其内部实现作判断。

### From Dust：最有启发的 3D 交互参照

开发者 Éric Chahi 的访谈说明：设计转向直接与自然互动，玩家取放水、沙和熔岩；团队为材料量选择了球形表示。另一篇访谈也确认，是模拟的可塑性推动了直接地形交互。[S4][S13]

更重要的是主程序员 Ronan Bel 的原话：模拟不是严格真实的，他们努力使其“不不真实”（“pas irréaliste”），并且是在为 gameplay 做模拟。团队调整流速、黏度、侵蚀和沉积阈值，也调整不自然的几何形态。[S4]

启发：交互不必追求每粒土的科学正确，但落点、物料量和地形后果必须让玩家觉得有因果联系；视觉调校可以是目标的一部分，不是物理失败后的补丁。

**不确定性**：本次没有读到其 SIGGRAPH 2011 技术报告全文，所以不把 From Dust 断言成某种具体 CA、DEM 或 MPM 算法，也不声称它的内部世界表示可直接适配我们的体素世界。[S14]

## 3. 工业界：运动中的土与稳定地形可以不同表示

### AGX Terrain：直接对应本题的混合表示实例

Algoryx 的官方文档明确描述：[S5]

- 地形数据在 3D 网格中存储质量、压实度和土类，地表以 2D 高度场表示。
- 开挖把稳定土转为具有六自由度的动态土颗粒。
- 动态土在地表接近稳定状态时并回地形。
- 提供 angle of repose（安息角）效果，以及受摩擦、压实影响的滑落行为。
- 可取得内部颗粒系统，用粒子发射器产生土颗粒。

因此，“先作为活动物料运动，稳定后成为地形”有真实工业实例。不是只有“全粒子”与“纯地形笔刷”两个选项。

这里可借鉴的是**状态转换与材料所有权**，而不是照搬产品：AGX 明确不支持高度方向不连续的稳定地形质量，只能在地表按列增减。洞穴、板下空间、悬空结构不在这种单层高度场的适用边界内。[S5]

对我们的设计推导：可以保留完整 3D 体素地形，只在活动区域考虑动态土和沉积规则；碰撞必须针对真实支撑表面。局部高度图可以是明确受限的辅助表示，不能偷偷成为整个世界的新权威。

### Houdini：看见粒子不等于有颗粒堆积

SideFX 文档直接区分传统 POP 粒子与 grains：普通粒子可用于沙尘；若要粒子相互堆叠、做沙地脚印或沙堡崩塌，就需要颗粒间约束。其 Grains 使用 PBD，通过位置约束解决穿透并允许堆积。[S6]

相关控制进一步说明了三个关键视觉因果：

- **摩擦**：阻止已经落地的材料无限滑动；官方区分静态和动态摩擦。[S6a]
- **结团**：Wet Sand 工具启用 clumping weights 和 `wetness`，而不只是改变颜色。[S6b]
- **休眠／唤醒**：稳定粒子可以休眠，附近粒子的运动可唤醒它们。[S6c]

Vellum grains 还要求足够的 substeps；官方配置教程建议手动建网络时至少 5 个子步，避免不稳定。[S6d] 这说明颗粒求解有稳定性要求，不能把一个粒子特效直接等同于可堆积土系统；这不是对我们项目的性能测量。

## 4. 学术界确实有关，但先借理解，不必先搬求解器

### Animating Sand as a Fluid，Zhu & Bridson，SIGGRAPH 2005

论文摘要明确采用连续介质近似，避免逐粒表示；在水模拟基础上加入颗粒间和边界摩擦。材料以粒子云携带，通过辅助网格施加边界条件与不可压缩约束。[S7]

它回答的是：大规模砂土可以按整体材料模拟，“计算粒子”不必对应每一粒真实沙子。并不是只把液体染成土色。

### Drucker–Prager Elastoplasticity for Sand Animation，Klár 等，SIGGRAPH 2016

论文使用 MPM 和弹塑性连续介质模型；Drucker–Prager 屈服条件表示剪应力与法向应力之间的摩擦关系。当支撑静态堆形所需的应力超过条件时，材料开始塑性流动。[S8]

其 related work 也明确列出高度场、CA、PBD、逐粒刚体等不同层级的方法。不存在只有完整 MPM 才能表现撒土的结论。

论文中的 material points 是连续材料的离散载体，不能理解为每个点都必须是一颗真实沙粒。MPM 内部的粒子／网格传输，也不同于 AGX 的“动态颗粒并回稳定地形”，两种“混合”不要混为一谈。

### Disney Matterhorn：生产应用

Disney 官方说明 Matterhorn 以 MPM 为核心，原用于《Frozen》的雪，后来增加本构模型以模拟泥、泡沫和沙等材料。[S9]

结论：学术界直接研究此题，但它们更多解决材料行为的统一表达。当前是否需要这种复杂度，要由后续产品需求和实际验证决定，不能因为“影视也在用”就认定应该采用，也不能未经测量断言它一定无法实时运行。

## 5. 对 Re: Flora 现状的核对

以下是调查基线中的本地事实，不是对所有地形系统的泛论。

1. `src/app/core/input.rs:1363-1423` 的 `try_shovel_place` 以连续笔划生成 brush edit，受 `SHOVEL_DIG_INTERVAL` 控制，再调用 `apply_surface_terrain_placement`。`src/app/core/mod.rs:920` 的间隔为 80 ms。
2. `src/app/world_edits.rs:71-75` 的 `TerrainBrushEdit` 只有 start、end 和 radius，描述的是几何笔划，不是单位时间材料源。
3. `src/app/core/vegetation.rs:2866-2917` 把笔划编译为 capsule，执行 surface placement，然后发布可见地形。
4. `shader/slang/chunk_modify.slang:252-266` 的 placement predicate 是“空格，且六邻域有实体邻接”，不是重力方向上的颗粒支撑／滚落规则。`shader/slang/chunk_modify.slang:354-414` 在命中几何范围并通过条件后写 atlas。
5. rooftop 分支也走同一 capsule surface 修改；`src/app/core/rooftop_scene.rs:516-588` 加上工作区限制，shader 的模型地板支撑逻辑提供初始邻接。它仍不是独立的土颗粒发射和沉积过程。
6. 可见地形通过统一 publication 路径发布；项目文档要求受影响 chunks 一致可见，不以部分更新换取视觉吞吐。地形碰撞跟进工作可以滞后，但这意味着未来落土也不能无条件依赖可能过期的 collider。[L1]
7. 本项目已有 MLS-MPM 水模拟，但 `crates/re-flora-water/src/mls_mpm/density.rs:457-465` 的 fluid stress 是压力加黏性应力，不能仅通过更换颜色或粒子大小得到摩擦屈服的砂土本构。[S8]

因此现象在语义上是合理的：系统知道“在笔划范围内放置邻接实体”，并不知道“以一定流量释放土，然后让它自然落定”。放慢现有操作或随机丢弃一部分候选格，能改变速度／覆盖，但不会自动补上这些材料行为。

现有 terrain connectivity 可以处理编辑后脱离的结构；这不等于系统已经具备逐粒撒土和局部颗粒滑落。

## 6. 可讨论的实现层级，不是已经选定的方案

| 层级 | 表达能力 | 边界 |
| --- | --- | --- |
| 随机稀疏笔刷 | 增土更稀、更碎，保留直接造型 | 没有落土过程；反复使用后仍可能变成均匀覆盖 |
| 表面沉积＋局部松弛 | 分批小土团落在有效支撑处；有限滑落、堆积 | 可以不模拟完整飞行；若碰撞后果重要，需要更明确的物料路径 |
| 可见落土＋局部沉积／滑落 | 看见释放、下落、落地后短暂调整，最终留下可编辑的土 | 本次建议优先讨论的表达目标；并不限定必须用某种求解器 |
| 局部 PBD／DEM 颗粒 | 粒子相互接触堆叠，适合推土、扫土、碰撞飞散 | 需要颗粒接触、摩擦与活动／休眠管理；精度和性能要实测 |
| 摩擦弹塑性 MPM | 更统一地表达体积材料、复杂变形和耦合 | 若要湿土、压实、土团破裂等深层玩法，才更有充分动机 |

这里的层级不构成经测量的成本排名。PBD、DEM、CA、MPM 的边界也可能组合使用。

### 一个值得讨论的生命周期

```text
背包／材料源
  → 以体积或质量每秒释放少量土
  → 动态阶段：下落、接触、有限滚落或结团
  → 稳定阶段：提交真实地形改变
  → 正常地形编辑、种植和保存
```

这是我们根据资料提出的设计候选，不是 AGX 或某款游戏的原样实现。

它不意味着每个可见粒子必定写一个 voxel：表现颗粒、物料载体、稳定体素可以有不同分辨率。如果单粒小于 voxel，必须有未沉积量／余量的明确归属，不能落地后凭空消失，也不能每粒都膨胀成一个大格。

若细粒在落地后仍长期可见，要明确它属于可继续运动的土、持久细节还是表现层；不能用一串短寿命粒子掩盖完全不相符的最终形状。

## 7. 泥土不等于棕色沙子

可选的视觉目标至少有两种：

- **细土／干沙式**：颗粒更小，滑落更明显，形成比较清晰的堆坡。
- **松散园艺土式**：细粒夹杂小土团，土团大小不同，有少量结团和有限滚动。

更湿、更黏的土还会带来保持团块、塑性和压实等需求，但这些不是用户说“泥土”就自动要求的功能。[S5][S6b][S9]

初步更适合 Restorative Garden 的候选是第二种：缓慢、少量、能看见材料留下来的撒土动作。它应该是一种有即时、清晰反馈的 Garden Moment，而不是为了展示大量粒子而制造的技术演示。[L2] 这仍需要用户确认，不能用项目定位代替用户的视觉偏好。

## 8. 下一轮讨论与未来可视验收

先问一个最高杠杆问题：**更接近“细土薄薄撒开”，还是“带小土块、逐渐堆出厚度”？**

随后再确认：

1. 土是否需要从空中明显落下，还是近地面出现／落定就够？
2. 同一点按住不动，是形成土堆，还是主要做零散覆盖？
3. 土到板边是落下去、沿表面滑动，还是被限制在目标区域？
4. 土稳定后是否需要被扫开、脚步扰动或再次铲动？这是决定完整颗粒物理必要性的重要分界。
5. 新动作作为独立工具／模式，还是改变现有加土操作？初步偏向保留精确笔刷，增加材料动作，但尚未决定。

若以后做可视候选，最有信息量的检查不是先看粒子数，而是看：

- 光板上短按：出现少量颗粒／小土团，而不是完整印章。
- 同一点持续释放：量逐渐增加，停止释放后能稳定，不无限振动或继续流水般铺平。
- 移动出料位置：留下可辨认的路径，但没有硬圆边／硬胶囊边。
- 斜面和板边：结果尊重重力、摩擦和真实支撑；不能附着在未定义的空中。
- 后续编辑／种植／保存：留下来的土有真实、一致的世界状态。

未来若用户明确要求 A/B，必须按项目规则提供运行时 Debug checkbox，保留原始模式。视觉确认与 Release 性能验收分阶段进行；本文没有实施实验，也没有做任何性能测试。[L3]

## 9. 安全与一致性边界

这些是候选方案必须解决的问题，不是当前系统已出现的新 bug：

- 活动土和稳定地形不能同时拥有同一份物料；转换失败应保留原有材料状态。
- 出料量应随时间定义，不随渲染帧率变化；暂停、停止输入、库存耗尽均有明确处理。
- 出世界边界、目标不支持沉积、保存／加载时尚未落定，需要明确语义。
- 地形新增后，落土碰撞必须感知新表面；异步 collider 的旧版本不能导致土穿过新堆。
- 初次沉积不能永久锁死所有松散土；反过来也不能把原有固定地形都变成会滑塌的土。
- 需要滑落的区域可以超过出料 footprint，但要有明确活动边界与跨 chunk 一致性。
- 一个地形变化批次应保持完整 Visible Terrain Publication；不能给每粒土单独发布导致邻块处在不一致的可见状态。[L1]

没有跑性能基准，因此本文不提供粒子预算、帧时承诺或算法性能排名。

## 参考资料与访问边界

### 游戏与开发者

- [S1] [Noita 官方介绍与 FAQ](https://www.noitagame.com/)。已读官方正文，包含“sand will pile up”及材料模拟说明。
- [S2] [The Powder Toy：SAND.cpp](https://github.com/The-Powder-Toy/The-Powder-Toy/blob/master/src/simulation/elements/SAND.cpp)。已读 raw 源码；master 会变化。
- [S3] [Minecraft 官方：Block of the Week — Sand](https://www.minecraft.net/en-us/article/block-week-sand)。官方搜索摘录支持重力事实；本次正文直接访问失败。
- [S4] [From Dust 团队访谈：Éric Chahi、Ronan Bel、Bruno Gentile](https://pierregaultier.wordpress.com/2015/08/09/entretien-eric-chahi-from-dust/)。已读正文；原访谈发表于 MCD 64，2011 年 9 月。依据是开发者第一人称回答，不是采访者猜测的算法。
- [S12] [GDC Vault：Exploring the Tech and Design of 'Noita'](https://www.gdcvault.com/play/1025695/Exploring-the-Tech-and-Design)。演讲入口已检索，本次未取得完整演讲内容，仅供后续观看。
- [S13] [Game Developer：The Core Of From Dust](https://www.gamedeveloper.com/design/the-core-of-i-from-dust-i-)。已读 Éric Chahi 访谈正文。
- [S14] Ronan Bel、Benoît Vimont，[Developing the interactive dynamic natural world of “From Dust”](https://doi.org/10.1145/2037826.2037855)，SIGGRAPH 2011 Talks。仅确认书目信息；ACM 全文访问返回 403，未用于推断具体求解器。

### 工业工具官方文档

- [S5] [Algoryx：AGX Terrain User Manual](https://www.algoryx.se/documentation/complete/agx/tags/latest/doc/UserManual/source/agxTerrain.html)，本次返回版本 2.42.3.0。已核对概述、地形表示、土粒合并、Angle of repose、粒子发射器和高度场限制。`latest` URL 后续可能变化。
- [S6] [SideFX：About Grains](https://www.sidefx.com/docs/houdini/grains/about.html)。已读 PBD、普通 POP 与可堆叠 grains 的区别。
- [S6a] [SideFX：Varying friction](https://www.sidefx.com/docs/houdini/vellum/varyingfriction.html)。已读静态／动态摩擦及碰撞双方属性组合。
- [S6b] [SideFX：Wet Sand](https://www.sidefx.com/docs/houdini/shelf/wetsand.html)。已读 clumping、wetness 与分辨率影响说明。
- [S6c] [SideFX：Auto-Sleep Particles](https://www.sidefx.com/docs/houdini/shelf/autosleep.html)。已读按周围运动休眠／唤醒说明。
- [S6d] [SideFX：Vellum configure grains](https://www.sidefx.com/docs/houdini/vellum/configuregrains.html)。已读 grains 的子步和稳定性要求。

### 学术与生产应用

- [S7] Yongning Zhu、Robert Bridson，[Animating Sand as a Fluid](https://www.cs.ubc.ca/~rbridson/docs/zhu-siggraph05-sandfluid.pdf)，SIGGRAPH 2005。已下载作者提供的 PDF，核对摘要、Introduction、Related Work 与 Frictional Plasticity（前两页）。
- [S8] Gergely Klár 等，[Drucker–Prager Elastoplasticity for Sand Animation](https://www.cs.ucr.edu/~craigs/papers/2016-sand/paper.pdf)，SIGGRAPH 2016，DOI `10.1145/2897824.2925906`。已下载作者提供的 PDF，核对摘要、Related Work、Background 与 Overview（前三页）；未对全文做复现或性能验证。
- [S9] [Walt Disney Animation Studios：Matterhorn](https://www.disneyanimation.com/technology/matterhorn/)。已读官方技术介绍。

### 本项目

- [L1] [Terrain Visual Rebuild Pipeline](../terrain_visual_rebuild_pipeline.md)；结合当前 `src/app/core/visible_terrain.rs` 及 placement 调用核对。
- [L2] [CONTEXT.md](../../CONTEXT.md)：Restorative Garden、Garden Moment 与 Free-form Planting。
- [L3] [AGENTS.md](../../AGENTS.md)：Visual Iteration Priorities 与 Release 模式验证规则。
