# 低分辨率像素风：远处高频细节的工业界处理方式

调研日期：2026-10-05。仓库基线：`experiment/global-low-resolution` / `9f8b3a5e`。

范围：调研与设计建议，不修改渲染、默认配置或生成文件，不做视觉或性能验收。下文区分**来源事实**、**采样原理推导**和**项目建议**；不是所有游戏采用同一套技术。

## 核心结论

这些引擎文档、商业渲染工具和制作亲述共同指向：**不应要求远处仍显示每一个高频细节，而应让这些细节平稳地转成可读的整体形状、覆盖和色块。** 纹理用 mipmap，几何用 LOD / billboard / proxy，闪烁用匹配问题类型的过滤；想保留关键特征，还需要美术上的重新表达，而不只是抗锯齿。[1][2][3][4][6]

需要先区分三种现象：

| 现象 | 含义 | 目标 |
| --- | --- | --- |
| 叶脉、花瓣小高光等微细节随距离平稳消失 | 正常的分辨率限制 | 接受简化，保住主要形状和颜色 |
| 小叶片、细线时隐时现，运动时闪烁或出现摩尔纹 | 混叠，或不稳定的覆盖估计 | 在采样前降低频率、改善覆盖与过滤 |
| 整个小花、枝条或树冠密度也丢失，无法识别对象 | 表现层级不合适，或覆盖被侵蚀 | 为远景换表示；必要时有选择地夸张关键特征 |

Godot 官方示例直接展示了低分辨率下植被闪烁、细线接近消失，以及各种 AA 的作用边界。[5] 上表的分类与处理目标是本调研的归纳，不是某一引擎的规范。

## 1. 为什么“加锐化”通常不能根治

**采样原理推导：** 一个场景像素只能输出一个颜色。它可以用平均颜色表达叶片覆盖了背景的一部分，却不能同时表达这个像素内部全部叶片的独立位置和间隙。因此要区分：

- **可检测**：一个亚像素亮点经过正确过滤，仍可能影响像素颜色而被注意到。
- **可分辨**：叶瓣、枝杈、花型内部结构能被独立辨认，需要更多实际采样。

不能把“低于一个像素”简单等同于“必须完全不可见”，也不能保证把所有细物体扩成一个不透明像素后仍保持正确密度。后者改变了面积和遮挡。

对透视投影，近似投影宽度为：

```text
scene_pixel_width ≈ world_width × focal_length_in_scene_pixels / view_depth
focal_length_in_scene_pixels = scene_height / (2 × tan(vertical_fov / 2))
```

这解释了为什么应考虑**实际场景渲染分辨率、FOV 和投影尺寸**，而不是只看世界距离。UE 的静态网格 LOD 使用 screen-space size 控制切换；但归一化 screen-size 本身不代表任意内部渲染分辨率下的绝对像素数。[2]

Epic 的纹理文档支持适量 mip 锐化，同时明确警告强锐化产生明暗边，法线图锐化可能加重高光混叠。[7] 这不是恢复已经丢失的几何覆盖或空间结构的方法。

## 2. 工业界的几条路线

### A. 纹理：先过滤，再采样；像素放大与 mipmap 不矛盾

Unity 和 Unreal 都提供 mipmap；Unreal 明确说明 mipmap 降低远处细纹理的 shimmering。Unity 也分别提供 mipmap 生成和 Point/Bilinear/Trilinear 采样选项。[1][7]

**项目建议：** 可以先让纹理在场景采样阶段得到合适的低频版本，最终再 nearest 放大低分辨率画面。最后一级仍是清楚的方块，不要求源纹理永远强取最高分辨率。

像素风还可以为较粗层级专门制作简化图案，把碎高光整理成少量色块。这是美术建议，不是本次已核实的所有像素游戏通用流程。Mip 只能处理纹理内容，不能让一根未被光栅化到的细枝重新出现。

### B. 几何：从细叶、细枝切到能表达整体的 LOD

Microsoft / Simplygon 的植被指南给出由近到远的组合：近处 triangle reduction，远处 billboard cloud、flipbook 或单视角 impostor。它明确指出过度减面会让叶片和细枝散掉、丢密度；对于卡通、块状、稠密植被以及远处 HLOD，remeshed proxy 也是可选路线。[3]

**项目建议：** 这里的重点不是“远处只少几个三角形”，而是改变表现单位：

- 单片叶 → 叶簇 → 树冠中的主要明暗块；
- 每根草叶 → 一小簇草的覆盖、方向和色块；
- 花瓣内部碎细节 → 花头整体轮廓和一个可辨认的颜色点。

应该保留整体覆盖、轮廓、主色与显著开口，而不是任意删叶，也不是把每片叶都强制放大。具体聚合方式尚未在本项目实现或验证。

