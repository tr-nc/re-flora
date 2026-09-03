# Re: Flora 每体素全局光照路线研究

> 日期：2026-08-13
>
> 范围：只研究动态 diffuse GI。直接太阳、VSM/叶片/云阴影与未来的镜面反射仍是独立问题。
>
> 证据标记：**【事实】**来自本仓库或一手来源；**【推断】**是把证据映射到 Re: Flora；**【建议】**是待 release-mode 实测验证的工程选择。

## 结论先行

**【建议】短中期不要用 surfel、Lumen 克隆或 ReSTIR GI 整体重写当前 DDGI。第一名路线是“现代化当前 DDGI，并保留统一的世界空间 irradiance 查询”；第二名才是“surfel 作为高频表面缓存，但 DDGI/probe cache 继续作为任意位置查询与 fallback”的 GIBS 式混合。**

理由不是 DDGI 最先进，而是它已经满足这个项目最难替代的产品契约：

```text
sampleDiffuseEnvironment(world_position, surface_normal)
    -> linear diffuse irradiance
```

terrain 可以向场中贡献太阳/天空反弹与多次 diffuse 传播，terrain tracer、raster flora、叶片、果实和未来 raster object 又能通过同一接口消费它。这个接口与“GI 的内部表示是不是规则 probe”应当继续解耦。

