# 随机 low-poly 花园石材：一手审计与原生实验

## 目标与证据界限

权威生成物是 **Stone Model**（常规 indexed triangle solid），不是 voxel 集合。**Paving Slab** 是上下平整、可作铺地/踩踏实体的切割石板；**Landscape Rock** 是轮廓不对称、适度分面的景观岩石。本实验不实现铺设玩法、背包或导航；预览不修改花园地形。视觉候选不等于用户认可，也不等于性能通过。

本次不派研究 agent（Worker 的授权明确禁止再委派）。以下是实际读过的作者/官方文档及源文件，不把搜索摘要或二手教程当作算法证据。访问于 2026-10-06；浮动 URL 只描述本次读取版本，后续实现没有复制其源码。

## 一手资料审计

| 来源（实际审计 URL） | 实际看到的实现/契约 | 许可与限制 |
|---|---|---|
| Blender 官方 [`rockgen.py`](https://raw.githubusercontent.com/blender/blender-addons/main/add_mesh_extra_objects/add_mesh_rocks/rockgen.py)、[`__init__.py`](https://raw.githubusercontent.com/blender/blender-addons/main/add_mesh_extra_objects/add_mesh_rocks/__init__.py) | 随机选择 12 种预制多面体，按轴随机缩放/顶点参数；`generateRocks` 添加两层 Subsurf、四层 Displace，可选 Smooth；固定 seed；不是保证任意输入都低面数/凸/不自交的方案。注册文件署名 Paul “BrikBot” Marshall。 | 源头 SPDX **GPL-2.0-or-later**。只研究方法，不复制/移植代码；不能把 GPL 源码静默并入不同许可项目。生成内容与程序许可不能混为一谈。依赖 Blender 的 modifier/texture 实现。 |
| 作者 Denton Woods [`README`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/README.md)、[`generate.py`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/rockgen/generate.py)、[`clip.py`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/rockgen/clip.py)、[`LICENSE`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/LICENSE) | icosphere 或 jittered point-cloud convex hull → 比例 → 法线方向噪声 → 平滑/侵蚀 → 真平面切割并封口 → 基底切平 → normals/颜色。审计到 clip 的边交点、环、封口与 winding 处理；作者明确声明激进位移可自交，封闭不能替代自交检查。 | **MIT**, copyright 2026 Denton Woods；复制实质代码须保留通知。作者披露 AI 协作来源；不将它视为工业验证。这里只采纳“真切面而非仅噪声”的方法，不导入 numpy/scipy/生成器。其文档声称输出归使用者，未把该声称推广到其它工具。 |
| Three.js 官方 [`ConvexGeometry`](https://threejs.org/docs/pages/ConvexGeometry.html)、[`LICENSE`](https://raw.githubusercontent.com/mrdoob/three.js/dev/LICENSE) | 3D points → 包围它们的 convex hull；官方说明平均复杂度 O(n log n)，是 addon，不是基础引擎。适合自然轮廓，但铺地石板仍需独立平顶设计。 | **MIT**，复制/分发须保留 copyright/permission notice。本实现不引入 Three.js，也不复制 QuickHull；离线展示只用 Canvas。 |
| SideFX 官方 [`Clip SOP`](https://www.sidefx.com/docs/houdini/nodes/sop/clip.html) | 按平面裁剪 geometry，可选保留一侧/两侧并生成截面；可作为“切面”工作流的官方参照，不等于存在官方 Labs Rock Generator。 | 文档说明不是源代码许可证。Houdini 为专有产品，不能据此声称可直接移植节点实现。本项目不需要 Houdini 运行时。 |

检索到的 `acfaruk/proc-rock` raw README/LICENSE 为 404，没有取得可靠许可文本，因此**未选用、未复制，也未给它下许可结论**。Unity 社区凸包石块帖子只能是作者方法描述，没取得可审计代码/许可，因此不作为实现依赖。

## 方法比较与选型

- **随机点云凸包**：低面数、轮廓自然、凸实体适合 voxelization；需要稳健 hull/共面处理，无法靠压扁同时表达“切割平顶铺地石板”。
- **扰动球/多面体 + noise**：有圆润/凹凸和侵蚀感；大位移可能翻面、自交，顶面不适合平稳踩踏；需要控制 LOD 和 normals。
- **凸 blank + 半空间裁剪**（本次）：轮廓和真正的平面切口均由明确 planes 控制。每个 plane 保留内核，实体始终凸；封口和 fan triangulation 简单、依赖为零。代价是没有洞穴、层理凹槽或脱落碎片；这些不在本次真实 variation 范围内。

石板从 prism 出发，只变更 XZ 角切/窄 chamfer，主顶面与底面始终水平；厚度与 footprint 有独立约束。岩石采用 8–24 个有限非对称 support planes，法线适度倾斜，不叠加无界噪声。两类共享裁剪实现而不共享不合适的形状配方。

## 权威模型 interface

`src/stone_models.rs` 的 `generate(StoneSpec) -> Result<StoneMesh>` 是唯一 seed→geometry seam：

- SplitMix64 固定算法与 24-bit 抽样，不依赖库 RNG 改版；seed 为 u32。
- 右手系、+Y up；256 terrain voxels = 1 garden world unit；底部局部 pivot `(0,0,0)`。
- 顶点为共享 topological positions；u32 indices、每三角 outward flat normal 与 sRGB face color。渲染与 OBJ 才拆 shading corners，不影响实体闭合性。
- f64 归一化裁剪后转 f32；有限参数 clamp、非有限回默认，size 与 facet budget 都有界；Slab 高度≤短边的 30%。
- `validate` 检查正面积、有限属性/索引/包围盒、CCW/向外、所有点在面内、每条边恰两次且反向、Euler=2、正体积。凸性 + 定向封闭拓扑给出本方案的无自交实体约束，不宣称是任意外部网格的自交修复器。
- `obj()` 输出标准 v/vn/f triangles，导出的是模型，不是 voxel 冒充模型；OBJ 不携带本实验 face palette，geometry/flat normals 保留。

离线展示与 native 路径/验证记录在实现完成后追加于本文。全局 final pixel grid、原生 GUI、Contrast-aware 青边修复与 dithering 不属于本次修改范围。
