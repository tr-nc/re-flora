# 蝴蝶：任意镜头下的实时 3D 低像素管线

## 结论与现状

**要消除视角锁定，应保留运行时真实翼面几何，由游戏当前相机投影，再只对蝴蝶做低分辨率采样与合成；不是继续增加预渲图集方向，也不是把整个游戏像素化。** 去掉身体是已接受的美术约束，不以解剖完整性为理由加回来；侧视辨识度应靠翼形、曲率、色块和动作解决。

本笔记只研究，不修改资产或代码。已读[前次调研](butterfly_blender_pipeline_followup.md)、[v3 说明](../../experiments/butterfly-method-comparison/blender-v3/README.md)及[网页验证](../../experiments/butterfly-method-comparison/blender-v3/browser-validation.md)。v3 是原生 12/16/128px、五方向五相位的离线输出，另有 GLB；网页把图集 canvas 和可旋转的 model-viewer 并排展示。**网页原型不是游戏集成，也没有证明真实游戏中的低像素几何、遮挡或性能。** 研究起点的 v3 尚保留胸腹。集成时已有 [v4 纯翼面源](../../experiments/butterfly-method-comparison/blender-v4/README.md)和[任意视角双预览（旧编号页面现已迁移到统一工具）](../../experiments/model-preview/?model=butterfly)：身体网格已删除，两块画布共享同一相机/场景/动画，低像素区实际渲染到 N×N WebGL 缓冲，不请求图集。默认正交也支持透视，未加后期量化。这解决了网页中的自由视角演示，仍不代表下面建议的游戏遮挡与多实例方案已经实现。

## 常见路线：差异在运行时表示，不在是否用 Blender

这里的“常见”指可复现的技术类别，不代表有行业占比统计。

| 路线 | 实际输出及适用边界 | 与我们目标的关系 |
| --- | --- | --- |
| 3D → 离线精灵帧 | Dead Cells 美术 Thomas Vasseur 说明以 3DS Max 建模动画，低分辨率无抗锯齿渲染，逐帧导出 PNG 与法线图；这是运行时 2D，不是实时角色网格。[1] | 与当前 v3 图集同属预渲路线；法线图可改善光照，不能补出任意新视角的轮廓。 |
| 3D → 缩图减色 → 手绘清理 | Pedro Medeiros 的旋转招牌案例用 Blender 参考、Aseprite 缩图减色，再另层重画。[2] | 能精修指定视图，但不自动解决自由相机。 |
| 实时 3D → 低分辨率目标 → 最近邻放大 | 引擎提供独立视口/渲染目标及纹理采样；t3ssel8r 本人确认项目使用 “true 3D assets in-engine”，可混合与复用动画。[3–5] | 符合本次方向；采访没有公开完整渲染算法，不能把下述方案说成对其技术的复刻。 |

**缩放、减色、像素设计是三件事。** 最近邻只选择已有纹素，不会限制颜色数量；线性缩图还会混出新颜色。调色板量化才把颜色映射到有限集合；把 RGB 各通道取整也不等于映射到指定美术调色板。[5] 建议先用无身体的简化翼面、两三块主色、原生低分辨率无 AA 输出；若要求最终颜色严格受限，在约定的色彩空间、光照/色调映射之后量化。高分辨率再缩小可作对照，不宣称优于原生采样；8–16px 的剪影仍需人为设计。此段是设计建议，不是已测结果。

## 推荐的局部实时路线（工程建议）

`共享翼面网格 + 每虫姿态/拍翼相位/配色 → 当前游戏相机 → 蝴蝶专用低分辨率颜色、覆盖与深度 → 可选减色 → 最近邻合成到原场景`

