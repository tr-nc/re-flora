# 直接照明能否存入 DDGI／共享光场？

调研日期：2026-11-03。范围：原始文献、后续研究与第一方应用资料；不实现渲染改动，不承诺本项目收益。代码观察基线：`28184813a614c5705a957aa8b2db6612c3ed2a63`。

[可视化摘要](../../reports/ddgi-direct-light/index.html)。下文是证据与推理记录，不是实现规格。链接到论文，不在仓库分发论文或全文转换。

## 结论与对先前判断的修正

**会，而且原始 DDGI 已明确描述面积光源的直接照明经过探针管线提供。** 2019 论文 §5.1（PDF p.11）并不只说“灯照亮墙、墙再照亮别处”：它专门说明，以面积光源的 emission profile 而非已经着色的 direct-illumination profile 为种子，可以用该管线计算面积光源直接照明，并给出浴室窗光示例。[S1]

因此，“DDGI 只能存间接光，必须另建一张直接光场”是过强的判断。更准确的是：

- DDGI 存储聚合的方向性 irradiance；其包含哪些光路，取决于更新器喂入什么 radiance。[S1 §4–5]
- 常见混合渲染仍将解析点光／方向光的直接照明独立计算；这是采样和质量选择，不是 irradiance 表示禁止直接光。[S1 §5.1；S3]
- 面积发光体的直接漫反射可以经由探针采样与插值提供。小而亮的发光体、尖锐遮挡、动态响应是主要困难。[S2 §4.2、§8.1；S4]
- 读取聚合场的接收端不必遍历光源，但更新场并非无条件、无误差地与灯数无关。[S1–S4；推论见后文]

## 先区分光路，而不是仅看 pass 名字

以接收点 x、另一个表面 y 和发光面 e 为例：

1. `e → x → camera`：对 x 是直接照明。若 probe 看见 e 并记录 emission，再由 x 查询 irradiance，物理上仍属于直接照明；probe 是计算采样点，不增加一次物理反射。[S1 §5.1]
2. `e → y → x → camera`：对 x 是间接照明。probe 命中 y、计算 y 的局部直接光并缓存它的出射 radiance，是这个路径。[S1 §4.3]
3. 把 y 的 direct-lit surface radiance 存进缓存，再供二次射线读取，不能单凭“缓存了 direct lighting”就推断 x 的直接光不再单独计算。Brixelizer GI 的官方说明正好明确：只用 direct lighting 填充 radiance cache，得到的是 one-bounce diffuse GI。[S9]

行业产品经常把发光体经 GI 管线提供的所有照明称为“indirect lighting”。本记录在讨论“能否替代草的逐灯照明”时按光路区分，同时保留来源原本的用词，避免概念偷换。

## 证据矩阵

| 来源 | 实际存储／处理什么 | 是否支持本问题中的方向 | 不能据此声称什么 |
| --- | --- | --- | --- |
| DDGI 2019，§5.1 | 明确将面积发光体 emission 注入探针照明管线，用于直接面积光照明 | **直接的原始证据** | 所有解析灯已经自动进入场，或可无损保留硬阴影 |
| Production DDGI 2021 | probe 更新、状态、重定位、级联与收敛；图 1 自动集成发光屏幕 | 支持动态发光体与多尺度场；继续报告小亮源拖影 | 后续优化已消除空间／时间误差 |
| NVIDIA RTXGI UE4 插件 | 发光表面可作为光源；建议减少 point/spot/area lights，换成部分 emissive surfaces | **直接的实际集成建议**，不是只有理论 | 官方建议所有灯一律删除，或精确阴影仍免费 |
| DDGI Resampling，2021/2022 | 把 DDGI 查询延后一个 bounce，与真实发光面候选统一 ReSTIR 重采样 | 是重要后续分支，但仍发射采样／可见性射线 | “把全部直接光存进 DDGI，主表面只查 8 probes” |
| Lumen | 发光材质通过 Final Gather 传播光；官方称该传播无额外性能成本 | **实际引擎中的发光体照明路线** | 任意小亮源、场景更新与整个渲染器都是零成本 |
| Radiance Cascades，作者 WIP | 缓存并合并不同距离区间的 radiance；PoE2 使用屏幕空间实现 | 更接近“对光场求解而非逐灯求和”的方向 | PoE2 已完全取消解析直接光，或其 2D/屏幕空间成本可直接迁移到 3D |
| Holographic RC，2025 arXiv 文稿 | 2D 多层方向性场；固定网格尺寸下成本与场景／灯数量脱钩 | 后续研究进一步验证这个方向 | 成熟的任意 3D 大世界方案；文稿明确指出 3D 内存 O(N⁴) 的问题 |
| Unity Baked Lights / Light Probes | 官方明确将 Baked Lights 的 direct + indirect 一起烘焙进 probes | **成熟的“接收端只读已合成照明”实例** | 支持本项目任意动态编辑／灯移动；这里是离线烘焙 |