### C. 覆盖：避免细物体“不是全有，就是全无”

Unity 的 **Preserve Coverage** 在生成 mip 时修正 alpha，使 alpha-test 下的覆盖得到保留；**Alpha Cutoff** 要匹配材质裁剪阈值。[1] NVIDIA 的 SpeedTree 章节展示 MSAA **alpha-to-coverage** 对叶片 / frond cutout 边缘和运动闪烁的改善，同时说明它并不等价于普通 alpha 混合，LOD 交叉淡入也需要专门处理。[4]

**适用边界：** alpha mip 覆盖修正针对 alpha-cutout 纹理。当前仓库公共 `flora.frag.slang` 返回不透明 alpha 1，不能把开启一个纹理导入选项当成对这条几何覆盖路径的修复。

**项目建议：** 对几何细枝可比较覆盖多采样，或在最终粗像素内做多点颜色采样 / 面积过滤。一个概念上正确的比较管线是：

```text
较密的场景采样
→ 在线性颜色空间做面积过滤，降到目标粗像素网格
→ nearest 放大
```

这改善粗像素的代表性，不增加最终画面的空间信息容量。它可能增加混合颜色、使细枝变淡，而且增加 GPU 工作量。若降采样仍只是取一个点，多渲染出来的样本并未被有效利用。

### D. 材质与运动：远处减少不稳定的高频信号

NVIDIA 的 SpeedTree 章节明确指出远树的镜面高光会 shimmer，因此按距离减弱 specular；树干轮廓的细节也在过渡区渐弱，而不是突然移除。[4] Godot 提供 roughness limiter，并说明普通 MSAA 主要处理几何覆盖，不能单独解决所有材质 / 高光混叠；TAA 对这类混叠有效，但可能模糊、拖影。[5]

**项目建议：** 远处减少逐叶尖锐高光、叶脉法线、碎阴影与独立快速摆动，保留树冠或草簇的整体明暗和摆动。这是从降低不可稳定采样的频率出发的设计建议，不是要求所有远处植物静止，也不是已有性能结论。

### E. 重要特征：选择性重新表达，而不是追求几何忠实

《Dead Cells》的美术制作亲述按最终约 50 像素高的角色来决定模型投入，用低分辨率无 AA 渲染生成像素动画；作者明确接受细节有限、优先动画，也承认当时没有彻底解决闪烁像素。[6] 它提供的是“以最终像素预算做取舍”的真实制作案例，不是自由透视远景渲染的直接方案。

商业 Unity 工具 ProPixelizer 允许整场景或选定材质像素化，以及 per-object pixel sizes。其 **Pixel Expansion** 在更细的底层采样格上生成并扩张像素，主要目标是改善 sub-pixel motion 与 pixel creep，不应宣称它能恢复所有远处细节。[8]

**项目建议：** 需要识别的小花、可交互枝条可以有选择地加粗、增大、提高与背景的对比，或采用更细的独立像素尺度。代价分别是透视 / 覆盖失真，或不再严格统一全屏像素大小。不能把“统一粗屏幕网格”“所有细枝保真”“所有距离都清晰”当成可以同时无代价满足的要求。

## 3. 与当前 Re: Flora 分支的对应

以下是代码检查，不是截图或运行时测量：

- [`scene_resolution.rs`](../../src/tracer/scene_resolution.rs) 的 `SCALE = 0.125`，并由 [`App` 初始化](../../src/app/core/mod.rs) 传给 tracer。2560×1440 对应 **320×180** 个场景像素；1920×1080 对应 **240×135**。
- [`post_processing.slang`](../../shader/slang/post_processing.slang) 对场景目标做整数坐标 point 读取再放大；一个场景像素约展开为 8×8 个输出像素。最终显示块变大，并不等于有更多源信息。
- [`flora_frame_plan.rs`](../../src/tracer/flora_frame_plan.rs) 中草与树叶的主绘制 LOD 仍根据 camera 到 chunk / bounds center 的世界距离选择。它没有在这些决策点直接度量细叶或细枝的实际场景像素宽度。
- [`flora.frag.slang`](../../shader/slang/flora.frag.slang) 的公共不透明植物 fragment 输出不使用 alpha-cutout 纹理；需按实际几何覆盖分析，不能直接套 alpha mip 修复。

**建议顺序，尚未实现：**

