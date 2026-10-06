# 树模型覆盖能力与轴线模型调研

## 结论

“现有模型完全无法生成一根主干持续向上、沿途不断分枝的树”**不完全符合事实**：已有 `continue_main_axis` 参数和 Debug 控件，打开后分枝节点会同时生成续段与侧枝，已有单元测试覆盖这个行为。但默认树和当前保存配置都关闭它。[C1][C2][C3]

更准确的问题是：**现有模型能表达这种局部拓扑，却不能独立、稳定地控制完整主干与各级侧枝的形态。** 核心限制是同一个 `level` 同时承担从根走过多少段、分枝级次、长度/半径衰减、叶簇选择和年龄显露顺序；增加主干上的分枝站点，也会改变这些其他语义。[C1][C2][C4]

建议建立 **Axis（完整生长轴）＋沿轴位置＋branch order（分枝级次）** 的参数化模型，借鉴 Weber–Penn 的按级参数和树冠形状控制，但不必照搬全部参数或立即实现完整植物生理模拟。[W1] 此建议是项目设计判断，不是已经实现或已通过视觉验证的功能。

## 1. 当前实现的实际行为

### 1.1 默认并不是持续主干

`default_tree_branching_desc()` 设置 `continue_main_axis = false`，`config/gui.toml` 也保存为 false。递归节点决定分枝时，通常只产生侧枝，原方向的轴线停止；只有不分枝、侧枝数为零等路径才产生续段。[C1][C2][C3]

Debug 中现有 `Continue Main Axis` 复选框打开后，分枝时先产生沿原轴方向的段，再产生侧枝。续段递归后仍可继续分枝，因此沿主干多个位置发出侧枝在拓扑上可行。可以先用该开关做现状比较，**不需要新增一个开关才能证明基本能力**。[C1][C3]

这只是局部续轴行为，不能将 `BranchSegmentRole::MainAxis` 理解成“整棵树的唯一主干”：侧枝自身的续段也会标成 MainAxis；该枚举没有全局 Axis 身份。[C1]

### 1.2 真正的模型耦合

| 现有机制 | 对持续主干树形的限制 | 代码依据 |
|---|---|---|
| 任何续段和侧枝都递归到 `level + 1` | 沿同一根主干多走一段也消耗一次递归预算，主干站点数与侧枝深度不能独立控制 | `grow_segment` [C1] |
| 同一节点的续段与侧枝都使用同一个 `length`，之后都乘 `length_dropoff` | 同层短侧枝与长主干续段无法独立设置；主干节间缩短和侧枝长度衰减耦合 | `recurse` / `grow_segment` [C1] |
| 半径只依赖 `segment.level` | 同一节点的续段与侧枝采用相同的起止半径，不能表达粗主干带较细侧枝 | `thickness_at_level` / `Tree::build` [C2] |
| 分枝窗口用 `level/(iterations-1)` | 名称虽是 fraction，实际是全局递归深度比例，不是当前完整轴的弧长比例；侧枝也不是从自己的局部零位置重新开始窗口 | `branch_level_fraction` [C1] |
| 全局一套概率、数量、角度和趋向参数 | 主干、一级枝、二级枝不能各自采用不同规则；启用续轴会作用于所有递归路径 | `BranchingDesc` [C1] |
| 叶簇锚点来自一个选定的 `level` 的端点 | 不是按末级枝/芽/局部位置布叶；不同高度发出的同级侧枝不能用同一分枝级次来控制布叶 | `leaf_level` 与 `leaf_anchors` [C2] |
| 年龄按 `visible_branch_levels` 裁切，风响应也按 `level` 设置 | 几何离散方式会影响显露顺序与风参数，不能只换生成器而忽略消费者 | [C2][C4] |

所以你的观察在“**难以覆盖更多可控、可辨识的树形**”这个意义上是成立的；不能把它简单归结成渲染器不支持长主干，也不能只把默认开关改成 true 当成根治。