### DDGI 原论文：最关键的两句话

§5.1，PDF p.11：[S1]

> “We can also handle direct illumination from extended area light sources using our indirect illumination pipeline”

> “seeding our indirect illumination shading routine with the area lighting emission profile”

同一节也明确点光和方向光采用 deferred renderer + variance shadow mapping。结论应同时保留这两面，而不是只引用“direct 独立”的部分。

### RTXGI：可用发光表面换掉一部分阴影灯

UE4 插件文档 “Emissive Surfaces are Light Sources with RTXGI” 说明，发光网格越大、越容易被探针看见，贡献越容易形成；可采用少量传统灯加 emissive surfaces，减少 shadow-casting lights 来提升性能。文档甚至提及只对光追可见的放大发光网格，但这属于艺术性近似，不能当作保持原光源形状的无损做法。[S4]

同一文档 “Sparse Probe Layouts Recommended” 明确警告：RTXGI 按设计不生成高频细节，增加密度也不保证精确、锐利的照明和阴影。不能从“面积直接光能存入探针”推出“原来每片草的可见性都能精确保留”。[S4]

### 后续研究不是只有“更多东西放进 probe”

DDGI Resampling（作者稿 2021；期刊条目 2022）将 DDGI 查询推迟到 secondary vertex，像 final gathering 一样利用 primary scattering 隐藏粗空间离散误差，再把直接和间接候选统一重采样。论文 §3.1 的基线仍明确区分直接项与 DDGI 间接项；§3.2 才统一采样。它证明缓存与精细采样可以组合，但不是零射线的主表面查场方案。[S5]

Production DDGI 在 §4.2 讨论小亮源被偶然命中时的 firefly，在 §8.1 仍承认小亮源 ghosting。空间、方向和时间采样都要预算，单加 probe 密度不是万能解。[S2]

### Radiance Cascades 与 HRC 的应用边界

Sannikov 的作者仓库文稿明确标注 WIP；本文不把它描述为已完成的 JCGT 审稿论文。其 PoE2 应用是为固定相机设计的屏幕空间 raymarching，作者明确承认离屏光源、不可见遮挡与深度缓冲后方几何缺失；更一般的世界空间方案成本更高。[S7]

2025 HRC 文稿直接研究有限 2D 网格中的光传输，固定尺寸下性能不依赖光源数／几何复杂度。它仍有小光源混叠、模糊等误差，结论指出 3D 扩展内存为 O(N⁴)，示例 3D 实现甚至未实现 surface normals。不能把 2D 常数成本转换成 Re: Flora 的 3D 性能承诺。[S8，§5–6]

## 点光源为什么不能等同于发光面

PBRT 将点光源归类为 DeltaPosition，方向光归类为 DeltaDirection；面积光有真实几何形状。普通连续方向采样不能可靠命中一个无面积的理想点光源，需显式光源采样／解析注入。小但非零面积的光源可以命中，却会因低概率产生高方差。[S10]

