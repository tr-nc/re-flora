# 成熟引擎的低成本水雾技术：哪些适合 Re: Flora

日期：2026-10-21；本仓库检查版本：`45392858`。
本轮是一手文档与公开源码调研，补充[上一轮光学热点分析](sprinkler-optical-optimization.md)。
**没有实现替代渲染、导入引擎/资产、购买工具或运行新的 GPU 性能实验。**

## 结论先行

确实有成熟的低成本路线，不必把所有视觉水雾都表达成“每个像素遍历所有三维核”。
但“工业级”不是一种算法，也不意味着完整体积雾一定比粒子便宜。

对我们最值得比较的是：

1. **保真路线：以 raster proxy 或核级 tiled rendering 枚举局部覆盖，保留现有解析雾核。**
   借用成熟粒子系统的工作组织，不必先改变光学场。
2. **更激进的视觉路线：柔和雾气用轻量局部体积/面片，清晰水滴独立渲染，近地形保留精细处理。**
   保留真实喷射/接触/声音时间，但允许光学表达近似；需明确的原版/实验版视觉 A/B。
3. **共享 froxel 体积作为后备**：适合未来全园雾、体积光和很多局部效果共用底座；
   不是当前16个近景喷头必然更省的直接替换。

已有同场景测量：16喷头 optical pass 12.430 ms，整帧22.734 ms；1喷头 optical仅0.389 ms。
这里引用的是上一轮66次Release测量，不是这些候选的成绩。新系统必须同时检查轻场景的固定成本。

## 1. 一手证据支持的成熟路线

| 路线 | 工业/公开实现证据 | 省成本的方式 | 对我们最重要的代价 |
|---|---|---|---|
| Sprite / billboard / soft particle | Godot官方建议、Niagara文档、AMD GPU粒子sample [S1,S5,S6] | 按局部几何覆盖绘制；片元取纹理/轻量光照，避免逐像素扫描全量核 | 相机进入、视差、交叠排序；大量近景透明覆盖仍会很贵 |
| Six-way lit flipbook | Unity VFX Graph内置Six-Way Smoke Lit [S3] | 将六方向光照响应预烘焙到纹理，运行时重建有体积感的2D特效 | 不是完整3D介质；要制作资产，不能自动提供水滴高光/折射 |
| 快速解析局部雾体 | Unreal Local Fog Volumes [S2] | 简单密度模型、解析积分、屏幕tile剔除；独立路径不算完整体积光影 | 默认形状/密度自由度有限，重叠合成排序有差别，缺少完整体积阴影 |
| 共享低分辨率3D介质网格 | UE、HDRP、Godot的体积雾；Frostbite统一体积框架 [S1,S4,S7,S8] | 先注入介质再在网格上算光照/积分，让许多像素共享结果 | 网格与历史缓冲固定成本；薄喷雾、近景水滴、快速脉冲容易失真 |
| 低分辨率特效层＋边缘重建 | NVIDIA GPU Gems 3 第23章 [S9] | 柔雾在较小目标上绘制，再合成；必要时边缘回到高分辨率 | 深度边缘halo、薄细节丢失与重建成本；不是整帧按像素比例加速 |

### 1.1 粒子面片不是“只能做廉价假雾”

Godot官方明确列出：quads通常比体积雾便宜，不依赖时间重投影，适合快速移动与细小细节；
同时警告相机进入时衰减较假、透明排序，以及大量近景sprites可能反而更贵。[S1]
Niagara官方也把透明sprite/mesh的屏幕覆盖与overdraw作为核心检查项，而不是只看粒子个数。[S5]

这对我们意味着：可以复用现有GPU发布的位置、年龄、消亡/接触信息，绘制小型局部代理，
**无需再实现一套独立CPU粒子模拟或另一个喷水时钟**。
但“一个喷头贴一张很大的半透明烟雾图”不是可靠低成本方案：空白quad区域、重叠、近景视差
都会出问题。应控制真实屏幕覆盖，分别处理柔雾和高频水滴。

soft particles的深度淡化只解决视觉交接，不能替代真实地形碰撞或保证体积不会延伸过薄墙。
AMD sample虽然有depth-buffer collision，也不能替换我们包含离屏地形的3D接触事实。

### 1.2 Six-way：可让2D雾团有光照层次，但不是本项目必选项

Unity VFX Graph 17.5文档提供内置输出，适用于HDRP与URP。六方向数据存于**两张RGBA纹理**，
不是运行时算六次体积光追；方向响应随场景光照混合。它支持探针/场景灯光等输入，
文档同时说明不支持reflection probe的specular contribution。[S3]