### 1.3 仅打开续轴的潜在代价

续段与所有侧枝都会执行同一套递归分枝规则。若每个节点总产生 m 个孩子，段数随深度呈几何增长；打开续轴会增加每个分枝节点的孩子数。当前概率和分枝窗口会影响实际数量，不能用最坏情况替代真实测量。[C1]

这不是本次测得的性能回归；本次没有性能基准，也没有生成新的视觉候选。

## 2. 外部模型给出的启发

### 2.1 Weber–Penn：按完整枝轴及分枝级次参数化

PyHelios 的第一方 Weber–Penn 实现文档明确区分 trunk（0 级）、后续子枝级次，并允许按级提供不同参数；其 `0Length` 控制完整主干长度，`BaseSize` 控制无枝基部占树高比例，`0Taper` 控制沿主干的锥度，`Shape` 控制圆锥、椭球、圆柱等树冠包络。[W1]

值得借鉴的不是“再增加一堆随机滑块”，而是：

- 主干是一条有全长、局部参数和多个附着位置的轴。
- 一级侧枝是该轴的孩子，但其长度和粗细不是主干续段的副产品。
- 同一条轴的几何采样段数不等于分枝级次。
- 枝长可随附着高度变化，从而形成可辨识的树冠轮廓。

文档中的级数限制属于这个实现，不能当成所有 Weber–Penn 模型的通用限制。本文借鉴其建模分工，不声称完整复刻 1995 年论文公式。[W1]

### 2.2 L-system：缺的是语义，不是表达能力

Algorithmic Botany 的作者资料展示了“主轴继续、侧轴生成”和“主轴终止、侧轴接替”等不同顶端活动模式，可以用 L-system 表达。[W2] 该页面讨论的是草本花序，不能直接推出某个木本树种的全部建筑特征。

项目 GUI 虽然叫 `L-System / Branching`，代码目前是固定规则的递归生成器，没有可配置产生式解释器。[C1][C3] 因而不应把当前能力限制误认为 L-system 本身的限制；也不必为了持续主干先实现通用 DSL。

### 2.3 Space colonization：更适合另一层问题

Runions 等作者的论文摘要指出，三维扩展的空间殖民模型可以生成真实感树结构，其参数面向可见的树形特征。[W3]

设计判断：它值得以后用于空间占据、树冠填充及不规则环境响应，但本次最直接的需求是**保留可控主干、区分枝级和沿轴分枝**。优先引入空间驱动的整树生成，比轴线参数化更难明确保住这些作者意图。不建议为当前问题先替换成纯空间殖民模型。此次只核实了作者摘要，未完整解析论文 PDF，故不据此声称具体算法或性能性质。

## 3. 推荐模型：完整轴＋附着站点＋分枝级次

以下为建议设计，不是现有 API。

### 3.1 三个必须分开的量

1. `branch_order`：主干为 0；其侧枝为 1；侧枝的侧枝为 2。同轴继续不增加 order。
2. `u`：当前轴上按弧长归一化的位置，0 为轴基部，1 为轴梢；分枝窗口、锥度、枝长分布使用它。
3. `segment_index / topological_depth`：几何离散及父子求解顺序；服务渲染和姿态计算，不定义生物分枝级次。

不要直接把现有 `level` 改名为 `branch_order`：两者在同轴续段上语义不同，会影响长度、布叶、年龄与风响应。[C1][C2][C4]

### 3.2 建议职责边界

```text
TreeArchitectureDesc
  trunk profile                  # 完整轴长、基部半径、锥度、曲率
  profiles by branch order       # 各级侧枝的长度/半径/方向规则
  crown envelope                 # 沿高度允许的冠幅变化
  seed + explicit limits

Axis
  stable axis id
  parent axis + attachment u
  branch order
  centerline + radius profile
  lateral attachment sites

compiled BranchSkeleton
  parent-before-child segments
  explicit segment endpoint radii
  axis identity + local u + order metadata
  foliage attachments
```