如果要把解析灯也汇总到场里，可在 probe 更新处显式求其直接入射照明，再由接收端查询。这是依据渲染方程与 probe 表示提出的设计方向，不声称 RTXGI SDK 已自动这么做。需要处理：

- probe 位置到实际接收点之间的近场位置误差和遮挡差异；
- 多个方向的累加、正确的 PDF／积分权重；
- 光源被显式采样且又被普通 probe ray 命中时的去重／MIS；
- 已有主表面直接光、发光体几何与代理灯之间的路径所有权。

尤其不能把点光源的 RGB 直接写进某一个空间 cell 就认为传播和阴影完成。

## “光源数不影响速度”究竟是哪一层？

**推论，不是本项目基准结果：**

- 接收端：固定八探针查询可以不遍历灯，避免 `grass samples × lights × shadow taps`。
- 常规 DDGI 构建端：固定 probe ray 数并不代表固定着色成本；若每个命中点仍枚举所有灯，其成本仍随灯数增长。[S3]
- 发光几何采样：固定数量射线可对大量发光体隐式积分，不需要逐个枚举；代价转移到空间／方向覆盖、收敛、几何遍历和场更新。
- 在要求精细直接光时，还有独立的 stochastic direct lighting 路线。Epic 的 MegaLights 官方文档明确采用每像素固定射线数，在灯复杂度上升时主要损失质量而非继续线性增加着色成本。这不是 DDGI，也不是免除所有加速结构维护开销。[S11]

因此合理的目标是：**草的读取成本与灯数解耦，场更新有界，质量和响应速度有明确验收。** 不是在任意灯数、任意几何和任意动态变化下同时保持精确结果与绝对固定总成本。

## 对 Re: Flora 的研究建议（未实施）

基线代码已经具有两条相关路径：

- [`ddgi_probe_trace.slang`](../../../shader/slang/ddgi_probe_trace.slang) 的 `ddgiTransportHitRadiance` 返回 `surfaceEmission + albedo * (directIrradiance + indirectIrradiance)`；所以 probe 直接命中发光体时，**本来就能记录其 emission**，并非完全缺少这条输入。
- [`flora_lighting_cache.comp.slang`](../../../shader/slang/flora_lighting_cache.comp.slang) 先查询 DDGI，再额外执行逐灯局部照明。

由此应修正实验顺序：

1. **先检查现有发光体入场的有效性**：固定石灯／草地／遮挡场景，观察 emission-only 场的稳态能量、覆盖、更新时间、小灯闪烁，而不是先新建一个直接光 atlas。
2. 明确 source/path accounting：哪个物理发光面由几何 emission 表示、哪些解析灯只是代理、哪些路径由草的显式直接项提供。现有两条代码同时存在本身不足以证明已发生能量双算；需要分项输出／参考结果核验。
3. 若大面积发光体表现足够好，可实验“草只读包含局部发光体贡献的场、太阳阴影独立”。保留运行时 A/B 和原模式；这是图像近似的比较，不是宣称严格等价。
4. 若小石灯经常漏采，先比较探针端重要性采样／显式局部光注入与局部加密，再决定是否需要独立分量。不得简单同时加上两份同路径能量。
5. 以 1/8/32/128 灯分别测 **草查询、场更新、总帧时间**，同时量化稳态亮度、阴影边界、漏光与开关灯／挖墙响应。当前运行时 light capacity 也是实验前需要明确处理的限制；不能只在未真正增加有效灯数时宣称扩展成功。

这轮只查文献与代码，不进行 A/B、不更改画质、不新增渲染功能，也不声称已有提速。首选方向是验证已有 DDGI 的 emitter-only 能力，而不是据名称判断它只能用于间接光。

## 来源与定位

全部于 2026-11-03 实际读取；网页／GitHub main 是访问时版本，论文以链接文稿为准。只短引必要原文，其余为独立摘要。