适合把带自遮蔽感的雾团烘焙成小型sprite/flipbook。对于我们的稀薄水雾，简单相函数＋
已准备的环境光/太阳可见性可能更合适：现有光照已按核预计算，prepare约0.23ms，
所以换six-way本身不等于解决12ms热点。主要节省仍来自**更便宜的空间表示与覆盖枚举**。
水雾也不能直接照搬黑烟材质；应保留水基介质的低吸收、方向性与独立水珠观感。[S4]

### 1.3 UE Local Fog Volumes：真正存在的轻量解析雾，不是完整Volumetric Fog

已读Epic文档确认：

- 独立Local Fog Volume是球形范围，结合径向/高度密度；不支持非均匀缩放。
- 使用快速解析积分，独立路径忽略阴影和light functions；接入Volumetric Fog可有更完整
  的体积光照/阴影，但成本提高。
- 使用屏幕tile剔除；成本受覆盖和重叠影响，镜头处于多个volume内是高成本情形。
- 独立路径按volume中心到相机的距离合成，可设sort priority；体素化参与介质则是另一种混合结果。
- 文档中的`r.LocalFogVolume.HalfResolution`注明**仅移动渲染路径**，不能说桌面也默认这样做。[S2]

可借鉴“简单局部介质＋解析积分＋细粒度剔除”，但我们已经对紧支撑核使用解析积分，
新收益不能只靠改名。若把256个光学核拟合为少量体积，属于**光学形状近似**，需视觉批准；
这与减少256个真实喷射/接触采样是两件事，后者本轮不建议动。
不能照抄引擎的距离起始值、实例上限或排序裁减来隐藏我们的近景喷雾。

### 1.4 Froxel / 统一体积：成熟，但有针对性的折中

UE文档明确描述相机视锥内的低分辨率volume texture、深度slices与heavy temporal reprojection；
快速变化灯光会留下拖尾，Volume-domain粒子注入也可能有很高的3D overdraw。[S4]
HDRP 17.0文档建议减少过大的fog volume、降低V-buffer屏幕比例；如果voxelization比lighting
还慢，说明注入体积太多/太大，换成体积网格并不会自动免除源的成本。[S7]

Godot源码可直接观察到：[S10]

- `volumetric_fog.glsl`将局部介质注入3D density/scattering/emission数据；
- `volumetric_fog_process.glsl`包含光照、历史重投影、滤波、沿Z积分；
- `MODE_FOG`按depth slice计算Beer透射并累计散射，而不是最终像素重新遍历所有粒子。

Godot官方还明确警告移动FogVolume的历史拖影，以及薄体积随相机移动出现闪烁。[S1]
我们的近景喷射、5–20Hz脉冲和细小水滴正是这种折中的敏感场景。可将慢变柔雾放入网格、
高频水滴留在原分辨率；但不应直接将整个喷流模糊成低分辨率雾团。

Frostbite SIGGRAPH 2015官方课程摘要提出统一物理参数、级联消光体积、粒子体素化、
体积阴影图与最终介质渲染。[S8] 这支持“共享介质底座”的工业路线；不能把各引擎的网格、
阴影能力或历史过滤说成完全相同，更不能拿旧硬件的演示毫秒数保证我们的结果。

### 1.5 低分辨率层是成熟手段，但适合柔雾而非所有水珠

GPU Gems的原文把低频烟雾作为适用对象，高频粒子作为不适合的对象，并讨论深度降采样、
halo、mixed-resolution边缘重画及其额外成本。[S9]

我们的内部场景已是820×512，半宽半高变为410×256。可以做**柔雾较低分辨率、水滴维持
当前场景分辨率**的混合，但要检查草叶细边、地形剪影与镜头内喷雾。四分之一的像素数
不等于整帧快四倍；也不能未经用户确认改保存的场景分辨率。

## 2. 开源实现能否直接拿来用

### AMD GPUOpen：最贴近上一轮候选筛选思路的现成参考

检查了`GPUParticles11`提交`1f2ef4f0cf2025238c7823eede0ec0db7346efab`的
`CoarseCullingCS.hlsl`、`CullingCS.hlsl`、`TiledRendering.hlsl`：[S6]

- 既提供rasterized billboards，也提供compute tiled rendering；
- 有coarse bin、fine tile粒子索引、groupshared缓存，说明“局部候选＋共享数据”有公开实现可借鉴；
- **不能照搬其近似**：渲染阶段最多取500粒子/每tile，源码明确说明截掉后方粒子；
  还按`g_AlphaThreshold`提前退出并强制alpha=1。它们不符合我们保留所有光学贡献的基线要求。