模型先确定完整主干，再在轴上的站点产生侧枝，最后对各级侧枝递归。一个分枝站点处，“续轴”是同一 Axis 的下一段，“侧枝”才是新 Axis。

完整轴需表达两类主要行为：

- **持续领轴**：原轴继续，沿途发出侧枝，优先满足本次需求。
- **终止/分叉/接替**：某处原轴结束，产生多个新轴，或一个侧轴接替领轴，用于扩展其他树形。

这些是生成规则，不意味着所有树种都能由两种按钮自动准确复现。

### 3.3 初期真正需要的控制

- 主干：全长、基部半径、沿轴锥度、曲率/趋向。
- 分枝分布：当前轴的起止 `u`、站点数量或间距、每站侧枝数、绕轴旋转规则（交替/成对/轮生/螺旋作为后续可选配置）。
- 每级侧枝：相对长度、基部半径比例、夹角、曲率、可继续分枝的最大级次。
- 树冠：简单沿高宽度曲线，用于调制侧枝长度；先支持圆锥/椭球轮廓，不必一开始实现复杂包络求交。
- 叶簇：按末级轴、芽或站点的明确规则附着，仍保持独立叶实例和所属段绑定。

沿轴半径函数与侧枝半径比例必须分开。可先使用可控 taper，之后再研究后代负载驱动粗细；不要同时乘几套衰减导致侧枝无意中过细。树冠曲线若仅用于调制长度，不应宣称硬性约束所有弯曲枝都在包络内部。

### 3.4 适配当前渲染和风系统

现有 `BranchSegment.parent` 已明确拓扑，并要求父段先于子段；`TreePose::new` 额外要求子段起点在父段终点。木质体输出为 RoundCone，叶实例有所属段，蒙皮也依赖所属段及 parent。[C1][C2][C4][C5]

因此建议保留“编译成段骨架”的下游接口，而不是替换渲染路径：

- 在每个侧枝附着站点切分父轴，确保站点成为父段端点；不能直接把侧枝挂在某段中点，否则违反当前姿态约束。
- 父段先输出，后输出其续段和侧枝；维持可拓扑求解顺序。
- 每段显式携带起止半径，木质体构建不再按全局递归 `level` 推导。
- 分枝方位沿 Axis 保留连续局部框架，避免采样段数改变或近竖直方向导致绕轴相位不稳定。
- 叶与果的附着沿用可靠的段 ownership；切分改变后，绑定与发布一并重建。
- 风响应转向半径、长度和明确的柔顺性配置，几何采样密度变化不能无意改变风格。
- 年龄显露需采用附着因果关系与轴上进度：子枝可见时，父轴至少已长到该附着站点。保持成熟骨架确定性，不在每个年龄随机重抽。

稳定身份也需有边界：同一作者描述下的年龄变化应保持 Axis/段身份；站点或采样方式变化可作为新 generation 重建。不要承诺任意拓扑编辑后数组索引仍然不变。

## 4. 建议实施与验收顺序

1. **现状比较**：使用已有 `Continue Main Axis`，验证当前模型能产生续干，也观察主侧枝粗细和长度耦合。只比较，不把调参称作新模型。
2. **最小候选**：只实现持续主干＋独立侧枝规则；生成原有段骨架与 RoundCone，暂不引入光照生长、空间竞争或生理资源模拟。
3. **视觉覆盖**：做持续主干圆锥冠、持续主干椭球冠、分叉阔冠三类作者预设。用户评价可辨识度，不用树种标签替代真实视觉验收。
4. **补齐拓扑模式与年龄/风语义**：先保证正确附着和确定性，再扩充更复杂架构。
5. **独立性能阶段**：记录 Release 模式段数、叶实例数、生成/发布时间和实际帧性能；视觉候选可先展示，不能提前声称性能达标。