- **S1** Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*, JCGT 2019。[论文](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=11)，§4.3、§5.1–5.3。面积光直接照明的核心证据在 p.11。
- **S2** Majercik et al., *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production*, JCGT 2021。[论文](https://jcgt.org/published/0010/02/01/paper-lowres.pdf)，图 1、§4.2–4.3、§7.4、§8.1。
- **S3** NVIDIA, [RTXGI Integration Guide](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/Integration.md)，Tracing Probe Rays；[Test Harness ProbeTraceRGS](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl)。后者说明 probe hit 仍执行 direct shading；该样例未显式显示 emission 累加，不能用它代替 S4 的插件证据。
- **S4** NVIDIA, [RTXGI UE4 Plugin README](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/ue4-plugin/4.27/RTXGI/README.md#emissive-surfaces-are-light-sources-with-rtxgi)，Emissive Surfaces are Light Sources with RTXGI；Sparse Probe Layouts Recommended。
- **S5** Majercik et al., *Dynamic Diffuse Global Illumination Resampling*。[作者稿](https://casual-effects.com/research/Majercik2021Resampling/Majercik2021Resampling.pdf)，pp.2–4、§3.1–3.2；[NVIDIA 发表记录](https://research.nvidia.com/publication/2021-12_dynamic-diffuse-global-illumination-resampling)。按作者稿 2021／期刊 2022 区分年份。
- **S6** Epic, [Lumen Global Illumination and Reflections](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine#lumenandemissivematerials)，Lumen and Emissive Materials。读取时页面标注 UE 5.8；没有在本机运行 Lumen。
- **S7** Alexander Sannikov, [Radiance Cascades 作者文稿源码](https://github.com/Raikiri/RadianceCascadesPaper/blob/main/RadianceCascades.tex)，Introduction、Path of Exile 2 GI、Limitations；WIP，不作正式审稿状态断言。
- **S8** Rouli Freeman, Alexander Sannikov, Adrian Margel, [Holographic Radiance Cascades for 2D Global Illumination](https://arxiv.org/pdf/2505.02041)，2025 arXiv 文稿，§5–6。作者与限制按实际读取 PDF，而不是搜索摘要。
- **S9** AMD, [FidelityFX Brixelizer/GI](https://gpuopen.com/fidelityfx-brixelizer/)，Algorithm overview - Brixelizer GI。作为“缓存 direct-lit radiance 不等于替代主表面直接光”的反例。
- **S10** Pharr, Jakob, Humphreys, [PBRT v4: Light Interface](https://www.pbr-book.org/4ed/Light_Sources/Light_Interface)，LightType、SampleLi、delta lights 与 small area light sampling。
- **S11** Epic, [MegaLights](https://dev.epicgames.com/documentation/en-us/unreal-engine/megalights-in-unreal-engine#techniqueoverview)，Technique Overview；固定射线与质量／性能权衡。读取时页面标注 UE 5.8；不属于 DDGI。
- **S12** Unity, [Light Modes](https://docs.unity3d.com/6000.0/Documentation/Manual/LightMode-Baked.html)，Baked Light behavior 明确写 direct + indirect → Light Probes；[APV Introduction](https://docs.unity3d.com/6000.0/Documentation/Manual/urp/probevolumes-concept.html) 说明八探针查询和烘焙场景。这里的肯定证据是 Baked Lights / Light Probes 的明确陈述，不把 APV 简介中的“baked indirect”泛化成所有模式都烘焙直接光。

### 未采纳的材料与限制

- 搜索摘要曾把 DDGI 简化为“只存间接光”，与 S1 §5.1 不完整一致；以论文正文为准。
- `takahiroharada.github.io/gi-1.0/` 页面末尾标注由 `gpt-oss:120b` 总结，本轮不将其作为 GI-1.0 论文事实依据。
- 没有做跨引擎或本项目的新性能测试。论文／引擎报告的帧时不直接外推到本项目软件体素遍历路径。
