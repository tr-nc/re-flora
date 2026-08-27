# Glass voxel：光栅 + compute/software ray tracing 技术调研

状态：技术选型与实施指导；不包含生产实现

日期：2026-08-27

Re: Flora 审计基线：`400794afca0b8d7dd3fbf44e2804a53354cca29f`

旧报告基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`

## 决策摘要

已明确采用以下路线：

- **不采用** Vulkan/DXR hardware RT、BLAS/TLAS、ray query；
- 保留当前 raster primary/opaque 内容与 compute voxel tracer；
- 为玻璃增加专用的 software voxel/grid traversal；
- 先完成 opaque scene，再执行独立的 scene-linear HDR glass resolve；
- raster 内容以 screen-space opaque scene 为第一层近似，缺失内容回退到可查询的 voxel/sky，且不把这个近似宣称为完整 secondary visibility；
- DDGI 继续做低频 diffuse transport，不承担主视图厚玻璃的镜面折射，也不把折射后的距离错误写入原方向的 visibility moments。

这条路线与当前项目最吻合。Re: Flora 已有自定义 Contree first-hit、权威 dense voxel atlas、compute tracer、raster/compute composition、DDGI probe trace 和 release benchmark 框架。它缺的不是一套新的通用 acceleration structure，而是两个清晰的深 seam：

1. `VoxelMaterial`：集中声明 opaque / dielectric、collision、probe relocation、DDGI visibility、direct-shadow 等语义；
2. `SceneQuery`：把 Contree 的快速 first surface 与 dense atlas 的精确 material-transition DDA 组合起来，向 primary、reflection、refraction、shadow 和 DDGI 暴露不同策略。

首个 production slice 应只承诺：空气 + 一种平滑均匀玻璃、一体素或 face-connected 连续玻璃、精确 entry/exit、Snell、介电 Fresnel、TIR、Beer-Lambert，以及有界且确定性的 secondary path。粗糙玻璃、色散、specular caustics、任意嵌套介质和完整 raster secondary visibility 都应留在后续阶段。

## 当前 `main` 相对旧报告发生了什么

旧报告仍正确的核心判断是：Contree 是表面壳，`marchScene()` 只返回 first hit；单层 raster color/depth 无法恢复玻璃后的 raster 内容；厚玻璃不是 alpha blending；推荐 opaque-first + glass resolve。

但 `main` 已经改变了若干实施前提。

| 领域 | 当前事实 | 对 Glass 设计的影响 |
| --- | --- | --- |
| voxel ID | [`VOXEL_TYPE_EMISSIVE = 8`](../shader/slang/voxel_types.slang#L10)，CPU 镜像同样使用 ID 8；edit stats 已扩到 9 | **Glass 必须使用 ID 9**；统计容量至少扩到 10，长期建议覆盖完整 16-ID 编码域 |
| persistence | [`TERRAIN_VOXEL_SCHEMA_ID` 仍为 1](../src/terrain_persistence.rs#L7)，代码把“占用新的 nibble 值”视为 layout-compatible | 新程序能 round-trip 新 ID，但旧 binary 仍会接受 schema 1 并误解未知材质。Glass 上线前应重新决定是否升级到 schema 2；不能沿用旧报告的“ID 8 + schema 2”原文 |
| emissive/local lights | 已有 `ProviderId + SourceLightKey -> LightId` registry、voxel/raster provider、稳定 small-N 选择；GPU point-light capacity 为 [`8`](../src/lighting/mod.rs#L20) | Glass 不能混入 emissive collector；local-light visibility 从 binary hit 升级为 RGB transmittance 时应复用统一 `SceneQuery`，不能为每个 consumer 写一套 glass skip |
| DDGI transport | probe ray 已在 opaque hit 上计算 direct sun、最多 8 个 local point lights、emissive surface radiance 与历史 indirect；每 probe 64 rays，每帧总预算 32,768 | Glass policy 会同时影响 radiance、signed hit distance、exact voxel visibility、relocation、local-light shadow 与 revision/temporal policy，必须成组修改 |
| DDGI revisions | geometry 与 radiance revision 已分离，并有局部恢复、immutable lighting snapshot、convergence/publication gates | occupancy / visibility-policy 改动走 geometry revision；IOR、absorption、tint 等只改变 transport 的参数原则上走 radiance/material revision，避免无谓重建 relocation |
| direct shadow | terrain VSM、leaf opacity、cloud transmittance 已分源；terrain receiver 已按 voxel 固定，但 terrain shadow map 仍是一个 nearest scalar depth | Glass 不能直接写现有 nearest depth，否则成为黑色 opaque caster；带色透射需要独立 RGB optical-transmittance 路径或 receiver-side software segment query |
| terrarium prototype | 新增 analytic panel/glass modules、glass mesh/pipeline 与 SSR/refraction prototype | 它们提供可复用的 screen-space validation、edge fallback 和 analytic box tracing 思路，但当前 composition 没有调用 `applyTerrariumGlass()` / `applyTerrariumPanelsToScene()`，且 raster path 固定 [`enable_glass = false`](../src/tracer/mod.rs#L4278)，所以仍不是可达的 production Glass feature |
| render targets | voxel color 为 RGBE `R32_UINT`，raster color 仍是 [`R8G8B8A8_UNORM`](../src/tracer/extent_dependent_resources.rs#L202)，composition 后才进入 `RGBA16F` | opaque-first glass resolve 应保留 scene-linear HDR；如果 raster highlight 要参与玻璃反射/折射，应评估把 opaque raster target 升到 `RGBA16F` |

当前几何事实没有根本变化：

- [`surface_extraction.slang`](../shader/slang/surface_extraction.slang#L146) 仍把任何 non-empty voxel 当 solid，并删除六邻域都 non-empty 的内部 cell；
- [`MarchingResult`](../shader/slang/marching_result.slang#L3) 没有 medium、transition、face normal 或 exit；
- [`marchScene()`](../shader/slang/scene_marching.slang#L44) 仍在第一个 Contree surface hit 返回；
- [`combineSceneColors()`](../shader/slang/composition_scene.slang#L16) 仍只比较一层 raster depth 与一层 compute depth；
- [`ddgi_probe_trace.slang`](../shader/slang/ddgi_probe_trace.slang#L203) 仍把 first hit 直接解释为 opaque radiance surface；
- [`ddgi_voxel_visibility_pack.slang`](../shader/slang/ddgi_voxel_visibility_pack.slang#L27) 仍把所有 non-empty voxel 打成 binary occupied bit；
- [`ddgi_probe_relocate.slang`](../shader/slang/ddgi_probe_relocate.slang#L28) 仍只把 Empty 当 probe 可用空间；
- [`local_lighting.slang`](../shader/slang/local_lighting.slang#L177) 仍用 first hit 做 binary local-light segment visibility。

因此，新增 Glass 不能是 `if (type == Glass) alphaBlend()`；它会横跨 material semantics、surface extraction、scene query、composition、direct/local shadows、DDGI 与 persistence。

## 一手资料给出的 production 经验

### Unreal Engine Lumen：screen trace 先行，world-space software fallback 兜底

Epic 的 Lumen 技术文档明确说明：Lumen 先做 Screen Traces，失败或射线走到表面背后时再用更可靠的 tracing representation；software mode 使用 mesh distance fields，并合并成 Global Distance Field。它还将 Detail Tracing 与 Global Tracing作为质量/成本档位，并明确列出薄几何、材质表示和动态变形等限制（[Epic, Lumen Technical Details](https://dev.epicgames.com/documentation/unreal-engine/lumen-technical-details-in-unreal-engine?lang=en-US)）。官方性能指南把 Global Distance Field 的缓存、probe update budget、screen-probe spacing/resolution 和 GPU profiling 作为生产控制项（[Epic, Lumen Performance Guide](https://dev.epicgames.com/documentation/unreal-engine/lumen-performance-guide-for-unreal-engine?lang=en-US)）。

对 Re: Flora 可迁移的原则：

- 便宜且能覆盖 raster-only geometry 的 screen-space 查询先行；
- query miss、屏幕边缘、被遮挡或 representation mismatch 必须有 world-space fallback；
- software representation 的更新、覆盖范围、质量档位和 profiler scope 是产品能力的一部分，不是 shader 私有细节；
- screen trace 与 fallback 的结果要能单独 debug，避免把 view-dependent artifact 当成材料问题。

不能从 Lumen 推导的结论：

- Epic 文档没有证明 Lumen software RT 对厚介电玻璃执行 Re: Flora 所需的 voxel entry/exit、Beer distance 或 DDGI glass transport；
- 文档反而说明 software tracing 对 material/geometry 有限制，translucent material 以 Lumen card 进入部分流程，且极薄 feature 可能无法被 distance field 表示；
- 因此 Lumen 是 hybrid fallback 与 production instrumentation 的证据，不是本项目 Glass 算法的直接实现证据。

### Godot SDFGI：软件 SDF + cascades + probes 可以 productionize，但默认是 opaque visibility

Godot 4 的官方 SDFGI 文档说明：SDFGI 跟随相机、支持动态 lights，但不支持 dynamic occluders / dynamic emissive surfaces；质量/性能由 cascades、cell size、probe ray count、frames-to-converge、light update interval 和 half-resolution 控制（[Godot 4.4 SDFGI](https://docs.godotengine.org/en/4.4/tutorials/3d/global_illumination/using_sdfgi.html)）。官方文档还明确区分 screen-space SSIL 与可覆盖 off-screen 元素的 SDFGI（[Godot, Environment and post-processing](https://docs.godotengine.org/en/stable/tutorials/3d/environment_and_post_processing.html)）。

开源 shader 进一步显示其工程形态：

- [`sdfgi_integrate.glsl`](https://github.com/godotengine/godot/blob/36a2dd0272a6fc097642acea6287242ed201479f/servers/rendering/renderer_rd/shaders/environment/sdfgi_integrate.glsl) 从 probe 发 ray，在 cascaded SDF 中前进，first hit 后取 voxel light，并写入 history / probe textures；
- [`sdfgi_direct_light.glsl`](https://github.com/godotengine/godot/blob/36a2dd0272a6fc097642acea6287242ed201479f/servers/rendering/renderer_rd/shaders/environment/sdfgi_direct_light.glsl) 用 cascaded SDF 做 direct-light visibility，并把 probe occlusion、feedback 与动态 lights 合入 voxel lighting。

对 Re: Flora 可迁移的原则：

- software grid/SDF traversal、camera-local cascades、temporal convergence 与分档更新是真实引擎路径；
- direct light visibility 与 indirect probe integration 可以共享同一 software representation，但仍需不同输出语义；
- probe ray count、更新帧数、cascade count 与 half resolution 都必须成为可测试的质量档，而不是隐藏常量。

不能从 Godot 推导的结论：

- 上述 shader 的 SDF 命中仍是 opaque collision；没有 thick dielectric entry/exit、TIR 或 Beer-Lambert；
- Godot 的 production existence 证明“software RT + raster + probe GI”可行，不证明“透明材质自动被 SDFGI 正确处理”。

### RTXGI / DDGI：SDK 规定 probe 数据语义，不替应用决定玻璃 transport

2019 DDGI 论文定义了每 probe 的低分辨率 irradiance 与 directional hit-distance moments，并用 visibility-aware eight-probe interpolation 抑制 leaks（[Majercik et al. 2019](https://jcgt.org/published/0008/02/01/)）。2021 production 论文加入了 self-shadow bias、probe relocation、probe states、cascades、temporal response 和 variability，且说明这些扩展进入 NVIDIA RTXGI、Unity、Unreal Engine 4 及商业引擎（[Majercik et al. 2021](https://jcgt.org/published/0010/02/01/)）。

RTXGI v1 的官方文档规定：

- relocation 用 backface hit ratio 判断 probe 是否在几何内部，offset 被限制在 grid cell 的 45%；
- classification 复用固定 ray 的 signed distances；
- irradiance 与 distance blending 消费应用写入的 ray radiance / hit distance；
- 低频 variability 可能永远不为零，应用负责决定何时暂停与恢复 probe update；
- 墙体厚度必须与 probe density 匹配，否则会漏光（[RTXGI-DDGI, DDGIVolume](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/DDGIVolume.md)）。

官方 test harness 的 [`ProbeTraceRGS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl) 将 front-face hit 的 radiance 与 hit distance 一起写入 ray data，backface 则写负且缩短的 distance；[`AHS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/AHS.hlsl) 只对 alpha-mask cutoff 调用 `IgnoreHit()`。该 sample 使用 hardware TLAS，且没有提供 thick transmissive glass policy。

这意味着：

- RTXGI 的 moments / relocation / classification 是可采用的表示原则；
- Re: Flora 可以继续用自己的 Contree + atlas software trace 来生产相同概念的 ray record；
- **不能**引用 RTXGI 作为“DDGI 已经替我们解决玻璃”的证据；glass ray radiance 与 distance 的一致性是应用自己的责任。

### 物理 dielectric：surface transmission 与 volume attenuation 必须分开

PBRT 给出 Snell、精确 dielectric Fresnel、TIR 与 smooth dielectric sampling；折射解不存在时就是 TIR（[PBRT, Specular Reflection and Transmission](https://www.pbr-book.org/4ed/Reflection_Models/Specular_Reflection_and_Transmission)、[PBRT, Dielectric BSDF](https://www.pbr-book.org/4ed/Reflection_Models/Dielectric_BSDF)）。Walter 等人的原始论文将 microfacet theory 扩展到 rough-surface transmission，并强调 transmission 至少跨越两个界面（[Walter et al. 2007](https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.pdf)）。

Khronos 的 ratified `KHR_materials_transmission` 明确区分 transmission 与 alpha-as-coverage；alpha 描述表面是否存在，不能表达光进入材质（[KHR_materials_transmission](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_transmission/README.md)）。`KHR_materials_volume` 则要求 closed/manifold boundary、IOR、world-space attenuation distance，并给出 Beer 定律：

```text
sigma_a = -log(attenuation_color) / attenuation_distance
T(distance) = exp(-sigma_a * distance)
            = attenuation_color ^ (distance / attenuation_distance)
```

该规范明确指出：raster thickness texture 只是有损估计，ray tracer 应使用实际 ray-traced path length（[KHR_materials_volume](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_volume/README.md)）。

因此 Re: Flora 的一个 voxel 厚玻璃仍是 volume，不是 thin sheet。斜入射距离可能大于一个 voxel edge；face-connected 同材质 cells 是一个连续 medium，内部 cell face 不产生 Fresnel event。

## 推荐的 renderer architecture

### Pass 顺序

```text
1. voxel primary query
   - 返回 first opaque voxel shading/depth
   - 另写 nearest GlassFront / glass event
   - Glass 不作为 opaque depth 终点

2. raster opaque pass
   - depth prefill 使用 first opaque voxel depth
   - flora / opaque objects 不会因 Glass front 被提前剔除

3. opaque composition
   - voxel + raster + sky/cloud -> UnifiedOpaqueHDR
   - 保留 nearest opaque depth 与 depth provenance

4. glass resolve (compute/fullscreen)
   - 读取 UnifiedOpaqueHDR、opaque depth、GlassFront
   - voxel reflection/refraction 使用 SceneQuery world-space software trace
   - raster 内容使用 validated screen-space sample
   - screen miss / disocclusion / off-screen 回退到 voxel hit 或 sky/environment
   - 写另一个 RGBA16F target，禁止同一 image 读写反馈

5. 明确受支持的 raster translucency pass
6. tone map / dither / final output
```

opaque-first 是这个方案的核心。如果 compute primary 把 Glass front 写进 terrain depth，现有 depth prefill 会在 raster pass 之前剔除玻璃后的 flora/object，后面的 screen-space refraction 无法恢复被丢弃的颜色。

### `VoxelMaterial` seam

建议把 encoded ID 与各子系统策略集中为一份权威声明，并生成/校验 Rust 与 Slang mirror：

```text
VoxelMaterial
  id
  surface_class: Empty | Opaque | Dielectric
  collision: bool
  terrain_support: policy
  probe_relocation_solid: bool
  blocks_ddgi_visibility: bool
  direct_shadow_policy
  local_shadow_policy
  optical: { ior, sigma_a_rgb, roughness, priority }
  soil / inventory / acoustic / particle semantics
```

建议 Glass = 9。ID 1 保留历史空洞；ID 8 已是 Emissive。Glass 的 moisture/fertility bits 应规范为 0，IOR、absorption 与 roughness 放 material table，不占 per-voxel soil-state bits。

### `SceneQuery` seam

```text
trace_first_surface(ray, mask)
  -> SurfaceHit | Miss

walk_voxel_media(ray, starting_medium, event_budget)
  -> ordered InterfaceEvent / MediumSegment / OpaqueHit / Miss

trace_radiance(ray, path_budget, fallback_policy)
  -> RadianceResult + diagnostics

trace_segment_transmittance(origin, target, policy)
  -> RGB transmittance + termination reason
```

实现策略：

1. 用现有 Contree 找第一个 surface/chunk entry，保留它对 sparse visible surfaces 的优势；
2. 一旦进入 Glass，用权威 dense atlas 的 Amanatides–Woo style DDA 沿 cell boundaries 前进；原始算法每跨一个 voxel 只需比较 next-boundary distances 并增量更新（[Amanatides & Woo 1987](https://doi.org/10.2312/egtp.19871000)）；
3. DDA 返回 axis-aligned geometric face normal、from/to material、world-space segment length 与 exact cell coordinate；
4. Glass -> Glass 只累计 distance；Glass -> Air/Opaque 才产生 interface event；
5. exit 后重新进入 Contree first-surface query，或在当前 chunk/domain 内继续 dense walk；策略隐藏在 `SceneQuery` 内；
6. camera 在 Glass 内时从 atlas 初始化 current medium，不靠 normal 猜 entering/exiting。

用于 Snell/TIR 的 normal 必须是 DDA geometric face normal。当前 smooth occupancy normal 可以继续给 opaque diffuse shading 使用，但不能决定 medium topology。

## dielectric path policy

### 界面与介质

每个 Air <-> Glass transition：

1. 按 geometric normal 定向 incident/outgoing medium；
2. 用 `eta_i / eta_t` 计算 Snell refraction；
3. 计算精确 unpolarized dielectric Fresnel `F`；
4. 无折射解时进入 TIR，`F = 1`；
5. 介质段乘 `exp(-sigma_a * distance_world)`；
6. Glass -> Glass 不创建界面；Glass -> Opaque 结束 transmitted branch。

第一版 material domain：Air + 单一均匀 Glass。未来出现水、不同 IOR 玻璃或 overlapping mesh volumes 时，再引入 medium stack / priority；不要先设计一个没有真实 adapter 的通用嵌套系统。

### 有界 branching

不能在每个界面无界地同时递归 reflection/refraction。建议 ship mode 使用确定性的 top-K work queue：

- `max_active_paths = 4`；
- `max_interface_events = 8`；
- `max_scene_queries = 8`；
- `throughput_cutoff = 1e-3`（最终值以 reference/error test 决定）；
- 每次 split 后按 RGB luminance throughput 排序，只继续最重要的 K 条 path；稳定 tie-break，不使用 frame-random branch；
- TIR path 优先继续；超过 budget 时写 counter，并用 environment/opaque fallback 终止；
- reference mode 用 CPU double precision 或离线 stochastic sampling 验证能量误差，不进入普通 `cargo test` 的 GPU/长时路径。

这些数字是首轮工程上限，不是已测最优值。验收必须统计 budget exhaustion；如果 authored 一体素场景频繁触顶，应调整算法或内容约束，不能静默裁掉。

## screen-space raster fallback 与 visibility 边界

Lumen 和 Unity HDRP 都体现了“screen-space first、representation fallback”的 production pattern。Unity HDRP 官方文档说明 screen-space refraction 使用当前 frame 的 depth/color buffer，并在 screen edge 淡出；其 refraction hierarchy 还会回退到 reflection probe data（[Unity HDRP, Screen Space Refraction](https://docs.unity3d.com/cn/Packages/com.unity.render-pipelines.high-definition%4010.4/manual/Override-Screen-Space-Refraction.html)、[Unity HDRP, Refraction](https://docs.unity3d.com/ja/Packages/com.unity.render-pipelines.high-definition%4010.5/manual/Refraction-in-HDRP.html)）。

Re: Flora 的 raster sample 必须满足：

- projected UV 在 viewport 内；
- sampled opaque depth 位于 Glass exit 之后；
- depth reprojection / thickness tolerance 通过；
- sample provenance 允许被折射（opaque raster 或已合成 opaque voxel）；
- 失败时不拉伸边缘 texel，而是回退到 world-space voxel hit / sky / probe approximation；
- debug view 区分 `screen-hit`、`voxel-fallback`、`sky-fallback`、`rejected-depth`。

当前方案能看见离屏/被遮挡的 **voxel terrain**，因为它有 world-space query；不能看见未进入 voxel structure 的离屏 flora/objects。这个边界应出现在产品说明和 screenshot gate 中。

## transparency sorting 不等于 secondary visibility

Khronos Vulkan PPLL OIT sample 的 gather pass 只保存当前 pixel 上 rasterized fragment 的 color/depth，combine pass 再排序；为了性能还把准确排序上限设为每 pixel 16 fragments（[Khronos Vulkan OIT sample](https://docs.vulkan.org/samples/latest/samples/api/oit_linked_lists/README.html)）。`KHR_materials_transmission` 也明确说 transparent primitive ordering 很难，但没有规定一种算法。

因此要把两个问题拆开：

| 问题 | 需要什么 | OIT / sorting 能否解决 |
| --- | --- | --- |
| 同一 primary pixel 上多个 raster translucent fragments 的顺序 | sorted forward、depth peeling、WBOIT 或 PPLL | 能，精度/内存/层数各有代价 |
| 折射/反射 ray 命中的离屏或 primary-view 被遮挡 geometry | world-space scene representation + secondary query | 不能；OIT 只有当前 view fragments |
| voxel Glass 的 entry/exit 与 thickness | material-transition DDA | 不能；alpha fragment 没有连续 medium |

首版建议明确限制 raster translucency：只保证 opaque raster geometry 可透过 Glass 看见；droplet/particle 在 Glass 前后的多层正确折射不进入首版。若未来要求“玻璃前后多层水滴都正确”，这是独立的 raster transparency project，不能以 Glass DDA 已完成为由宣布支持。

## DDGI policy

### 为什么 probe ray 首版不应真正折射

DDGI 的一条 ray record 同时携带：

- 该原始 probe direction 上的 radiance sample；
- 该方向的 signed hit distance，之后被过滤为 first/second moments 并用于 receiver visibility。

如果 ray 在 Glass 中改变方向，再把弯折路径末端的 hit distance 写给原始方向，moment visibility 会把一段折线路径误当成原方向上的遮挡距离。这不是 2019/2021 论文或 RTXGI SDK 提供的 glass contract。

因此下面是**基于 DDGI 表示约束的 Re: Flora 工程推断**：首版 DDGI 对 Glass 做低频、同方向 transmission approximation，不做 Snell bending。

### 推荐 probe trace

```text
direction = original probe direction
throughput = 1

沿 direction 做 material-transition DDA：
  Air -> Glass:
    throughput *= (1 - Fresnel(direction, geometric_normal))
  Glass segment:
    throughput *= Beer(sigma_a, segment_distance)
  Glass -> Air:
    throughput *= (1 - Fresnel(direction, -geometric_normal))
  Opaque hit:
    radiance = throughput * shade_opaque_hit()
    distance = original straight-ray distance to opaque hit
  Miss:
    radiance = throughput * sky(direction)
    distance = far
```

这里忽略折射位移、caustics 与玻璃反射返回的间接能量，是有意的低频近似。它的优点是 radiance 与 visibility distance 仍共享同一原始 direction；比“弯折 radiance + 原方向 moments”更一致。

### visibility / relocation / classification

| 子系统 | Glass 首版策略 | 原因 |
| --- | --- | --- |
| probe radiance trace | 同方向穿透，累计 Fresnel/Beer | 让窗后 diffuse transport 存在，同时保持 direction/distance 一致 |
| moment hit distance | 记录首个 opaque hit 或 far，不记录 Glass front | 防止 Glass 被当作完全 opaque wall |
| exact voxel visibility bits | Glass 不设置 `blocks_ddgi_visibility` bit | 当前 exact segment visibility 否则会与 probe trace 打架 |
| empty-block acceleration | 从 semantic visibility bits 生成，不从 raw non-empty 生成 | 否则 coarse block 会重新把 Glass 当 blocker |
| probe relocation | Glass 仍是 solid | 避免 probe 位于 dielectric medium 内；当前 query 没有 probe-medium state |
| probe classification | 使用 opaque/relocation-aware fixed-ray semantics；对 Glass case 单独验证 | RTXGI fixed-ray state 假设 signed distance 代表稳定 geometry；不能混用随机 refracted distance |

需要特别检查 Re: Flora 的 relocation safety cage：当前 candidate 要求 26 邻域 empty。一体素玻璃墙会改变附近 probe 的可用位置，可能让低密度 grid 在窗附近损失样本。应在 glass wall 两侧记录 nominal/actual probe positions、clearance 与 valid count。

### temporal / revision policy

- 放置/删除 Glass、改变它是否 block DDGI visibility：geometry revision；重建 semantic visibility bits，并触发 relocation/local recovery；
- 只改变 IOR、attenuation color/distance：radiance/material revision；局部降低 irradiance history retention，visibility history 可保留；
- 由 opaque <-> dielectric 改类：geometry + radiance 两者都变；
- immutable DDGI lighting snapshot 必须包含 glass material revision，禁止一个 sweep 混用两套 optical 参数；
- Glass edit 的局部 recovery 仍需满足当前 finite-field、atlas delta 与 publication gate；不能因为画面“看起来亮了”就提前发布；
- 新增 counters：glass segments、transmitted rays、Beer-underflow、TIR（DDGI 首版应为 0）、opaque distance、budget exhaustion、non-finite。

### Glass 表面如何使用 DDGI

平滑 dielectric surface 不应以黑色或 tinted Lambertian albedo 参与 `ddgiTransportHitRadiance()`。Glass primary appearance 的反射/折射应由专用 resolve 负责。DDGI 可以提供低频 environment fallback；若未来支持 rough glass，可研究 2021 production paper 提到的“irradiance data as prefiltered radiance for recursive glossy reflection”，但当前 `sampleDdgiDiffuseEnvironment()` 不是 smooth specular reflection query，不能直接冒充镜面结果。

## direct sun 与 local-light shadow policy

### Direct sun

当前 terrain VSM 是 scalar nearest-depth representation。Glass 若进入该 depth，会成为 opaque shadow；若简单 skip，则没有玻璃阴影。建议分两阶段：

1. 首个可发布 slice：Glass 从 terrain opaque shadow depth 中跳过，明确标注“无 Glass direct shadow / 无 caustics”；
2. 后续质量阶段：新增独立的 light-space RGB optical-transmittance pass，沿 sun direction 做 bounded voxel DDA，累计 entry/exit Fresnel 与 Beer。receiver 将它与 terrain VSM、leaf、cloud 分源相乘。

RGB transmittance map 只是 filtered colored shadow，不是 specular caustics。若没有追踪光线方向聚散和 receiver footprint，不能把它命名为 caustics。

### Local point lights

当前最多 8 个 point lights，receiver-to-light visibility 已经是 exact finite segment software query。最干净的升级是把 binary `visible/occluded` 改为 `trace_segment_transmittance()`：

- Opaque before emitter sphere：`T = 0`；
- Glass segments：`T *= entry/exit Fresnel * Beer`；
- 无 blocker：`T` 保留；
- `irradiance *= T.rgb`；
- diagnostics 同时记录 candidates、opaque-blocked、glass-transmitted、mean/min transmittance。

不要让 visible terrain、flora cache、tree-leaf cache 和 DDGI probe hit 分别复制 glass skip；它们都应走同一 local-light optical visibility helper。

## 分阶段实施与 gates

### Phase 0：material / schema seam

交付：

- Glass = 9；
- `VoxelMaterial` semantics；
- edit stats、inventory、particles、acoustics、collision、terrain support、emissive collector 明确策略；
- persistence schema 决策与 fixtures；
- 不改变 rendered image。

Gate：

- ID 8 Emissive round-trip 与 lighting 行为不回归；
- Glass state high nibble 非法值在 mutation 前拒绝或规范化；
- old fixtures byte-identical load；
- 若升级 schema，旧 binary 必须拒绝新文件，不能静默误解释；
- feature off 通过现有 fmt/check/test/hidden smoke。

### Phase 1：voxel-only dielectric

交付：

- `SceneQuery` + dense material-transition DDA；
- first opaque + GlassFront outputs；
- voxel/sky reflection/refraction；
- exact Fresnel/Snell/TIR/Beer；
- deterministic bounded path queue；
- CPU double-precision reference tests。

Gate scenes：

- 单 cell slab：正入射、30°、60°、接近 grazing；
- camera inside Glass；
- 2+ face-connected Glass cells，无内部 Fresnel event；
- 一 cell air gap，产生四个真实 interfaces；
- Glass 紧贴 opaque；
- chunk seam / world boundary；
- TIR angle 两侧；
- colored attenuation 随 path length 单调变化；
- DDA tie：edge/corner crossing 不重复或漏掉 cell；
- 所有 path budget exhaustion 都可见且默认场景为 0。

### Phase 2：opaque hybrid composition

交付：

- UnifiedOpaqueHDR ping-pong；
- validated screen-space raster sample；
- voxel/sky fallback；
- front raster occlusion；
- debug provenance views。

Gate scenes：

- Glass 后的 opaque flora/object 不被 depth prefill 剔除；
- Glass 前 raster opaque 保持前景；
- camera orbit 时 screen-hit/fallback 边界没有拉伸与未初始化 texel；
- viewport edge、disocclusion、resize、camera cut、terrain edit 都稳定；
- HDR emissive/local-light highlight 透过 Glass 不在 RGBA8 阶段裁掉；
- RenderDoc/validation layer 无 read-write feedback 与 layout hazard。

### Phase 3：DDGI / shadows / temporal hardening

交付：

- semantic DDGI visibility bits；
- straight-through DDGI Glass transport；
- relocation/classification tests；
- local-light RGB segment transmittance；
- direct-sun Glass policy；
- material revision 与 local recovery。

Gate：

- 窗两侧 probe radiance 与 opaque distance 分开 capture；
- Glass 前后 receiver 的 moment visibility 不出现因 Glass bit 不一致导致的黑边/漏光；
- relocation actual positions 与 valid count 在 repeated run 中 deterministic；
- optical 参数修改不重建无关 geometry，但能让 irradiance 在规定 epochs 内收敛；
- direct sun、local lights、DDGI exact sun 和 visible surface 不再各自拥有冲突的 glass skip；
- 不把 colored shadow 误验收为 caustics。

## 性能与质量门槛

### 必须新增的 instrumentation

- GPU scopes：`glass.resolve`、`glass.screen_trace`、`glass.voxel_query`、`glass.direct_transmittance`；
- per-frame counters：glass pixels、screen-hit/fallback 比例、DDA steps median/p95/max、interface events、active paths、TIR、throughput cutoff、budget exhaustion；
- DDGI counters：glass segments/rays、opaque/miss distance、visibility-bit rebuild bytes/time、local recovery epochs；
- memory report：新增 HDR targets 与 event buffers；
- debug views：GlassFront depth/normal/material、opaque depth provenance、screen validity、fallback reason、DDGI semantic occupancy。

1920×1080 下，一个额外 `RGBA16F` target 约 15.8 MiB；两个 `R32` GlassFront event planes 合计也约 15.8 MiB。若 raster color 从 RGBA8 升到 RGBA16F，再增加约 7.9 MiB。实际格式应由 capture 需要和带宽实测决定，不能默认所有数据都常驻 full resolution。

### release A/B gate

仓库已有 authoritative `render-steady` release scenario，600-frame warm-up、至少 120 samples，并跟踪 `frame.render`、`tracer.render`、`tracer.shadow_prepass` 与 `composition.pass`；当前 regression budgets 分别是 2%、2%、3%、3%（[`config/perf_scenarios.toml`](../config/perf_scenarios.toml#L3)）。按仓库标准使用 order-reversed `A,B,B,A` compare（[`docs/performance-benchmarking.md`](performance-benchmarking.md#L38)）。

硬 gate：

- Glass feature OFF / scene 无 Glass：必须满足现有全部 percent budgets，不能因新 descriptors/targets/branches 让默认路径回归；
- Glass ON：增加固定相机、固定 Glass coverage 的 named scenario，分别测 10%、25%、50% screen coverage；
- baseline/candidate 必须同 GPU、分辨率、present mode、camera、DDGI ready state 与 authored voxel signature；
- 同时报 median 与 p95，至少两侧各两轮并反转顺序；
- 任何 non-finite、validation error、budget exhaustion、DDGI mixed revision 都直接 RED；
- 先以“25% coverage 下 glass 增量 median ≤ 1.0 ms、p95 ≤ 1.5 ms @ 1080p internal”作为**规划目标**，不是当前实测或永久产品预算；正式数值必须在目标 GPU 上由 product/perf owner 写入 scenario 后才成为 ship gate。

质量 gate：

- CPU reference 的 interface sequence、exit position、Fresnel、TIR 和 Beer relative error 有固定 tolerance；
- screenshot suite 同时包含 screen-hit 与 forced-fallback reference，不允许只拍最有利视角；
- 10–15 秒 orbit / lateral motion 检查边缘 popping、植被穿帮、history ghost 与 DDGI convergence；
- 对 raster secondary visibility 的不支持场景保留已知限制截图，防止以后被误报为 regression 或误宣称已支持。

## 最终推荐

1. 认可 raster + compute/software voxel RT 的路线；不要为 Glass 引入 hardware RT infrastructure。
2. Glass 使用 ID 9，并先建设 material semantics；不要把又一个 type 特例散落到各 shader。
3. 复用 Contree 做 fast first surface，新增 dense atlas transition DDA 做 medium truth；不要试图从 surface leaf 猜 exit。
4. 以 opaque-first + UnifiedOpaqueHDR + independent glass resolve 作为 pass 架构；不要把现有 terrarium-specific prototype 直接改名复用成通用 Glass module。
5. primary Glass 做真实 refraction；DDGI 首版做同方向低频 transmission，保持 radiance/distance semantics 一致。
6. probe relocation 把 Glass 当 solid，DDGI exact visibility 不把它当 opaque blocker；这两个 policy 不矛盾，但必须在 `VoxelMaterial` 中显式命名。
7. direct sun 首版宁可明确“无 Glass shadow”，也不要输出错误的黑色 opaque shadow；带色 shadow 另做 RGB transmittance path。
8. transparency sorting 与 secondary visibility 分项目验收；OIT 不替代 SceneQuery。
9. 每阶段先做 deterministic reference/capture 与 release A/B gate，再进入下一阶段。

## Primary sources

- Epic Games: [Lumen Technical Details](https://dev.epicgames.com/documentation/unreal-engine/lumen-technical-details-in-unreal-engine?lang=en-US), [Lumen Performance Guide](https://dev.epicgames.com/documentation/unreal-engine/lumen-performance-guide-for-unreal-engine?lang=en-US), [Lit Translucency](https://dev.epicgames.com/documentation/unreal-engine/lit-translucency-in-unreal-engine).
- Epic Games SIGGRAPH 2022: [Lumen: Real-time Global Illumination in Unreal Engine 5](https://advances.realtimerendering.com/s2022/SIGGRAPH2022-Advances-Lumen-Wright%20et%20al.pdf).
- Godot Engine: [SDFGI documentation](https://docs.godotengine.org/en/4.4/tutorials/3d/global_illumination/using_sdfgi.html), [`sdfgi_integrate.glsl`](https://github.com/godotengine/godot/blob/36a2dd0272a6fc097642acea6287242ed201479f/servers/rendering/renderer_rd/shaders/environment/sdfgi_integrate.glsl), [`sdfgi_direct_light.glsl`](https://github.com/godotengine/godot/blob/36a2dd0272a6fc097642acea6287242ed201479f/servers/rendering/renderer_rd/shaders/environment/sdfgi_direct_light.glsl).
- Majercik et al.: [Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields, 2019](https://jcgt.org/published/0008/02/01/), [Scaling Probe-Based Real-Time Dynamic Global Illumination for Production, 2021](https://jcgt.org/published/0010/02/01/).
- NVIDIA RTXGI-DDGI: [DDGIVolume documentation](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/main/docs/DDGIVolume.md), [ProbeTraceRGS source](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl), [probe ray storage helpers](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/include/ProbeRayCommon.hlsl).
- PBRT v4: [Specular Reflection and Transmission](https://www.pbr-book.org/4ed/Reflection_Models/Specular_Reflection_and_Transmission), [Dielectric BSDF](https://www.pbr-book.org/4ed/Reflection_Models/Dielectric_BSDF), [Transmittance](https://pbr-book.org/4ed/Volume_Scattering/Transmittance).
- Khronos glTF: [`KHR_materials_transmission`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_transmission/README.md), [`KHR_materials_volume`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_volume/README.md), [Attenuation Test asset](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/main/Models/AttenuationTest/README.md).
- Khronos Vulkan: [OIT linked-list sample](https://docs.vulkan.org/samples/latest/samples/api/oit_linked_lists/README.html).
- Walter, Marschner, Li, Torrance: [Microfacet Models for Refraction through Rough Surfaces, 2007](https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.pdf).
- Amanatides, Woo: [A Fast Voxel Traversal Algorithm for Ray Tracing, 1987](https://doi.org/10.2312/egtp.19871000).
- Unity HDRP: [Screen Space Refraction](https://docs.unity3d.com/cn/Packages/com.unity.render-pipelines.high-definition%4010.4/manual/Override-Screen-Space-Refraction.html), [Refraction in HDRP](https://docs.unity3d.com/ja/Packages/com.unity.render-pipelines.high-definition%4010.5/manual/Refraction-in-HDRP.html).