若实施为 A/B 实验，Debug 必须有运行时“旧模型（未勾选）/轴线模型（勾选）”切换；已有续轴开关不等于新模型的 A/B。新设置按项目要求持久化并按调整关注点组织。[C6]

最小正确性测试应包括：

- 主干多个站点发枝后仍延伸到目标长度。
- 同一主干轴全部保持 order 0，侧枝 order 1，与几何段数无关。
- 所有孩子附着于父段端点，且 parent index 小于 child index。
- 主干续段半径连续，侧枝基部半径可独立设置。
- 改变中心线采样密度不改变分枝站点和作者级次；允许几何逼近误差，不允许拓扑意外变化。
- 固定 seed 与配置确定性生成；年龄不重抽成熟拓扑，年轻侧枝不悬空。
- 最大轴数/段数/分枝级次有显式预算，避免失控递归。

## 5. 本次验证边界

- 仓库分析基于 `adde77ac`；只新增本调研文档，不改生成器、配置或 shader。
- `cargo test --bin re-flora branch_skeleton::tests -- --nocapture`：8 个现有测试通过，包括续轴与父子拓扑测试。
- `cargo test --bin re-flora tree_gen::tree::tests -- --nocapture`：13 个现有树生成测试通过。
- 最初单独测试命令搭配不完整测试名及 `--exact`，实际运行 0 个测试；不将该次执行作为能力证明。之后的测试执行才是有效验证。
- `cargo fmt --check` 与当前工作树的 `cargo build --release` 通过，Release 构建耗时 2m 35s；没有生成文件改动。
- 本次仅文档改动，未进行 GPU 隐藏运行，也未自动打开可见游戏；没有新候选截图、实际树种视觉确认或性能结论。

## 来源

### 仓库第一方依据

- [C1] [`src/branch_skeleton.rs`](../../src/branch_skeleton.rs)：`BranchingDesc`、`BranchSegmentRole`、`recurse`、`grow_segment`、`branch_level_fraction` 及 `continuing_main_axis_keeps_parent_after_lateral_branching`。
- [C2] [`src/tree_gen/tree.rs`](../../src/tree_gen/tree.rs)：默认分枝描述、`thickness_at_level`、`Tree::build`、`visible_branch_levels` 与叶附着。
- [C3] [`src/app/tree_gui/branching.rs`](../../src/app/tree_gui/branching.rs) 及 [`config/gui.toml`](../../config/gui.toml)：已有 GUI 开关及保存配置。
- [C4] [`src/tree_gen/pose.rs`](../../src/tree_gen/pose.rs)：`TreePose::new` 的父子端点约束、风参数与 GPU 拓扑深度。
- [C5] [`src/tree_gen/skin.rs`](../../src/tree_gen/skin.rs)：段 ownership、parent 与蒙皮绑定。
- [C6] [`AGENTS.md`](../../AGENTS.md)：视觉 A/B、配置保存、验证及性能阶段规则。

### 外部第一方资料

- [W1] [PyHelios Weber–Penn 插件文档](https://plantsimulationlab.github.io/PyHelios/WeberPennTreeDoc.html)，该实现维护方对 `Levels`、`0Length`、`BaseSize`、`0Taper`、`Shape` 与按级参数的说明。原论文：Jason Weber & Joseph Penn, *Creation and Rendering of Realistic Trees*, SIGGRAPH 1995，[DOI](https://doi.org/10.1145/218380.218427)。本次参数细节证据来自实现文档，不把未读取原文的公式当作已核实。
- [W2] [Algorithmic Botany / Visual Models of Morphogenesis, Section 9](https://algorithmicbotany.org/vmm/section09.html)，作者对草本花序不同顶端活动和分枝模式的解释。
- [W3] [Runions, Lane & Prusinkiewicz, *Modeling Trees with a Space Colonization Algorithm*, 2007](https://www.algorithmicbotany.org/papers/colonization.egwnp2007.html)，作者托管的摘要与论文下载页。本次 PDF 未取得可读全文，只使用摘要确认的范围。
