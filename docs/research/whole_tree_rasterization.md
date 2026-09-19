# 整树光栅化与风动：可行性调研

日期：2026-09-16。代码基线：`c9b9d0f99dfb879105e94138c247b703ef90b40f`，当前 `agent/butterfly-block-flight` 工作树；不代表其他 checkout。仅阅读源码与一手资料，未改实现、未运行游戏或性能测试。

## 结论

整树光栅化是合理的长期方向。建议统一树的拓扑、附着关系和动态姿态，再由它产生主视图几何、阴影几何及必要的碰撞/场景查询代理。主要难度不是画出会摇摆的网格，而是把树从 terrain 移出后，保持它对光照、场景查询和交互的贡献。

不能把“顶点变形便宜”推成“完整动态树成本很低”。相较每帧重写体素及重建派生结构，骨架变形具有明确的结构优势；相较当前完全不动的树，它会增加动画、动态阴影和查询更新成本。未测量前不承诺整体提速。

## 当前代码证据

| 事实 | 源码位置 | 对方案的影响 |
| --- | --- | --- |
| 树已有程序化骨架，最后生成 RoundCone 枝干与叶片位置；Tree 的输出没有保留完整父子绑定 | `src/tree_gen/tree.rs:142–261`；`src/branch_skeleton.rs:73–96` | 可复用生成算法，但要保留稳定 segment/parent 身份、局部静止姿态与附着权重。当前 BranchSegment 只有 start/end/level/role，层级数不能代替父节点身份 |
| 已有四边截面的树干网格预览 | `src/tracer/tree_preview_mesh.rs:11–100` | 从枝干描述生成三角形已有局部基础；蓝图预览不是生产树材质、连续接头或动画实现 |
| 树发布包含 terrain 枝干、叶片、果实、阴影、声音及规范记录 | `docs/tree_publication_architecture.md`；`src/app/core/vegetation.rs:1007–1068,1105,1901` | 迁移要修改现有事务的派生结果，不能绕过删除、替换、年龄变化、回滚与存档恢复 |
| 动态果实先光栅化阴影深度，再复制并与 terrain 阴影取 min | `src/tracer/mod.rs:3475–3500`；`shader/slang/dynamic_fruit_shadow.vert.slang`；`shader/slang/shadow_depth_copy.slang`；`shader/slang/tracer_shadow.slang:123` | “光栅物体不能投影到地形”不是当前代码的准确描述；可扩展已有不透明投影机制 |
| 树叶已有风动阴影代理与历史融合 | `shader/slang/leaves_shadow.vert.slang`；`src/tracer/leaf_shadow_proxy.rs`；`src/tracer/mod.rs:3432–3474` | 已经是混合树表示。代理阴影并不等于逐叶精确阴影，枝干新姿态必须进入同一投影时刻 |
| 地形用太阳 cosine，果实/植被的通用 stylized 权重有 0.7 下限；两者环境光查询也不完全同构 | `shader/slang/tracer.slang:217–240,598–624`；`shader/slang/flora_shadow.slang:86–94,145–163`；`shader/slang/dynamic_fruit.vert.slang:58–93` | 不能直接把木头套现有果实着色器；应统一木质与 terrain 的材质/光照规则，叶片保留合理的独立光学模型 |
| DDGI 探针追踪及命中后的太阳可见性查询走 voxel 场景 | `shader/slang/ddgi_probe_trace.slang:98–134,250–282`；`shader/slang/scene_marching.slang` | 从 terrain 删除树干而仅增加 raster draw，会让这些射线失去树干，影响遮挡、反弹和距离统计；共享 DDGI 采样不足以解决 |
| 局部灯遮挡也调用 voxel 查询；玻璃明确注明 raster-only 物体不在 voxel SceneQuery 内 | `shader/slang/local_lighting.slang`；`shader/slang/tree_leaf_lighting_cache.comp.slang`；`shader/slang/scene_query.slang:692`；`shader/slang/glass_resolve.slang:545` | 需审计局部灯、反射/折射及屏幕外可见性；现有屏幕空间补偿不能作为完整世界查询替代 |

这些是静态源码证据，不证明当前每个开关组合的运行效果均正确。

## 光影会不会明显不一致？

光栅化与体素射线遍历主要决定“找到哪个表面”，不必决定不同的着色风格。只要得到兼容的世界位置、法线、材质信息，并使用一致的光照约定，就可以融合。当前风险具体分成：

1. **表面着色：**统一木头的线性颜色、调色板、太阳角度响应、环境光单位与曝光链路。共享光照函数，不等于把 terrain 专用的体素中心偏移生搬到网格。
2. **法线与采样：**树变形必须同时变换法线；按顶点、体素中心、逐像素取光可能产生不同梯度。普通圆滑圆柱即便光照一致，也会与方块轮廓不同。
3. **双向直接阴影：**树投到地形、地形投到树，以及树自阴影都要测。太阳贴图可复用，偏移、薄枝分辨率、遮挡者裁剪、动态历史拖影是渲染工程问题，不能只算业务接线。
4. **间接光：**树既要接收 DDGI，也要被 DDGI 看见。静止姿态的隐形树干留在 terrain 会产生脱离可见树的遮挡，不能当完整实现。
5. **其他可见性：**主画面深度融合、玻璃中的树和局部灯阴影都需要覆盖。仅主视图与太阳投影成功，不代表整个渲染完成。

这些差异不是都叫 aliasing：阴影锯齿/闪烁是采样问题，接口漏缝是几何连续性问题，两侧亮度跳变是着色或可见性不一致；应分别验证。