1. 用固定近 / 中 / 远视角判断：丢的是微纹理、独立几何覆盖，还是辨认对象所需的主要特征；同时检查静止和慢速移动画面。
2. 保持目标粗像素风格，先做“小花 / 草簇 / 树冠”的远景色块与轮廓 LOD；选择规则考虑实际场景像素尺寸，而不是仅调远景距离或全局锐化。
3. 对仍有闪烁的轮廓局部比较覆盖采样；让原版 / 候选能运行时 A/B。若未来实施，应使用可保存 Debug 设置，不在本轮直接添加开关。
4. 仅对必须辨认的细枝或小花尝试最小显示尺寸 / 独立像素密度，并明确记录密度、遮挡和风格代价。
5. 单独评估材质高光、阴影与风动。先看实际视觉结果；视觉获认可后，再用 Release 实机测量讨论优化与性能接受。

**一句话：近看叶片，远看叶簇，更远看树冠；保留视觉身份，不保留全部高频结构。** 这是本调研对来源的设计归纳。

## 4. 后续问题：高分辨率渲染后再整合成低分辨率，是否可行？

**可以。** 相对于最终的粗像素网格，这就是空间超采样（SSAA）与过滤降采样，再配合 nearest 放大。SSAA 是成熟的通用方法；Godot 有直接支持，NVIDIA 也详细描述了高分辨率渲染后按过滤核整合到最终像素的实现。[5][10] 但本次没有足够资料统计它在像素游戏中的普及率，不能称为所有像素风游戏的标配。

### 正确管线与一个常见误区

例如固定最终风格网格为 320×180：

```text
640×360 渲染场景
→ 每 2×2 个高分辨率样本做平均，得到 320×180
→ nearest 放大到 2560×1440
→ 原生分辨率 UI
```

640×360 已经是相对于 320×180 的超采样，不必先渲染到屏幕原生 2560×1440。每个最终粗像素有四个颜色样本，而不是一个。

**只把高分辨率图的 UV 吸附到粗网格中心，再读取一个 texel，不等价于过滤超采样。** 那样仍然可能错过细枝，只是多花了渲染成本。NVIDIA 给出的降采样是多个源样本的加权和，并说明提高采样率尤其有利于 subpixel geometry 和 shader variation。[10]

对这个项目，可先以线性颜色的 2×2 box average 作为清楚的对照，而不是大半径模糊。更宽的过滤核能进一步减少高频混叠，但也会改变像素间的对比与锐利程度；滤波选择属于视觉取舍。[10]

### 真实低保真游戏制作案例及边界

Lucas Pope 在《Return of the Obra Dinn》的开发日志中，明确描述**以 2× 分辨率做抖色阈值处理，再 box-downsample 到 1×**。它与球面映射的抖色坐标一起改善转头时的游动、摩尔纹和闪点，并保留低保真风格；代价之一是平均后的输出不再严格 1-bit。[9]

这是直接相关的制作案例，但要限定：该段披露的是**抖色阶段**的超采样，不足以宣称所有几何、光照 pass 都以同样的倍数渲染；稳定的图案坐标也参与了效果，不能把全部稳定性归功于超采样。日志是当时的制作说明，不是本次对最终发行版渲染器的源码审计。

### 优点

- **覆盖更有代表性。** 细枝、叶片只占粗像素的一部分时，有更多机会贡献颜色，不必只依赖一个中心点命中。对于已被这些样本解析到的细节，能减少突然漏掉和亮暗跳变。[10]
- **不仅处理几何边缘。** 完整颜色超采样也可过滤透明、材质和高光混叠，而普通覆盖 MSAA 并不提高所有 shader 的颜色采样率。[5]
- **最终块大小不必变。** 降到同一 320×180 再 nearest 放大后，每个显示块仍是单一颜色；区别是颜色估计更细致。这是管线推导，不是本项目已观察到的效果。
- **不依赖历史帧。** 单帧 SSAA 不需要 TAA 的历史积累，因而不会由它自身引入历史重投影拖影；但不保证屏幕粗网格下的所有运动闪烁消失。[5]

### 缺点与不可恢复的东西

- **成本主要在高分辨率渲染，不在最后平均。** 宽高各加倍意味着场景样本数变成四倍；受分辨率影响的颜色 / 深度附件、fragment shading、逐像素 ray tracing 与部分后处理会增加工作或存储。几何、CPU 与缓存工作不一定同比增长，因此不能把样本倍数直接说成实测帧耗时倍数。[5] 当前项目的具体成本未测量。
- **可能变软、颜色更多。** 黑枝与亮背景平均得到中间色；边缘的粗像素方块仍然清楚，但形状轮廓可能比无 AA 更淡。若要求严格两色或固定少色调色板，后续再次量化可能重新带来跳变。《Obra Dinn》作者明确记录了失去严格 1-bit 的取舍，也测试过再阈值化。[9]
- **不增加最终可分辨结构。** 一根宽 0.2 个粗像素的枝条，可以被表达为较淡的颜色贡献，不会因此变成清楚的一根完整枝条。多片叶若落在同一粗像素内，最终仍只有一个颜色；花瓣形状或枝杈辨认仍需要 LOD / 美术重新表达。
- **管线顺序要设计。** 高分辨率描边降采样后可能变薄、变淡；颜色可以过滤，深度、法线、对象 ID 则不能无条件用同一平均规则，否则跨物体边界的遮挡和描边语义会出问题。这是实施风险分析，本轮没有改动这些路径。
- **不是全部闪烁的统一修复。** 有限数量的样本仍可能漏掉更细几何；不稳定阴影、图集切换和屏幕格相位变化也需分别处理。