- sample使用前到后排序的alpha合成；我们的当前模型先求和`opticalDepth/source`再统一合成，
  两者不等价，不能连合成方式一起无声替换。

这是2016年的DirectX 11 sample，不是可直接链接的Rust/Vulkan库，也没有本项目性能保证。
其根许可及所读shader文件包含MIT式许可；后续摘取需保留声明并单独核查其它文件/资产。
**本轮没有复制任何实现或贴图进项目。**

### Godot与Unity/Unreal的复用边界

- Godot引擎根许可是MIT；公开GLSL与渲染结构可作参考，但依赖其灯光、cluster、资源系统，
  仍需适配本项目的Slang/Vulkan及资源生命周期，不是直接把整个fog模块搬入。[S10]
- Unity Graphics根许可明确为 **Unity Companion License for Unity-dependent projects**；
  不能把它当作可任意移植到Rust引擎的MIT代码。本文仅参考公开算法和文档。[S11]
- Niagara、UE Local Fog Volumes、HDRP/VFX Graph是各自引擎组件，不是独立中间件。
  使用公开设计思想不等于获得引擎源码/特效资产的再分发权。未使用受限UE源码。

## 3. 对我们更具体的候选组合

### A. 保真候选：成熟粒子覆盖枚举＋现有体积积分

不改变256个雾核与128个水滴的生成：

1. 为每个有效核生成保守屏幕代理，由rasterizer覆盖像素；或沿用compute但建立核级tile候选。
2. 仅在覆盖位置计算**原来的**ray/ellipsoid交段、地形裁切、解析积分。
3. 累计optical depth和source，再执行原来的最终合成，不用普通sprite alpha替代。

它借鉴工业粒子渲染的调度，却不必用烟雾贴图替代三维场。raster路线新增累积目标、
清空/混合/合成带宽，且浮点顺序、格式支持、精度与相机入体积都要验证；不能预告比compute快。
两种覆盖枚举应与当前版本做同输入A/B，由实测决定，不先押注其中一个。

### B. 低成本视觉候选：分层混合，而非把全部雾体塞进同一种算法

```text
现有权威喷射时钟 / 完整源与接触数据（不变）
  ├─ 清晰水滴：独立局部覆盖绘制，保留真实出生/轨迹/触地消亡
  ├─ 柔和雾体：轻量sprite或少量局部体积；必要时仅这一层降分辨率
  └─ 薄墙、台阶、近景：保留精细处理或有依据的完整回退
```

这条路线**允许光学近似，但不允许通过改浇水/时间/接触制造“性能提升”**。
若只改变显示表示，可以继续计算便宜的完整共享源，不需要降低物理采样或改声音。
表达切换/混合不能重复计入同一份光学量，也不能让相机靠近时喷雾突然变亮或消失。

最关键的边界：屏幕depth fade不等于三维地形裁切；降低分辨率也不能靠吞掉薄墙来省成本。
精细回退的范围要计入总成本，不把它作为未实现的安全承诺。

### C. 以后确有共享需求，再做统一froxel底座

如果全园天气雾、光束、多个局部介质都要共享光照，那么统一体积的工程投入更合理。
只有几个喷水器时，先确认网格/注入/历史/上采样固定成本是否值得；目前单喷头0.389ms
是实际需要保护的轻场景基线，不能只用16喷头演示掩盖单喷头退化。

## 4. 如何判定“工业替代真的更便宜”

- 保留原模式；所有实验通过声明式保存的Debug A/B复选框切换，unchecked=原版。
- 首先实际看近景水滴、太阳方向反馈、5/20Hz脉冲、移动镜头/进入雾体、墙/台阶/草边缘。
  B类有意近似不能宣称像素等价；A类需同一帧同一源/深度/背景的误差检查。
- 同花园、同相机、同保存画质，0/1/4/8/16喷头，30%/100% duty，spread/overlap，多轮暖机
  交错Release测量。统计新目标清空、注入、绘制、合成/上采样和回退，不只比较某个旧scope。
- 光学外的出水、接触、湿土、音频与生命周期不变；0% duty、暂停、remove/resize、玻璃与
  相机在体积内仍需正确。不能靠500项截断、实例上限、距离隐藏或不安全opacity早退冒充无损优化。
- 用现有runner与重资源锁规则，保留原始证据。任何文献/引擎样例毫秒数都不能替代本机测量。