## 推荐的树表示

一个权威树定义保留 seed、年龄、拓扑和枝叶果实绑定；每个模拟时刻发布一份可消费的姿态。颜色 pass 与阴影 pass 读取同一份姿态。拓扑变化重新生成派生资源，普通风动只更新姿态及需要跟随的代理。

- 根部固定、沿树干逐渐增大柔顺性，主枝继承父枝变换，细枝与叶片叠加自己的局部响应。不能给所有顶点独立位移，否则会伸长、开裂或脱落。
- 接头共用边界或连续蒙皮，叶片与挂果跟随变形后的附着点；果实脱离时再交给自由物理。
- 复用已有风场和机械响应数学，但现有叶片位移节点不能直接充当整树层级骨架。
- 若要保持现有体素美术，可以先考虑静止姿态的体素表面网格再绑定骨架；运动时不必重新体素化。但这会带来顶点数、合并面跨骨骼弯曲和方块变形的选择。另一候选是分段低面数枝干；其轮廓变化必须视觉验收，不能默认等价。
- 主视图网格、简化阴影/查询/碰撞几何可以不同，必须由同一拓扑与姿态生成。按用途保留派生数据是正常设计，仍需要明确版本、生命周期和误差范围。

不建议以“树干永远在静态 terrain、细枝独立摆动”作为本需求的终态。但它不是原则上坏设计：远景、弱风或粗干位移小于像素时可能是有效 LOD。根部固定也是物理边界条件，不妨碍整个树采用统一动态模型。统一模型不意味着每个位置都要同幅度运动。

## 场景查询的路线选择

| 路线 | 优点 | 代价 / 边界 |
| --- | --- | --- |
| 由骨架生成动态锥体/胶囊等简化查询代理，和 terrain 命中统一比较 | 可保留现有软件 voxel 遍历；避免每帧重写静态 terrain | 要有空间加速、最近命中/材质协议、姿态同步；代理轮廓与碰撞精度必须验证。是优先研究候选，不是已证实最优解 |
| 动态三角形查询结构 | 与可见几何更接近 | 当前 voxel 查询之外的新集成；软件 BVH 或硬件 RT 都有更新、遍历和平台成本 |
| 单独动态体素代理 | 与部分体素查询语义相近 | 仍要动态更新、处理精度与融合；不可默认它比骨架查询便宜 |
| 静态代理 / 纯屏幕空间 | 便于局部试验 | 强风错位与屏幕外缺失无法消除，只适合明确标注的近似实验 |

不需要为了整树光栅化立即把整个 terrain 改为硬件光追。也不能只补探针 primary hit：DDGI 命中后的太阳/局部灯可见性、距离统计与历史收敛都应使用兼容的场景表示。

## 难度与建议的验证顺序

相对判断，不提供无测量的人日或性能数字：生成可见网格为中等难度；连续枝干形变和附着绑定为中高；太阳阴影接入为中等、稳定薄枝效果为中高；GI/其他射线完整性及编辑碰撞迁移为高。

1. **静止整树与 terrain 融合。** 固定 seed、年龄、机位、时刻；比较树根接地、木色/法线、太阳光、纯环境光、局部灯和树自阴影。未来若做 A/B，按仓库约定提供 Debug 运行时 checkbox，原模式未勾选，候选勾选。
2. **单树风动与直接阴影。** 弱风、阵风、强风、停风回弹，检查接头、树根、附着物和阴影同相位；先给用户看实际效果。
3. **补齐场景与生命周期。** 证明探针与局部光查询看到动态树；检验玻璃、碰撞/拾取、删除、替换、生长、存档和恢复。碰撞可以有意简化，但要说明树干移动范围与实体代理误差。
4. **视觉认可后评估密林 release 性能。** 分别测动画、主视图、阴影、GI/查询结构更新与追踪、CPU 发布、内存及帧时间尾部。区分树数、枝段数、可见像素与强风；动画便宜也可能被叶冠覆盖率或阴影开销淹没。

第 1、2 步是候选效果验证，不是完整交付。保持运行正确性门槛，所有尚未完成的 GI/交互项目明确记录。此次未启动这些实现步骤。

## 外部一手来源

- [GPU Gems 3 Chapter 6 — GPU-Generated Procedural Wind Animations for Trees](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-6-gpu-generated-procedural-wind-animations-trees)：层级枝干变换、GPU 风场、位置与法线变换及实例化，支持整树 GPU 动画可行性；属于视觉近似，不保证强风物理正确。
- [GPU Gems 3 Chapter 16 — Vegetation Procedural Animation and Shading in Crysis](https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-16-vegetation-procedural-animation-and-shading-crysis)：主弯曲与局部叶片细节分层，支持同一实体采用不同运动/材质尺度；简单高度弯曲不宜直接当复杂树强风方案。
- [GPU Gems 3 Chapter 4 — Next-Generation SpeedTree Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)：动态树阴影与地面/自阴影、叶片卡投影的困难；证明可投影，不证明本项目现有路径已正确。
- [Scaling Probe-Based Real-Time Dynamic Global Illumination for Production](https://arxiv.org/abs/2009.10796)：探针追踪、距离信息与历史更新。结合本地源码得出的工程结论是：接收 GI 与参与 GI 必须分开验收。
- [Creating Optimal Meshes for Ray Tracing](https://developer.nvidia.com/blog/creating-optimal-meshes-for-ray-tracing/)：形变加速结构的更新/重建取舍及细长三角形、透明几何成本。仅用于候选三角形查询路线，不能作为当前软件体素路径的性能数字。