**【事实】“surfel 已经完全替代 DDGI”不符合目前最强的出货证据。** Frostbite 的 GIBS 确实以 surfel 解耦 ray-tracing rate 与 shading rate，但生产系统仍保留 probe clipmap：非 deferred/透明等任意位置 consumer 查询 probe；probe 也用于 surfel ray 和覆盖不足时的 fallback。College Football 25 已完全依赖 GIBS 处理 indirect diffuse；Skate 的 Xbox Series X、1440p flythrough 实测配置为约 100k surfels + 25k probes，完整 GIBS 平均约 3.2 ms、峰值 4 ms，优化配置约 2.5 ms 且低于 3 ms。这是一个**surfel + probe**的生产系统，不是 surfel-only 系统。[Frostbite, *Shipping Dynamic Global Illumination with GIBS*, SIGGRAPH 2024](https://advances.realtimerendering.com/s2024/content/EA-GIBS2/Apers_Advances-s2024_Shipping-Dynamic-GI.pdf)

**【事实】现代生产方案也没有离开 probe/cache。** Lumen 使用 Surface Cache、World Space Radiance Cache 和 Screen Probe Gather；Fortnite Chapter 4 在 PS5/Xbox Series 上以 4 ms 的 GI+reflection 预算、60 fps 出货。Assassin's Creed Shadows 的出货架构是 per-pixel trace 与 DDGI-like probe/irradiance cache 的混合。DOOM: The Dark Ages 的 idTech 8 GHOST 则把 spatial-hash world radiance cache 蒸馏进六级 cascaded + local irradiance volumes，再供 final gather 与透明 froxel volume 使用。三者都说明现代生产方向是**高频 producer/cache + 稳定 world-space field**，而不是淘汰后者。[Epic Lumen 技术细节](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine)、[Fortnite Chapter 4 的 Lumen 出货复盘](https://www.unrealengine.com/tech-blog/lumen-brings-real-time-global-illumination-to-fortnite-battle-royale-chapter-4?lang=en-US)、[Ubisoft, *Ray Tracing the World of Assassin's Creed Shadows*, SIGGRAPH 2025](https://advances.realtimerendering.com/s2025/content/Advances%202025%20-%20Raytracing%20the%20world%20of%20Assassin%27s%20Creed%20Shadows.pdf)、[id Software, *FAST AS HELL: idTech 8 Global Illumination*, SIGGRAPH 2025](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf)

因此本研究的最终选择是：

1. **现在：现代化并优化当前 DDGI**，继续解决 full-volume、full-precision 与无 per-probe activity 带来的更新成本；64 rays/probe、epoch rotation 与 temporal accumulation 已落地。
2. **随后：做有限的 terrain radiance producer cache + DDGI 实验**，优先让 probe ray 的重复 terrain hit 复用 surface-voxel/world-radiance cache；surfel 只是候选表示之一，DDGI 仍保留通用 consumer seam。
3. **不建议现在投入：**完整 Lumen/GI-1.0 克隆、纯 VCT/SDFGI、ReSTIR GI/path resampling、Radiance Cascades、neural cache。这些分别增加了整套场景表示/屏幕历史/去噪器或硬件限定，不能直接满足“任意 raster consumer 稳定查询”的核心需求。

## 1. 当前项目基线：已经拥有的不是早期 sky-only probes

**【事实】当前代码默认使用 32 voxel 的 probe spacing；512 voxel 的有限世界对应 `17 × 17 × 17 = 4,913` probes。每个 probe 每个 update epoch 追踪 64 条经全局 SO(3) rotation 的 Fibonacci directions，probe batch 为 512。Irradiance Map 是每 probe `8 × 8` interior、`RGBA32F`；Visibility Map 是 `16 × 16` interior、`RG32F`；两者都使用 full-precision source/destination atlas。** 见 [`src/ddgi/atlas.rs`](../src/ddgi/atlas.rs)、[`src/ddgi/resources.rs`](../src/ddgi/resources.rs) 与 [`shader/slang/ddgi_probe_trace.slang`](../shader/slang/ddgi_probe_trace.slang)。

**【事实】probe ray miss 写 authored sky；front-face terrain hit 写入 stable terrain albedo ×（exact direct sun + 可用的 source DDGI irradiance）。每个 epoch 读取不可变 source atlas、写另一 destination atlas，并对 irradiance/visibility 做 history accumulation；完整一轮后才发布。新 geometry/density 的 e0 不读旧 geometry history，但完成后立即可见。** 见 [`shader/slang/ddgi_probe_trace.slang`](../shader/slang/ddgi_probe_trace.slang) 与 [`docs/ddgi_indirect_transport_spec.md`](ddgi_indirect_transport_spec.md)。

**【事实】runtime consumer query 对 cage 的八个 probes 做 position、surface-side、moment visibility 与 support/confidence 加权；packed-voxel exact visibility 只保留给 probe transport 和 diagnostic/reference query。公开 seam 是 `sampleEnvironmentIrradiance(worldPosition, surfaceNormal)`；terrain compute 与 flora/leaf lighting cache 都通过它消费同一场。Raster Consumers 不进入 DDGI occluder geometry。** 见 [`shader/slang/ddgi_query.slang`](../shader/slang/ddgi_query.slang)、[`shader/slang/environment_lighting.slang`](../shader/slang/environment_lighting.slang)、[`shader/slang/flora_lighting_cache.comp.slang`](../shader/slang/flora_lighting_cache.comp.slang) 与 [`CONTEXT.md`](../CONTEXT.md)。

**【事实】当前 transport 有 hidden release 验收：sealed、portal、donor、dogleg 覆盖 no-created-energy、leak、颜色传播、多 epoch 传播、batch-order、revision 与 publication。收敛策略是至少 8 个 epoch、absolute delta `0.0025`、relative delta `0.02` 连续通过两轮，并以 128 个 epoch 为有限 sample budget；旧的代表性 spacing-32 曲线均以 `SampleBudget` 在 e63 睡眠，现作为 64-epoch 历史基线，而不是数值收敛证据。** 见 [`docs/ddgi_transport_acceptance.md`](ddgi_transport_acceptance.md)、[`docs/ddgi_convergence_calibration.md`](ddgi_convergence_calibration.md) 与 [`Cornell follow-up`](references/ddgi/cornell-box-grid-followup.md)。

**【事实】RTX 3060 Ti 上三个匹配的 release hidden `terrain-edits-closed` 样本产生六次完整更新：edit 到 e0 promotion 为 `31-36 ms`、median `34.5 ms`，旧两阶段日志的两次为 `87/88 ms`；静态 portal 在 e63 后没有新的 scheduler claim。这个结果证明当前生命周期响应更快且会休眠，但旧基线只有两个观测，不能外推成通用帧性能结论。** 见 [`docs/ddgi_transport_acceptance.md`](ddgi_transport_acceptance.md)。

**【事实】仓库里较早、匹配的 local environment probe 测量显示：spacing 32 的 steady `frame.render` median 为 6.146 ms、`tracer.render` median 为 4.339 ms，旧 global-SH bridge 的平均值约 5.325/2.977 ms，即 position-dependent visibility 当时约增加 1.24/1.26 ms；但该测量发生在现有 multi-bounce transport、cache 与 lifecycle 继续演化以前，不能当作当前性能基线。** 历史测量保留在 Git 提交 [`fc30f14a`](https://github.com/tr-nc/re-flora/blob/fc30f14ac6dc83b49206c8bf4430806c7fd3ebb3/docs/local_environment_probe_plan.md)。

**【推断】当前主要问题不是算法没有 terrain bounce 或 temporal sampling，而是 production scheduling 仍不够细：** full-precision 双 atlas、所有 probes 等额工作、full-volume terrain rebuild 和 128-epoch 全场 budget 仍偏重。下一步应测量 raw variability、per-probe activity 与局部 invalidation，而不是继续增加历史长度。

## 2. 候选方法的成熟度与项目适配排名

排名同时考虑：低噪、steady/update 成本、动态 terrain、任意 world-position consumer、生产证据、实现风险。这里的“常见采用”不是论文引用数，而是公开的出货/引擎证据。

| 排名 | 路线 | 生产成熟度 | 屏幕噪声 | 任意 `position + normal` 查询 | 对本项目的结论 |
|---:|---|---|---|---|---|
| 1 | 现代化 DDGI / irradiance field | **生产证明；多引擎/商业集成** | 低；误差主要是空间泄漏、延迟和低频化 | **原生支持** | 保留并优化；最小风险 |
| 2 | GIBS 式 surfel + probe | **AAA 出货**，但公开生产族群较窄 | 低到中；靠持久 cache、filter 与时间累计 | **靠 probe clipmap，不是 surfel 单独完成** | 长期高质量混合实验 |
| 3 | Lumen / GHOST / GI-1.0 / Brixelizer GI 式 surface/world/screen cache | Lumen、GHOST **已出货**；Brixelizer 为官方 SDK/sample | 低到中；明显依赖 temporal cache、screen probes、reprojection/denoise | 有 world cache/volume 才支持，不能只靠 screen result | producer cache 架构值得借鉴，整套移植代价太大 |
| 4 | Voxel cone tracing / SVOGI / SDFGI clipmap | **长期生产证明**，技术较老 | 通常平滑；伪影是 leak、ghost、aliasing、级联切换 | 可另建 irradiance volume；常见实现偏 screen resolve | voxel-native 有诱惑，但重复场景表示与分辨率问题明显 |
| 5 | ReSTIR GI / ReSTIR PT / SHaRC path cache | 以论文、官方源码/SDK 为主；本文未找到足以列为广泛出货方案的 SHaRC 一手证据 | 比 naïve PT 低很多，但仍需要 denoiser/history | 不是 irradiance field；通常要从 shading point 发 ray/path | 适合未来高端 path mode，不适合替换当前通用 GI seam |
| 6 | Neural radiance cache | 研究强；现有开放产品仍有 experimental/technical preview 限定 | 可显著降噪，但训练本身有响应/稳定性风险 | 预测 radiance 而非廉价通用 irradiance lookup | 不适合当前平台与工程规模 |
| 7 | 3D Radiance Cascades | 2D/WIP 社区实现与辐射传输论文；**无可核实的 3D 游戏出货证据** | 理论上可避免 Monte Carlo 噪声 | 尚无成熟 3D 任意表面 consumer 契约 | 观察，不进生产路线 |

### 2.1 DDGI / irradiance fields

**【事实】DDGI 将动态 irradiance 与距离矩存入规则 probes，用 visibility-aware interpolation 在任意世界坐标查询；2019 原论文明确定位为 dynamic scenes 的 compact full irradiance field。2021 production paper 记录了它进入 RTXGI、Unity、Unreal Engine 4 与多个商业引擎过程中形成的 state machine、relocation、cascaded volumes、memory/performance 与 artist control 改进。** [Majercik et al. 2019](https://research.nvidia.com/publication/2019-05_dynamic-diffuse-global-illumination-ray-traced-irradiance-fields)、[Majercik et al. 2021](https://research.nvidia.com/publication/2021-05_scaling-probe-based-real-time-dynamic-global-illumination-production)、[RTXGI v1 official source/integration guide](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/Integration.md)

**【事实】官方 RTXGI integration 明确要求 probe front-face hit 计算 direct lighting，再递归采 nearby probe irradiance；query 接收 world position、surface bias 与 geometric normal。NVIDIA 给出的目标量级是 fixed-time 约 1–2 ms/frame，同时公开承认低端 GPU 的 world-space lighting lag、single-sided/zero-thickness leak 与 diffuse-only 限制。** [RTXGI integration guide](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/Integration.md)、[NVIDIA RTX Global Illumination Part I](https://developer.nvidia.com/blog/rtx-global-illumination-part-i/)

**【推断】这与 Re: Flora 的 voxel marcher 比硬件 RT 绑定更弱。** 项目已经能直接从 probe trace 访问 authoritative voxel occupancy/material，且 consumer 是 O(8) 附近 probes 的 deterministic gather；无需引入 TLAS、GBuffer spawn、per-pixel GI history 或全图 denoiser。DDGI 的主要缺陷——稀疏场导致低频化、thin-wall/relocation/interpolation seam、radiance update 延迟——也正好已有 exact voxel visibility 和 acceptance fixtures 可量化。

**【建议】把“现代化 DDGI”定义为当前算法的 production pass，而不是切到 NVIDIA SDK：** compact irradiance/visibility encoding；randomly rotated ray sets + calibrated hysteresis；probe activity/priority/variability；terrain-edit dependency region；必要时才增加 camera-relative cascade。每个改动必须单独 A/B，不能一起打开后只看最终截图。

### 2.2 Surfel GI：可以替换 transport representation，不能单独替换 consumer field

**【事实】GIBS 通过 GBuffer 在可见表面生成并持久化 surfels；surfel 将昂贵 ray tracing 与逐像素 shading rate 解耦，使用 acceleration grid、ray guiding/binning、spatial filtering 与 temporal estimators。公开 2021 方案支持动态物体、skinned characters、transparency、arbitrary scale，且不要求预烘焙、专用 mesh 或 UV。** [EA SEED, *Global Illumination Based on Surfels*, SIGGRAPH 2021](https://www.ea.com/seed/news/siggraph21-global-illumination-surfels)

**【事实】2024 出货版本仍明确维护两套系统：surfels 为 deferred static surfaces 提供 indirect diffuse；probe clipmaps 给不走 deferred lighting 的 draw 查询，也给 surfel ray 和 surfel coverage 做 fallback。它甚至把 probe interpolation 改成类似 DDGI 的 octahedral irradiance + variance depth software interpolation。** [Frostbite GIBS 2024](https://advances.realtimerendering.com/s2024/content/EA-GIBS2/Apers_Advances-s2024_Shipping-Dynamic-GI.pdf)

**【事实】GIBS 的出货 RT acceleration geometry 不包含 vegetation，也不包含 alpha-tested geometry；演讲明确把“visible in raster but absent from ray-tracing representation”的 vegetation 列为限制。** [Frostbite GIBS 2024](https://advances.realtimerendering.com/s2024/content/EA-GIBS2/Apers_Advances-s2024_Shipping-Dynamic-GI.pdf)

**【推断】这恰好证明一种适合本项目的有限职责划分：opaque terrain 可以贡献 GI，flora/leaves 可以经 probe field 消费 GI；但不能据此宣称叶片也参与 GI 遮挡或反弹。** 后者若成为需求，必须另外评估 alpha-tested geometry 的 acceleration/update、能量与性能成本。

**【推断】纯 surfel 对本项目有三个结构性问题：**

1. Surfel 贴在 surface 上，flora vertex 或未来 object 的查询点常在空中；从 world-space neighborhood gather surfel 需要额外的 spatial index、visibility、normal support 与 hole fallback，成本和 seam 风险都高于八 probe 查询。
2. GBuffer-driven spawn 天生 view-dependent。Re: Flora 需要 off-screen terrain transport 能传播回屏幕；只依赖当前可见 surfel 会丢 transport history 或需要更复杂的 coverage/residency。
3. Raster flora/leaves 当前是 consumer 而非 occluder。若让大量动态叶片生成 surfel/TLAS geometry，更新和重叠成本会暴涨；若不生成，仍需能在任意动画位置查询的 volume cache。

**【结论】surfel 可以完整替换“DDGI 如何保存/更新可见表面 radiance”的部分，但不能在保持当前 consumer 契约时单独完整替换 DDGI。** 可以理论上写 `gatherSurfelIrradiance(position, normal)`，但强出货证据选择的是 surfel + probe，而不是支付每个 arbitrary consumer 的 surfel gather。

**【建议】若做实验，采用 GIBS 的真正形态：** terrain visible/hit surfaces 生成 stable surfels，半分辨率应用到 terrain；保留 DDGI 作为 flora/leaves/future objects 的 universal field，并允许 surfel 更新采样 DDGI fallback，或把 surfel 的低频结果注入 DDGI。不要先删除 DDGI。

### 2.3 Lumen 与两级 radiance cache

**【事实】Lumen 不是单算法。Surface Cache 用 mesh Cards 对 nearby surfaces 参数化并 amortize direct/indirect updates；Screen Probe Gather 从 pixels/probes 进行 final gather；World Space Radiance Cache 保存 distant lighting，也服务 translucency/volume。它可以 software trace SDF/global distance field，也可 hardware trace triangles。** [Epic Lumen technical details](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-technical-details-in-unreal-engine)、[Epic Lumen performance guide](https://dev.epicgames.com/documentation/unreal-engine/lumen-performance-guide-for-unreal-engine?lang=en-US)、[Lumen SIGGRAPH 2022 slides](https://advances.realtimerendering.com/s2022/SIGGRAPH2022-Advances-Lumen-Wright%20et%20al.pdf)

**【事实】Lumen 已在 Fortnite、Matrix Awakens 和默认 UE5 路线上证明生产价值。官方预算是 next-gen console 1080p internal 下 High 约 4 ms（60 fps），Epic 约 8 ms（30 fps），且依赖 temporal upsampling；全局照明变化可能需要数秒传播。Surface Cache 对复杂单 mesh、deforming skeletal mesh、foliage coverage 有明确内容/质量限制。** [Lumen overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/lumen-global-illumination-and-reflections-in-unreal-engine)、[Fortnite Chapter 4](https://www.unrealengine.com/tech-blog/lumen-brings-real-time-global-illumination-to-fortnite-battle-royale-chapter-4?lang=en-US)

**【事实】AMD GI-1.0 同样使用 screen-space probes + world-space hash cache 两级结构。其 Brixelizer GI SDK 是简化实现：对 sparse distance fields 发 screen-probe rays，previous-frame radiance cache 反馈近似多 bounce，再把 screen probes 投影为每 brick 的 L2 SH world-space irradiance cache，最后 resolve + denoise。** [GI-1.0 paper](https://gpuopen.com/download/GPUOpen2022_GI1_0.pdf)、[FidelityFX Brixelizer GI official documentation](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/brixelizer-gi/)、[Brixelizer sparse SDF documentation](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/brixelizer/)

**【推断】这些系统比 DDGI 更能保留 near-surface/high-frequency detail，但完整收益来自 surface representation、screen probes、world cache、history、reprojection、denoise 和 upsample 的组合。** Re: Flora 已经有 voxel traversal 和稳定的 raster consumer cache；照搬相当于再建一个 renderer subsystem，而不是替换一个 atlas。

**【建议】借鉴而不克隆：**最有价值的是“双层 cache”边界——camera-visible terrain 使用高频 screen/surface detail，world-space DDGI 继续作为 off-screen transport 和 arbitrary consumer 的低频 cache。只有 DDGI 生产优化仍达不到画质时才做这个第二层。

#### idTech 8 GHOST：已出货的 world radiance cache + irradiance volumes

**【事实】GHOST 已用于 Indiana Jones and the Great Circle 的早期迭代与 DOOM: The Dark Ages 的最新迭代；官方称两款项目都在全平台以 60 Hz 或更高实时运行。其每帧流程是 world visibility sampling → spatial-hash world radiance cache → irradiance volumes → final gather → denoise → upscale，而不是 NVIDIA SHaRC，也不是一次“path-tracing update”。** [id Software, *FAST AS HELL: idTech 8 Global Illumination*, SIGGRAPH 2025](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf)

**【事实】每个 ray hit 触发 spatial-hash cache 更新；系统每帧 shading 约 20k active cache entries 并复用数帧，shading 时采 previous-frame irradiance volumes 得到多次 bounce。随后它把结果写入类似 DDGI 的 octahedral probe atlas（RGB9E5 irradiance + RG16F visibility），覆盖六级 cascades 和最多 100 个 local volumes。Final gather 依次查询 screen-space cache、world radiance cache，最后 fallback 到 irradiance volumes；透明物体则共享 2-band SH froxel irradiance volume。** [idTech 8 GHOST 2025](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf)

**【事实】DOOM 出货 hotspot（大 vista、敌人、植被、粒子）中，整个 world sampling/cache/volume/final-gather/denoise/upscale 链的 serial cost 约 1.7–2.11 ms；console async cost 约 1.4–1.82 ms，其中 Xbox Series X 1440p 约 1.4 ms、PS5 1440p 约 1.55 ms。** [idTech 8 GHOST 2025 performance table](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf)

**【推断】GHOST 是本项目比“surfel-only”更直接的长期架构证据：昂贵 terrain-hit shading 可以由 world-radiance producer cache 摊销，而任意 consumer 仍读取 irradiance field。** 但这些数字依赖 HWRT、async compute、低分辨率 final gather、denoise/upscale 及 idTech 内容管线，只证明架构可出货，不能外推 Re: Flora 的毫秒收益。

### 2.4 Voxel cone tracing / SVOGI / SDFGI

**【事实】Voxel Cone Tracing 在 2011 年已展示以 voxel mip hierarchy 近似 cone integral，并在当时 GTX 480 达到 25–70 fps、支持 diffuse/glossy 与两次 bounce。CryEngine SVOGI 长期生产化：runtime voxelization + GPU voxel rays，Xbox One 常见配置约 3–4 ms，PC 约 2–3 ms；但官方同样列出 ghosting、aliasing、leak、noise、camera teleport catch-up、forward particle/water 与 procedural vegetation 限制。** [Crassin et al. 2011](https://research.nvidia.com/sites/default/files/pubs/2011-09_Interactive-Indirect-Illumination/GIVoxels-pg2011-authors.pdf)、[CryEngine SVOGI documentation](https://www.cryengine.com/docs/static/engines/cryengine-5/categories/23756816/pages/25535599)

**【事实】Godot SDFGI 以 camera-following cascades 支持任意世界尺寸和 procedural levels，但官方称其 semi-real-time、不能处理 dynamic occluders/emissive、是 Godot 中最昂贵的 GI 之一，并明确列出 low ray count blotches、frames-to-converge 与 cascade shift。** [Godot SDFGI documentation](https://docs.godotengine.org/en/latest/tutorials/3d/global_illumination/using_sdfgi.html)

**【推断】Re: Flora 的 terrain 本来就是 voxel，并不意味着 VCT 自动更合适。** 现有 occupancy/contree 是 intersection representation，不是可各向过滤 radiance 的 sparse mip volume。要 VCT，仍需解决 radiance injection、anisotropic filtering/mips、clipmaps、surface normal/opacity、edit propagation 和 raster consumer irradiance lookup；这些会与已有 DDGI visibility/radiance cache 重叠。

**【建议】不做完整 VCT 迁移。** 可把 voxel clipmap/SDF 作为未来 cheap ray accelerator 或 DDGI priority/invalidation helper；只有 release benchmark 证明当前 voxel march 是 probe update 的支配成本，才比较 Brixelizer-like sparse distance field traversal。

### 2.5 ReSTIR GI、ReSTIR PT 与 spatial hash radiance cache

**【事实】ReSTIR GI 对每像素 path 做 temporal/spatial reservoir resampling；论文在 1 spp/frame 下相对 path tracing 报告 9.3×–166× MSE 改善，但明确仍与 denoiser 结合得到 real-time quality。它是更有效的 stochastic path sampler，不是无噪 irradiance field。** [Ouyang et al., *ReSTIR GI*, 2021](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)

**【事实】NVIDIA SHaRC 是 world-space hashed outgoing-radiance cache，查询发生在 path tracing 的 geometry hit；RTXGI 2.0 的公开 sample 把 NRC 标为 currently experimental，并说明 SHaRC 也有已知限制。本文没有找到足以把 SHaRC 本身列为常见生产方案的一手出货证据。特别要避免把它与 DOOM: The Dark Ages 的 GHOST 混同：GHOST 是 idTech 8 自研的 world radiance cache + irradiance volume 混合架构。** [RTXGI 2.0 official source](https://github.com/NVIDIA-RTX/RTXGI)、[SHaRC official source](https://github.com/NVIDIA-RTX/SHARC)、[idTech 8 GHOST 2025](https://advances.realtimerendering.com/s2025/content/SOUSA_SIGGRAPH_2025_Final.pdf)

**【推断】它们解决的是“camera path 已经发出后，怎样复用重要的 indirect path/radiance”，不是“任何 flora vertex 不发 ray 就怎样得到稳定 irradiance”。** 若强行取代 DDGI，每个 raster consumer 需要发 secondary ray、共享一个 screen result，或另建 irradiance cache；第三个选择又回到 probe/world-cache hybrid。

**【建议】ReSTIR GI 只保留为未来高端 per-pixel detail/reflection 路线。** 它可以叠加到 DDGI 上，例如 primary secondary ray 用 DDGI/SHaRC terminate，但不应成为这次“快速、低噪、全 raster consumer”目标的主路线。

### 2.6 Neural radiance cache 与 Radiance Cascades

**【事实】NVIDIA NRC 在线训练 fully dynamic scene 的 radiance function；论文报告 Full HD cache update/query 约 2.6 ms，并显著降噪，但它仍嵌在 path tracer 中。AMD FSR Radiance Caching 当前官方状态是 technical preview，只支持 Windows/DX12 且面向 RDNA 4；文档明确讨论在线训练在 abrupt lighting/camera changes 下的 broad-frequency flicker 和 hyperparameter stability。** [Müller et al., *Real-time Neural Radiance Caching*, 2021](https://research.nvidia.com/publication/2021-06_real-time-neural-radiance-caching-path-tracing)、[AMD FSR Radiance Caching preview](https://gpuopen.com/manuals/fsr_sdk/techniques/radiance-cache/)

**【事实】Radiance Cascades 的 3D game-GI 文稿仍被作者站点标成 WIP；目前同行评审的一手论文面向 multidimensional non-LTE radiative transfer，公开 game demos 主要是 2D/community implementations。** [Radiance Cascades author/community hub](https://radiance-cascades.com/)、[Osborne & Sannikov 2025](https://doi.org/10.1093/rasti/rzae062)

**【建议】两者都不进入当前 production plan。** NRC 的跨平台/训练/调试门槛与本项目不匹配；Radiance Cascades 在 3D、动态 voxel edits、surface-material transport、arbitrary raster consumer 上缺少可核实出货证据。可以关注论文与实现成熟度，但不能用“现代”代替 production proof。

## 3. 推荐架构：producer cache + 世界空间 consumer field

```text
authoritative voxel terrain + material/sun/sky revisions
                         |
      optional terrain surface-voxel/world-radiance producer cache
     (cache repeated hit shading; revisioned, off-screen-capable)
                         |
             DDGI probe trace / transport
                         |
        published world-space irradiance field
                         |
 sampleDiffuseEnvironment(world_position, geometric_normal)
              /                              \
 direct terrain query               raster lighting caches
                                     flora/leaves/future objects

direct sun / VSM / leaf / cloud shadows remain separate
specular / reflections remain separate
```

**【事实】terrain shading 现在直接查询已发布 DDGI field；曾有的 receiver-side cache 已在
2026-08-16 移除。** 它与上图建议的 producer cache 本来就是不同 ownership。新 producer
cache 若成立，应由 geometry/material/sun/sky/source-field revisions 标识，复用 probe rays
命中 terrain 后的 radiance 计算，再把结果投影进现有 irradiance field；不能因为历史上有过
receiver cache 就沿用它的失效策略。

**【建议】provider 的语义保持稳定，内部实现可换。** 当前 shader ABI 可以暂时仍只返回 irradiance；host/domain 边界应把 `confidence`、`geometry_revision`、`radiance_revision` 作为内部 ownership 继续保留。任何新 backend 都必须证明：

- 同一点和 geometric normal 的 terrain/raster 查询在同一 published revision 上一致；
- 无可信数据时 fail closed 或用已定义的 Global Sky outside-volume policy，不能悄悄拿 screen history 代替；
- direct sun 不进入慢速 GI consumer field；
- flora/leaves 可消费 terrain bounce，但不因消费就自动成为 GI occluder；
- camera/off-screen 状态不改变 world-space 查询的基本可用性。

**【建议】若加 surfel 高频层，组合规则应是“职责分离”而非直接相加：** visible opaque terrain 的 final gather 可优先使用可信 surfel/screen detail；raster consumers 与无 surfel coverage 的 terrain 使用 DDGI。若要 blend，必须用 coverage/confidence 和频带分解避免双计能量，而不是 `surfel + DDGI` 两份完整 irradiance 相加。

## 4. 分阶段迁移计划

### Phase 0：冻结可比较基线，不改算法

**【建议】**在当前 commit 上新增/复用 release-only benchmark artifacts，记录 spacing 32 的 steady query、active update、完整 terrain edit rebuild、sun/sky revision、内存和画质。把当前 full deterministic DDGI 作为 reference backend；不要以旧 local-probe 数字替代当前测量。

退出条件：下面验收矩阵所有 baseline artifacts 齐全；日志能分开 trace/filter/query/cache/publication；同场景重复运行有可接受方差。

### Phase 1：先做低风险 DDGI productionization

每项独立红/绿、release A/B、独立提交：

1. **compact storage experiment**：优先测试 `RGBA16F` irradiance 与适合 visibility moments 的格式；保留 capture/reference full precision path。
2. **raw variability + activity experiment**：以当前 64 rotated rays + history 为基线，分别测 pre-blend variance、per-probe sleep/priority 与更小 active ray budget。目标是减少全场 update cost，而不是用更长 history 隐藏 noise。
3. **activity/priority/variability**：newly dirty/near edits/high variance probes 优先；stable/off-surface probes sleep。参考 2021 production DDGI，而非自行发明 camera-only heuristic。
4. **dependency-bounded terrain refresh**：先从 changed voxel bounds + probe cage/support + conservative propagation halo 开始；保持 latest-revision-wins、active/staging 与 atomic publication。
5. **只有世界扩大后才做 clipmap/cascades**：当前有限 512³ world 不需要先承担 scrolling/residency 复杂度。

Phase 1 成功标准：在 sealed/portal/walls/donor/dogleg 与 runtime edit acceptance 全绿的前提下，update GPU p95 和 edit-to-ready 明显下降；steady consumer query 不退化；静态镜头无新 noise，移动/编辑后的响应不慢于 baseline。

### Phase 2：terrain radiance producer cache + DDGI 的 bounded prototype

**【建议】**先做比完整 GIBS 更窄的实验：以 terrain surface voxel 或 probe-hit quantized position 为 key，缓存 stable albedo、direct sun/sky、sun visibility 和 previous-field bounce 所形成的 hit radiance。Probe ray 命中 terrain 后先查这个 producer cache，再投影进现有 DDGI atlas；只覆盖 opaque terrain，不让 flora/leaves 进入 producer，DDGI 始终是 arbitrary consumer 与 off-screen delivery field。这个方案直接借鉴 GHOST 的 producer/delivery 分层，同时能复用项目已有 voxel geometry 与 deterministic transport。

它的首个目标是证明“避免重复 hit shading/精确太阳遮挡/previous-field query”是否能释放可观预算，并保持 baseline 输出 bit-identical 或在预先冻结的 tight tolerance 内；不是先追求更高频图像细节。这个 cache 发生在 probe ray 已找到 hit 之后，**不会省掉主 voxel ray march**；若 Phase 0 证明 traversal 才是绝对瓶颈，就应跳过本实验。若 producer cache 无显著收益，也应停止，不扩大到 surfel residency/gather。

只有第一步成立且 terrain 仍缺 contact/high-frequency indirect detail，才扩展为 GIBS 式 stable surfels：从 terrain primary hit/GBuffer 或 voxel surface publication 生成 surfels，以 half/quarter-rate 应用到 visible opaque terrain；probe field 继续负责 flora、叶片、future objects、off-screen transport 和 coverage fallback。

需要分别回答：

- producer lookup 的命中率，以及它实际省掉的 hit shading、exact sun shadow 与 previous-field query 成本；主 voxel traversal 必须单列，不能算作 cache 收益；
- baseline 与 producer-cache DDGI 的逐 probe/最终图像误差，以及 sun/sky/terrain edits 后的失效正确性；
- 若扩展 surfel，terrain 的 contact GI/indirect shadow 是否显著优于 DDGI；
- visible/off-screen/camera cut 时 coverage 是否稳定；
- surfel spawn/recycle/grid/gather 是否在本项目软件 voxel traversal 和 Vulkan/Metal 目标上成立；
- 怎样避免与 DDGI 双计；
- 是否必须引入 TLAS/HWRT，还是能复用 voxel ray marcher。

停止条件：若 producer cache 没有超出 run-to-run variance 的 active-update 收益，或 surfel terrain image 的盲测/crop metric 没有 material benefit，或新增 steady cost、内存、edit latency 超过 Phase 1 的收益，就终止，不继续复刻 idTech/Frostbite infrastructure。

### Phase 3：只有明确画质缺口才增加 screen/world 两级 cache

**【建议】**如果 surfel 不合适但 DDGI 高频 detail 仍不足，再比较一个 Brixelizer/Lumen-inspired screen probe layer。World DDGI 继续是 off-screen transport 和 raster consumer cache；screen layer 只修复 camera-visible terrain 的近场 detail。不要同时实现 surface cache、SDF clipmap、screen probes、radiance cache、denoiser和reflection。

### 暂不排期

- ReSTIR GI/PT、SHaRC/NRC path backend；
- neural training/inference；
- 3D Radiance Cascades；
- diffuse GI 与 specular/reflection 的统一重写。

## 5. Release-mode 性能与画质验收矩阵

**【事实】按仓库政策，性能结论只能来自 release hidden app run；debug/unit tests 不是性能证据。** 标准命令基础为：

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

**【建议】**每个候选 backend 固定同一 commit、相同分辨率/present mode、相同 camera/terrain/sun、相同 warmup/sample window；至少 5 次 run，报告 median、p95、p99 与最差 run，而不只报最好的一次。

| 场景 | 必采性能信号 | 必采质量/正确性信号 | 候选相对 baseline 的 gate |
|---|---|---|---|
| 静态日景、无编辑 | `frame.render`、terrain query/cache、flora cache、GI update median/p95、VRAM | 固定镜头 luma delta、reference RMSE/SSIM、terrain/raster irradiance parity | frame p95 不退化；静态 noise 不高于 DDGI；画质不得靠长 history 才稳定 |
| windy flora/leaves | flora/leaf cache 与 draw cost；GI update overlap | 同位置/normal 的 terrain-vs-raster query；叶片响应 latency | raster consumer 不能因新 backend 发 per-vertex rays；不得新增 GI ghost trail |
| camera flythrough/cut | per-frame update、spawn/recycle、cache miss、p99 spike | disocclusion holes、screen/off-screen lighting discontinuity、catch-up frames | p99 spike 和 catch-up 不差于 baseline；world query 不因 camera cut 丢失 |
| sun/sky abrupt change | update GPU、queued/coalesced work、90% response time | old/new revision isolation、luminance response curve、flicker | direct sun 下一帧响应；GI 延迟不差于 baseline，且无跨 revision 混合 |
| 单次 terrain edit | voxel visibility、trace/filter/publication、edit-to-first-valid、edit-to-converged | stale-light strict gate、latest revision、e0 first publish、portal/wall leak | correctness 全保留；edit-to-valid 至少不退化，优化方案应证明显著缩短 |
| 连续 brush edits | queue depth、obsolete work、GPU p99、CPU scheduling | latest-wins、无旧 field 回写、无长时间黑场 | 无 unbounded queue；active field continuity 与当前 lifecycle 契约一致 |
| sealed / thin wall / portal | trace/query cost | sealed 最大线性亮度不超过 `1e-5`（dense 当前为 exact zero）；moment-vs-exact p99；halo/leak crop | 现有 committed thresholds 不得放宽 |
| donor / dogleg | 每个完整 update epoch 与总睡眠时间 | e0 signal、multi-epoch dogleg、能量有限非负 | 现有 lifecycle gates 与 128-epoch sample budget 不得静默放宽 |
| raster-only future object fixture | object cache/query cost | 在任意空中 position + normal 取得当前 terrain bounce；revision parity | 必须无需 GBuffer-visible 或 per-object ray 才能消费 GI |
| memory stress：spacing 32/16 | atlas/cache/TLAS/SDF/surfel/history 总 bytes、allocation peak | 同质量配置 | 报总系统内存而非单 cache；Phase 1 默认不得用额外大常驻表示换取小时间收益 |

除现有 correctness thresholds 外，性能 gate 应先用 Phase 0 的当前实测定义，不先拍脑袋写绝对毫秒。最终推荐至少要求：

- **steady frame p95 非劣化**；
- **GI active-update p95 有可重复的实质下降**，幅度大于 run-to-run 方差；
- **terrain edit 到可信 field 的 wall-clock 不增加**；
- **静态 noise、运动 ghost、光照变化 latency 三者不能互相偷换**；
- **总 GPU memory 和瞬时 peak 单独报告**；
- **最低目标设备通过**，不能用高端 discrete GPU 的结果替代 Apple/Vulkan/Metal 实际目标。

## 6. 风险与明确不确定项

- **【事实】当前分支缺少 multi-bounce DDGI 完成后的匹配 release steady/update breakdown。** 历史 6.146/4.339 ms 数字只能作架构上下文；Phase 0 必须重测。
- **【不确定】当前目标 GPU、分辨率和总 GI budget 未在本任务中冻结。** 因此外部 1–4 ms 数据只证明生产可行性，不能直接预测 Re: Flora；尤其 Frostbite 是 console HWRT，Re: Flora 是自有 voxel traversal。
- **【不确定】surfel 用软件 voxel marcher 更新是否足够快。** GIBS 出货结果依赖 TLAS、硬件 RT、ray limiting、shadow-map reuse、offline BLAS/rebraiding 等整套基础设施，不能只移植其 surfel data structure 就期待 2.5 ms。
- **【事实】Lumen/GI-1.0、ReSTIR/SHaRC 与 neural cache 都通过不同程度的 temporal reuse、reprojection、denoising 或 upsampling 换取效率。** “低噪”不等于“没有时间历史”；必须把 noise、ghost、lighting latency 分开验收。
- **【推断】DDGI 的可见 grid/seam 问题不能只靠更密 spacing。** 本仓库 spacing 32/16 已有 relocation/spatial weighting 故障史；优化必须继续对 exact visibility 与 saved-terrain seam fixtures 负责。

## 7. 最终决策表

| 决策 | 结论 |
|---|---|
| surfel 能否完整替换 DDGI？ | **不能按本项目要求单独替换。** 可以替换/增强 surface transport cache，但任意 world-position raster consumer 仍需要 probe/irradiance volume 或更昂贵 surfel gather；出货 GIBS 本身选择了 probe clipmap。 |
| 现在最值得生产投入的方案 | **现代化当前 DDGI**：compact formats → adaptive ray/history → activity/priority → bounded invalidation，逐项 release A/B。 |
| 画质进一步提升的第二条路 | **surfel terrain detail + DDGI universal field**，限定 prototype，不先做 flora occluder 或全 renderer 重写。 |
| Lumen 是否“更好”？ | 它是更完整、更高频的多 cache renderer，且大规模出货；但不是比 DDGI 更小的替换模块，移植成本与当前需求不成比例。 |
| ReSTIR GI 是否解决过去 path tracing 的 noise/speed？ | 相对 naïve PT 显著降 variance，但仍是 screen/path stochastic solution、仍依赖 denoiser/history，且不自动提供 arbitrary raster irradiance query。不是当前首选。 |
| Voxel terrain 是否应直接选 VCT/SDFGI？ | 不应仅因“都是 voxel”而选。需再建 radiance/SDF/mip/clipmap representation，已有生产方案也有 leak/ghost/forward consumer 限制。 |
| Radiance Cascades / neural cache | 前者缺 3D game production proof，后者目前硬件/训练/preview 限制强；持续观察。 |

**【建议】下一项实际工作不是再选一种 GI 名字，而是实施 Phase 0 measurement，然后按 Phase 1 顺序逐项优化。** 如果 modernized DDGI 在相同 correctness 下已经进入预算，项目就没有理由承担 GIBS/Lumen 级重写；如果仍超预算或画质不足，Phase 0 的 breakdown 会告诉我们真正需要的是更便宜的 rays、局部更新，还是 surface-frequency detail，而不是凭视频评论更换架构。