- **只隔离蝴蝶层。** 可从一个与屏幕同宽高比的低分辨率透明目标起步，一次绘制可见蝴蝶。目标覆盖全屏不等于把全场景像素化：地形、植物、UI 保持原管线，空白区域不改背景。Godot 的 SubViewport/Camera3D 剔除层和 Three.js RenderTarget 是实现这类分层的官方 API 例子；本项目是自有 Vulkan 渲染器，不能直接套用这些组件。[4,6]
- **像素预算须明确。** 共享屏幕网格给所有虫一致的屏幕像素块，远处自然更小；不保证每只永远 12px。若坚持每虫独立固定 12/16px，可以探索动态 tile 图集，但须每帧按真实相机投影更新，处理裁剪、guard band、放置锚点和深度；它不是旧的离线角度图集，也不宜默认一虫一相机/一目标。
- **相机不应为像素风强换正交。** 正交不随距离缩小、易设世界单位/像素关系；透视有近大远小，二者都能渲低像素。[6] 若主场景是透视，蝴蝶必须匹配其 view/projection、宽高比、裁剪面和深度约定；另用正交小相机会造成空间与遮挡不一致。任意角度可渲不等于每个角度都好看：薄翼恰好侧对镜头时仍可能消失。
- **固定采样网格，不逐帧 auto-fit。** 固定目标尺寸、整数放大倍数和网格原点；窗口尺寸不整除时明确边距/裁剪策略。避免每帧按包围盒改比例，避免对单个顶点分别取整造成翼形变形。屏幕锚点取整能减少子像素漂移，但会让平移阶梯化；自由旋转、透视缩放及拍翼改变覆盖本来就可能跳点，不能承诺完全无闪烁。Blender Studio 作者的实际实验也报告网格错位、移动跳点，最终保留固定相机；不是自由相机已解决的证据。[7]
- **不要让时间滤波破坏候选。** 首版蝴蝶层用无抖动投影，不加 TAA/FXAA、运动模糊或动态分辨率。TAA 官方说明涉及逐帧 jitter、历史融合、运动模糊感及 ghosting。[8] 若背景使用抖动/历史重投影，合成的坐标和深度仍须对齐，不能仅关一个开关。动作相位量化与相机更新分开：可保留少帧拍翼，但每帧仍按当前相机画真实几何。

## 遮挡、透明边及本仓库接入边界

以下仓库事实基于研究起点 `01d42cd5`，不是完整渲染审计：

- [`src/particles/animation.rs`](../../src/particles/animation.rs) 当前精灵为 16px、五帧、0.2 秒/帧，只消费逻辑两视图（物理行 1、3）。[`src/tracer/mod.rs`](../../src/tracer/mod.rs) 按速度与相机方向选视图及镜像并上传实例；[`particle_lod_textured.vert.slang`](../../shader/slang/particle_lod_textured.vert.slang) 存在相机朝向 billboard 路径。更换 PNG 或导出 GLB 不会自动替换这条绘制路径；现有 `ButterflyBlock` 枚举也不证明 v3 GLB 已接入。
- [`particle_lod_textured.frag.slang`](../../shader/slang/particle_lod_textured.frag.slang) 用 0.5 alpha 阈值构造掩码、输出预乘颜色和深度；透明区域深度设为 1，并非直接 `discard`。`record_all_graphics_passes` 内有把 ray-traced terrain 深度预填硬件深度的逻辑。因此新目标必须接入现有混合光追/栅格遮挡链，不能只把一张 RGBA 图最后覆盖上去。

**独立目标内的深度只能解决虫与虫/翼与翼，不能自动解决虫与地形。** 应在兼容的低分辨率深度上预填场景遮挡，或合成时显式比较蝴蝶和场景深度，并接入后续需要的深度输出。Three.js 也把 `depthBuffer` 与可供后处理采样的 `depthTexture` 分开，创建颜色目标不等于已有可用场景深度。[4]

不同分辨率处的轮廓必须选策略：逐原分辨率像素测试背景深度，遮挡准确但会切碎放大的方块；整块统一判定，方块完整却可能侵入或过度缩退地形边界。保守深度缩减也可能过遮挡；深度不能按颜色那样线性平均。需要以枝叶前后穿行、两虫交叠、靠地飞行实际检查，不把单深度层描述成对所有透明叠层都正确。