### 本项目值得先比较的候选

建议固定粗像素网格不变，在未来的运行时 Debug A/B 中比较直接渲染与 2× 每轴超采样后平均，先观察细枝完整度、草叶覆盖、边缘淡化与镜头移动。以下只有数学计数，**不是性能 benchmark**：

| 最终粗网格 | 场景渲染尺寸 | 每个粗像素颜色样本数 | 相对于直接渲染的场景像素数 |
| --- | --- | --- | --- |
| 320×180 | 320×180 | 1 | 1× |
| 320×180 | 640×360 | 4 | 4× |
| 320×180 | 960×540 | 9 | 9× |
| 320×180 | 1280×720 | 16 | 16× |

优先看 640×360 候选是否值得：它能检验当前是否主要是覆盖采样不足，不必先付出原生全屏渲染成本。若视觉认可，再用 Release 实机测量评估；它与远景 LOD 互补，不替代最终像素预算的取舍。本轮仅补充调研，未实现候选或运行游戏。

## 一手来源与核对边界

正文引用页面均已获取可读正文；未复现外部算法、未逐帧检查来源视频、未做本项目视觉或性能实验。引擎功能证明一种工业界技术存在，不证明它在本项目成本可接受，也不证明所有像素游戏都采用它。

1. **Unity 6：Default texture Import Settings**。Mipmaps、Preserve Coverage、Alpha Cutoff、采样过滤选项。<https://docs.unity3d.com/6000.0/Documentation/Manual/texture-type-default.html>
2. **Epic：Optimizing LOD Screen Size Per-Platform**。静态网格按 screen-space size 选择 LOD。<https://dev.epicgames.com/documentation/en-us/unreal-engine/optimizing-lod-screen-size-per-platform-in-unreal-engine>
3. **Microsoft / Simplygon：How to Optimize Vegetation with Simplygon**，2026-01-20。植被近到远的表示选择，以及块状卡通植被 proxy 的适用条件。<https://developer.microsoft.com/en-us/games/articles/2026/01/how-to-optimize-vegetation-with-simplygon/>
4. **NVIDIA GPU Gems 3，第 4 章：Next-Generation SpeedTree Rendering**。远处轮廓细节渐弱、specular shimmer、alpha-to-coverage 与 LOD 过渡的边界；是历史实时渲染案例，不是当前项目 benchmark。<https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering>
5. **Godot：3D antialiasing**。低分辨率细线 / 植被示例，MSAA、TAA、SSAA、roughness limiter 的效果与代价。<https://docs.godotengine.org/en/stable/tutorials/3d/3d_antialiasing.html>
6. **Thomas Vasseur / Motion Twin：Art Design Deep Dive — Using a 3D pipeline for 2D animation in Dead Cells**。制作人员本人文章；按最终低分辨率取舍细节与动画，亦明确承认未解决的闪烁。<https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i->
7. **Epic UE 4.27：Texture Properties**。Mip 对远处纹理 shimmering 的作用，以及锐化限制；相关描述来自此版本，不冒充所有新版参数都完全一致。<https://dev.epicgames.com/documentation/en-us/unreal-engine/texture-properties?application_version=4.27>
8. **ProPixelizer 作者文档：Pixelisation Controls**。低分辨率目标、混合渲染、Pixel Expansion、per-object pixel sizes；运动稳定性与细节保留须分别判断。<https://propixelizer.github.io/docs/usage/pixelization/>
9. **Lucas Pope：Fullscreen, Round 3**，2017-11-23。《Obra Dinn》球面抖色的 2× 阈值处理与 box-downsample，保持风格和不再严格 1-bit 的取舍；注意超采样披露的阶段范围。<https://dukope.com/devlogs/obra-dinn/tig-32/>
10. **NVIDIA GPU Gems 2，第 21 章：High-Quality Antialiased Rasterization**。高分辨率渲染、加权降采样、过滤核与 subpixel geometry；是历史通用 / 电影渲染技术说明，不是像素游戏普及率或本项目性能数据。<https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-21-high-quality-antialiased-rasterization>