**调研后的优先级变化**：不只在当前大循环里做局部微调；将“按局部覆盖绘制”提升为与核级
compute筛选并列的保真候选，再准备一个明确可见的低成本混合表示供用户评审。
暂不为喷水器直接引入整套全场景体积引擎，也不优先增加流体模拟或液面重建。

## 5. 一手来源与核查范围

- **[S1] Godot官方文档**：[Volumetric fog and fog volumes](https://docs.godotengine.org/en/stable/tutorials/3d/volumetric_fog.html)。
  本次stable页面标记4.7；已读性能/历史拖影/薄体积及“Faking volumetric fog using quads”。
- **[S2] Epic官方文档**：[Local Fog Volumes](https://dev.epicgames.com/documentation/en-us/unreal-engine/local-fog-volumes-in-unreal-engine)。
  页面标记UE5.8；已读解析光照、形状限制、排序、tile成本与mobile-only半分辨率说明。
- **[S3] Unity官方文档**：[Six-way lighting, VFX Graph 17.5](https://docs.unity3d.com/Packages/com.unity.visualeffectgraph@17.5/manual/six-way-lighting.html)。
  已读六方向预烘焙、两张RGBA通道、内置输出、HDRP/URP与不支持reflection-probe specular的限制。
- **[S4] Epic官方文档**：[Volumetric Fog](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-fog-in-unreal-engine)。
  已读视锥低分辨率volume、temporal reprojection、灯光拖尾和3D overdraw；未外推PS4/GTX970旧样例数字。
- **[S5] Epic官方文档**：[Measuring Performance in Niagara](https://dev.epicgames.com/documentation/en-us/unreal-engine/measuring-performance-in-niagara)。
  已读实际场景测量、透明覆盖、Quad Overdraw及降粒子数改变视觉的说明。
- **[S6] AMD GPUOpen**：[GPUParticles11说明](https://gpuopen.com/learn/gpuparticles11-directx-11-sdk-sample/)，
  [固定版本shader目录](https://github.com/GPUOpen-LibrariesAndSDKs/GPUParticles11/tree/1f2ef4f0cf2025238c7823eede0ec0db7346efab/gpuparticles11/src/Shaders)，
  [许可](https://github.com/GPUOpen-LibrariesAndSDKs/GPUParticles11/blob/1f2ef4f0cf2025238c7823eede0ec0db7346efab/LICENSE.txt)。
  已检查coarse/fine culling、shared缓存、500上限与alpha阈值退出，不以sample等同现成生产库。
- **[S7] Unity HDRP 17.0官方文档**：[Create a Fog Volume shader](https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@17.0/manual/create-a-fog-volume-shader.html)、
  [Create a local fog effect](https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@17.0/manual/create-a-local-fog-effect.html)。
  另交叉阅读官方Graphics仓库对应文档，区分voxelization与lighting成本；未把不同版本限制笼统合并。
- **[S8] SIGGRAPH 2015课程原站**：[Frostbite / Sébastien Hillaire讲座摘要](https://advances.realtimerendering.com/s2015/)。
  已读作者课程摘要；158MB含视频PPT未下载，未声称核查其全部幻灯片或其中具体性能。
- **[S9] NVIDIA / Iain Cantlay**：[GPU Gems 3, Chapter 23](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-23-high-speed-screen-particles)。
  已读低频/高频适用边界、深度降采样与mixed-resolution；引用方法而非旧硬件倍数。
- **[S10] Godot公开源码**：固定提交`679066724352c12673001d692d41dc2dab9314c0`，
  [局部介质注入](https://github.com/godotengine/godot/blob/679066724352c12673001d692d41dc2dab9314c0/servers/rendering/renderer_rd/shaders/environment/volumetric_fog.glsl)、
  [光照/滤波/历史/积分](https://github.com/godotengine/godot/blob/679066724352c12673001d692d41dc2dab9314c0/servers/rendering/renderer_rd/shaders/environment/volumetric_fog_process.glsl)、
  [MIT许可](https://github.com/godotengine/godot/blob/679066724352c12673001d692d41dc2dab9314c0/LICENSE.txt)。
  只检查相关shader路径，不声称审计整个引擎。
- **[S11] Unity Graphics许可声明**：[LICENSE.md](https://github.com/Unity-Technologies/Graphics/blob/master/LICENSE.md)。
  本次读取声明为Unity-dependent项目的Unity Companion License；不据此授权跨引擎直接复制。

本轮未产生候选画面或新的性能成绩；以上优先级是基于一手资料和本项目约束的工程建议。