首版建议不透明色块/二值覆盖，薄翼确认双面或建模厚度；透明背景只表示“未覆盖”，不是半透明翼。Godot 官方材料 API 明确区分 alpha blend 与 alpha scissor，透明混合还有排序问题。[9] 若保留半透明边缘，必须统一预乘约定、清屏透明值、滤波和合成次序，避免黑边；最近邻放大不能修复源图已混入的背景色。严格调色板与软 alpha 混合后的背景颜色也不能同时简单保证。

## 多虫与下一步验收

共享网格、实例化的世界变换/拍翼相位/配色、视锥裁剪和共享低分辨率目标是合理起点；翼铰链可程序驱动，未必需要为每只维护完整骨骼播放器。这是待实现设计，**没有本机性能数据，不能说比现有 billboard 更快**：额外目标、深度处理、合成、过绘和 CPU 提交均有成本，低分辨率也不会消除几何成本。

后续最小候选应提供 Debug 运行时复选框（未勾选原精灵，勾选真实几何低像素层），不把网页演示当验收。先看无身体翼形在任意俯仰/侧视、连续转镜头、近远变化、遮挡与多虫交叠；再记录 release 隐藏运行下不同可见数量的 CPU/GPU 时间、目标尺寸及显存。视觉通过和性能通过分开记录。本次没有改代码、运行游戏或测性能，也不启动构建。

## 一手来源与限制

以下正文均已在线读取；引擎文档用于核对能力，不代表本仓库已经实现。动态 `stable`/`dev` 页面可能随版本变化；没有逐帧审阅嵌入视频，也没有证据支持行业占比、具体速度收益或 8–16px 自动成功。

1. [Thomas Vasseur / Motion Twin：Dead Cells 制作亲述](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-)：What、Result 小节；角色约 50px，不能直接外推极小蝴蝶。
2. [Pedro Medeiros：Using 3d models as pixel art reference](https://saint11.art/blog/3d-ref/)：作者混合制作实例，不是实时渲染方案。
3. [Cascadeur 对 t3ssel8r 的本人采访](https://cascadeur.com/blog/general/making-an-animation-for-a-3d-pixel-art-game)：确认实时真 3D 与动作取舍；不证明已发行或其完整后处理实现。
4. [Three.js RenderTarget](https://threejs.org/docs/pages/RenderTarget.html)：尺寸、颜色纹理、深度纹理及 `samples=0`；[Godot SubViewport](https://docs.godotengine.org/en/stable/classes/class_subviewport.html)、[Using Viewports](https://docs.godotengine.org/en/stable/tutorials/rendering/viewports.html)：独立渲染区域/目标纹理。
5. [Three.js Texture](https://threejs.org/docs/pages/Texture.html)：放大/缩小滤波配置；[官方 constants.js](https://github.com/mrdoob/three.js/blob/dev/src/constants.js)：`NearestFilter` 选择最近纹素。旧 constants 文档地址抓取为 404，改读官方源码，不引用失败页。
6. [Godot Camera3D](https://docs.godotengine.org/en/stable/classes/class_camera3d.html)：`PROJECTION_PERSPECTIVE`、`PROJECTION_ORTHOGONAL`、`cull_mask`。
7. [Rik Schutte / Blender Studio：Exploring 3D Pixel Art in Blender 4.2](https://studio.blender.org/blog/3d-pixel-art-in-blender/)：Static Camera、Shading、Conclusion；作者承认具体混合方案的固定相机限制。
8. [Godot 3D antialiasing](https://docs.godotengine.org/en/stable/tutorials/3d/3d_antialiasing.html)：TAA 小节的 jitter、blur、ghosting。
9. [Godot BaseMaterial3D](https://docs.godotengine.org/en/stable/classes/class_basematerial3d.html)：透明模式与剔除模式；是接口/局限参考，不是移植承诺。
