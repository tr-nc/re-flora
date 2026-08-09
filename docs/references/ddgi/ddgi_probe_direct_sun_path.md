# DDGI Probe 直接太阳光摄入路径与墙边层带根因研究

研究日期：2026-08-09

源码基线：`f9d783d8`（`agent/ddgi-sun-research`）

状态：根因分层诊断；本文不修改 Shader 或 Rust 实现

## 结论先行

1. **这不是 DDGI 的天空采样“随机踩到太阳盘”。** Re: Flora 的 DDGI Probe miss
   路径调用 `getAuthoredSkyRadiance()`；它只包含 authored sky 渐变和一个平滑的太阳方向
   halo，不读取 `sun_luminance`、`sun_color`、`sun_size` 或太阳贴图。玩家看到的太阳贴图
   只在最终 composition 中采样，未进入 DDGI。
2. **太阳能量确实被显式写入 Probe，而且是有意设计。** 从 S1 开始，每条命中正面的
   Probe ray 都会在命中表面朝太阳再发一条精确体素遮挡 ray，计算
   `sun_color * sun_luminance * NdotL * visibility`，再连同上一轮 DDGI irradiance 一起乘
   命中体素 albedo，写成该 Probe ray 的返回 radiance。
3. 这不等于“把最终接收面的 direct sun 存进 ambient”。光路是
   `太阳 -> Probe ray 命中的表面 A -> 最终表面 B`。对 B 而言它已经反射一次，是 diffuse
   indirect lighting。最终表面 B 的 crisp direct sun 仍由独立的 direct-light/VSM 路径添加。
   2019 DDGI 论文和 NVIDIA RTXGI 官方集成也明确要求在 Probe front-face hit 上计算 direct
   lighting，再加入 recursive irradiance。
4. **当前墙上层带的“能量来源”和“形成层带的缺陷”不是同一件事。** 强太阳和开口令相邻
   Probe 记录的 radiance 差异很大；已有 matched capture 则把可见内部层带定位到这组非均匀
   Probe 值与当前空间权重/稀疏 relocation 场的交互。最终 direct VSM、consumer visibility，
   以及“raw atlas 自身已经带状”都已被现有证据排除为必要条件。
5. 已确认一个相关的项目缺陷：当前 relocation-aware position weight 在 nominal cell face
   两侧不连续。但是临时切换 ordinary nominal trilinear 后真实截图仍为 RED，所以它不是完整
   根因。**现在还缺一个干净的 `sun_luminance 1.65 -> 0` matched `exact-irradiance` A/B**；这项
   实验在不改变 sky model 的情况下能直接判断 Probe hit 的 direct-sun 项是不是层带的必要
   激励源。

## 需要先区分的两个问题

### 问题 A：太阳能量为什么会进入 DDGI Probe？

答案已由源码和一手算法资料确认：为了让太阳照亮的表面把能量反射到阴影区域，Probe 更新
必须在 ray hit 上评估 direct lighting。删除这条路径会同时删除 sun-driven one-bounce 和后续
multi-bounce diffuse GI，而不是只删除一个错误的“太阳盘采样”。

### 问题 B：为什么墙上会出现跟 Probe 上下层对应的亮度断层？

当前证据支持的链路是：

```text
屋顶/墙边开口形成高频太阳可见性边界
  -> 不同高度的 Probe rays 命中不同数量/方向的阳光表面
  -> Probe hit 上的二值 direct-sun shadow 产生高对比 radiance
  -> 每个 Probe 独立过滤为低分辨率 Irradiance Map
  -> 稀疏且 relocation 后的八 Probe 空间权重混合这些非均匀值
  -> 当前权重契约至少有一个已确认的 cell-face 不连续
  -> 墙上出现 Probe 层带
```

其中“强太阳产生相邻 Probe 高对比”是高度可信的放大机制；“当前空间查询把差异变成层带”有
matched capture 支持；“direct-sun transport 本身是否还存在命中点/遮挡误判”尚待专门 A/B，
不能只从最终截图推断。

## Re: Flora 的实际光照路径

### 1. DDGI build 锁存独立的太阳 lighting snapshot

`U_DdgiRadianceSun` 只有：

- `direction`；
- `terrain_ray_origin_offset_world`；
- `color`；
- `luminance`。

它没有 `sun_size`、`sun_display_luminance` 或太阳纹理。Rust 在一次完整 DDGI build 开始时从
`DdgiRadianceSnapshot` 锁存这四项，保证跨多帧 build 不读取不断变化的 live uniform。

源码：

- [`ddgi_types.slang`](../../../shader/slang/ddgi_types.slang)，`U_DdgiRadianceSun`；
- [`resources.rs`](../../../src/ddgi/resources.rs)，`latch_radiance_snapshot()`；
- [`environment_lighting.rs`](../../../src/environment_lighting.rs)，`DdgiRadianceSnapshot` 与
  radiance identity。

### 2. 每个有效 Probe 固定追踪 256 条 Fibonacci rays

`ddgi_probe_trace.slang` 从 relocation 后的 `actual_position` 发出 256 条确定性球面 rays。每条
ray 有三种主要结果：

| Probe ray 结果 | 写入 radiance | 写入 distance |
| --- | --- | --- |
| miss | `getAuthoredSkyRadiance(direction, sunDirection)` | far distance |
| backface hit | `0` | 负 hit distance |
| front-face hit，S0 | `0` | 正 hit distance |
| front-face hit，S1 及以后 | `ddgiTransportHitRadiance(...)` | 正 hit distance |

源码：[`ddgi_probe_trace.slang`](../../../shader/slang/ddgi_probe_trace.slang)，
`fibonacciDirection()` 与 `main()`。

因此 direct sun 并不是靠 256 条 Probe rays 中某一条碰巧对准太阳方向而进入。Probe ray 先命中
普通几何，随后 hit shader 逻辑从该表面朝太阳发一条单独的 shadow ray。

### 3. Front-face hit 上显式加入 direct sun

`ddgiTransportHitRadiance()` 的计算等价于：

```text
directIrradiance = sun.color
                 * sun.luminance
                 * sunAboveHorizon
                 * max(0, dot(hitNormal, sunDirection))
                 * exactVoxelSunVisibility

rayRadiance = hitAlbedo * (directIrradiance + previousDdgiIrradiance)
```

`exactVoxelSunVisibility` 从 `result.center_position` 沿存储 normal 重建 canonical voxel-surface
位置，再沿 normal 加 `terrain_ray_origin_offset_world`，朝单一 `sunDirection` 调用
`marchScene()`。它是 0/1 二值遮挡；没有 soft sun disk，也没有 VSM、leaf shadow 或 cloud
shadow。

源码：[`ddgi_probe_trace.slang`](../../../shader/slang/ddgi_probe_trace.slang)，
`ddgiExactTerrainSunVisibility()` 与 `ddgiTransportHitRadiance()`；
[`terrain_ray_origin.slang`](../../../shader/slang/terrain_ray_origin.slang)，
`terrainRayOriginAlongNormal()`。

这条实现有两个与墙边高对比直接相关、但尚未被证明有错的项目特性：

- shadow ray 从 canonical `center_position` 派生点出发，而不是 Probe ray 的精确
  `result.position`；
- 它采样单一太阳中心方向并使用 binary visibility，而最终 direct-light 路径可以使用不同的
  receiver 和 VSM/leaf/cloud filtering。

这两项会让墙/屋顶边缘附近的 Probe-hit direct radiance 比最终可见 direct term 更离散。它们是
应测量的候选机制，不是本文已经确认的修复点。

### 4. Ray radiance 被过滤成每个 Probe 的 8x8 Irradiance Map

每个 Probe 的 256 条 ray records 独立做 cosine convolution，写入 8x8 octahedral interior。
filter 在不同 Probe 之间不做平滑；相邻 Probe 可以保留很不同的 irradiance。

源码：[`ddgi_irradiance_filter.slang`](../../../shader/slang/ddgi_irradiance_filter.slang)，
`main()`。

### 5. 最终消费者仍把 direct 与 environment 分开

Terrain final path 的结构是：

```text
color = environmentIrradiance * albedo;
color += directLight;
```

`exact-irradiance` debug view 在添加 `directLight` 之前直接返回 DDGI query 的 irradiance。这也是
为什么该 debug view 上仍存在的层带不能归因于最终 direct VSM 合成。

源码：[`tracer.slang`](../../../shader/slang/tracer.slang)，`directLighting()`、
`ddgiTerrainDebugValue()` 与 `getPixelColor()`。

## 天空模型里到底有没有太阳？

答案需要分三层说。

### 事实 1：DDGI sky miss 不包含可采中的太阳盘

`getAuthoredSkyRadiance()` 只返回 `getSkyColor() * pi`。`getSkyColor()`：

- 根据太阳高度选择 authored top/bottom sky keyframes；
- 根据 view altitude 混合天空上下色；
- 用 Henyey-Greenstein 形式产生一个平滑、归一化、clamp 后的 sun-direction halo；
- 不读取太阳物理 luminance、color、size 或纹理。

因此天空并非与太阳方向完全无关，但它没有一个高能量 delta/disk 可被 Fibonacci ray
“不小心踩中”。把 `sun_luminance` 设为 0 时，这条 sky miss 函数输出保持不变，只要太阳方向
不变。

源码：[`skylight.slang`](../../../shader/slang/skylight.slang)，`getSkyColor()` 与
`getAuthoredSkyRadiance()`；
[`sky_environment_data.slang`](../../../shader/slang/sky_environment_data.slang)，authored
keyframes。

### 事实 2：Global Sky Irradiance 也不包含太阳盘

DDGI volume 未 ready 或 receiver 在 volume 外部时，会读取单独的 Global Sky Irradiance。
它由 2048 条 Fibonacci directions 积分同一个 `getAuthoredSkyRadiance()` 得到，仍然只传
`sunDirection`，不传 `sun_luminance` 或 sun sprite。

源码：[`ddgi_global_sky_filter.slang`](../../../shader/slang/ddgi_global_sky_filter.slang)，
`main()`；[`ddgi_query.slang`](../../../shader/slang/ddgi_query.slang)，
`sampleDdgiGlobalSky()`。

### 事实 3：玩家看到的太阳盘只在 composition 中加入

`computeSkyWithSunAndStars()` 先算 `getSkyColor()`，再采样 `sun_sprite`，并使用独立的
`sun_display_luminance` 合成太阳盘。该贴图和 display luminance 不在 DDGI binding/snapshot
中。`getSkyColorWithSun()` 虽然仍定义在 `skylight.slang`，但当前仓库没有调用点。

源码：[`composition_sky.slang`](../../../shader/slang/composition_sky.slang)，
`sampleSunSprite()` 与 `computeSkyWithSunAndStars()`；
[`tracer_types.slang`](../../../shader/slang/tracer_types.slang)，`U_SunInfo`。

## 为什么“Probe 只算环境光”与源码并不矛盾

`environmentIrradiance` 描述的是**最终 receiver 收到的 diffuse indirect/environment term**，
不是说构建该项时禁止读取直接光源。对最终 receiver B：

| 光路 | 最终分类 | 应否经过 DDGI |
| --- | --- | --- |
| `Sun -> B` | direct sun | 否；由独立 direct path 计算 |
| `Sky -> B` | environment/diffuse irradiance | 是 |
| `Sun -> A -> B` | one-bounce diffuse indirect | 是 |
| `Sun -> A -> C -> B` | multi-bounce diffuse indirect | 是，通过 feedback 收敛 |

2019 原始 DDGI 论文 §4 的更新流程明确要求：Probe rays 先得到 surfels，再以与可见像素相同的
direct + indirect routine 给 surfels 着色，最后把 ray radiance 过滤进 Probe。§4.3 再次说明
secondary surfel shading 同时包含 direct illumination pass 与 previous-probe indirect pass。

一手来源：

- [Majercik et al. 2019, §4, PDF p. 8](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=8)；
- [Majercik et al. 2019, §4.3, PDF p. 10](https://jcgt.org/published/0008/02/01/paper-lowres.pdf#page=10)。

NVIDIA RTXGI 的官方 Integration Guide 把 Probe ray generation 的步骤直接写成：处理 miss/
backface、**在 front-face hits 上执行 direct lighting**、在 hit 附近 sample recursive
irradiance、再写最终 radiance。官方 `ProbeTraceRGS.hlsl` 的实际样例则是：

- miss 立即存 `skyRadiance`；
- front-face hit 调 `DirectDiffuseLighting()`；
- 再调用 `DDGIGetVolumeIrradiance()`；
- 把 direct diffuse 与 albedo/PI 乘 recursive irradiance 相加后存进 ray data。

一手来源：

- [RTXGI Integration Guide, lines 56-72](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md#L56-L72)；
- [RTXGI sample `ProbeTraceRGS.hlsl`, lines 131-200](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl#L131-L200)。

所以 Re: Flora 的架构边界符合 DDGI/RTXGI：**Probe 不替代最终 direct lighting，但 Probe hit
必须允许 direct light 成为 bounced radiance 的种子。**

## 与 saved-terrain 现场证据的合并判断

现有 matched hidden-release 证据来自同一 terrain、camera、lighting、spacing 32 和 converged
field：

| 视图 | 结果 | 能排除什么 |
| --- | --- | --- |
| `exact-irradiance` | RED，24 internal bands，ratio `1.961422` | 层带不需要最终 direct VSM |
| `unoccluded-irradiance` | 同样 RED，24 bands，ratio `1.961422` | consumer moment/hard visibility 不是必要条件 |
| `equal-weight-irradiance` | GREEN，1 band，ratio `0.067848` | 移除 position/surface-side 幅值后层带消失 |
| `raw-cage-irradiance` | GREEN，1 band，ratio `0.089716` | raw local atlas 本身没有同样的内部层带 |

来源：[`ddgi_probe_seam_research.md`](ddgi_probe_seam_research.md) 与
[`ddgi_probe_seam_fix_plan.md`](../../ddgi_probe_seam_fix_plan.md)。

后续 terminal readback 又确认：固定 cell face 两侧八个 Probe 都 valid/trustworthy、visibility
均为 1、无 rejection；但旧 relocation-aware 聚合权重从约 `0.477975` 跳到 `0.523814`。
CPU 一维 oracle 也确认 shared-probe weight 在相邻 cell 两侧为 `1.0` 与 `0.9090909`。这是已确认
的 continuity defect。

不过 ordinary nominal trilinear 临时进入 production consumer 后，真实
`exact-irradiance` 仍为 RED（22 bands，ratio `4.291830`）。因此当前最严谨的分层结论是：

### 已确认事实

- direct sun 由 Probe hit shader 显式加入，不来自 sky disk；
- final direct term 不是 debug 层带的来源；
- consumer visibility 不是层带的必要条件；
- 局部 raw atlas 没有同样的层带；
- 当前 relocation-aware position weight 违反 cell-face continuity。

### 高可信推断

- 太阳通过 Probe-hit shading 令相邻 Probe 的数值差异变大，是层带的强放大器；
- 可见层带主要在八 Probe 空间混合非均匀 irradiance 时形成，而不是由太阳贴图直接投影到
  Probe；
- binary central-direction sun visibility、canonical hit receiver、256-ray sparse angular
  sampling 和 32-voxel spacing 共同让墙边开口成为很强的压力测试。

### 仍待验证的假设

- H-SUN-NECESSARY：把 Probe-hit `directIrradiance` 归零后，内部层带是否消失；
- H-SUN-ORIGIN：`center_position` 派生的 sun shadow origin 是否在墙/屋顶边缘把本应遮挡的
  hit 分类为 lit；
- H-SPATIAL-REMAINING：修复已知 position continuity 后，是否仍需要更平滑且连续的空间
  reconstruction 才能混合高对比 Probe 值；
- H-RESOLUTION：若连续 reconstruction 仍失败，层带是否随 spacing 32/16/8 缩放，从而属于
  稀疏场质量上限。

## 最小、判别力最高的下一步实验

### A/B 1：只把 `sun_luminance` 从 1.65 改为 0

保持 terrain、camera、sun direction、sky source、spacing、ray count 和 debug view 全部不变，
重新 build 到 converged，然后比较 `exact-irradiance`：

- A：`sun_luminance=1.65`；
- B：`sun_luminance=0`。

这个 A/B 比“把太阳转走”更干净。转动太阳同时会改变 authored sky keyframes/halo 和实际光路；
把 luminance 设为 0 则只令 `ddgiTransportHitRadiance()` 的 explicit direct term 归零，
`getAuthoredSkyRadiance()` 保持不变。`exact-irradiance` 又天然排除了最终 direct-light 合成。

判定：

- B 仍 RED：direct sun 不是层带的必要条件，继续修 spatial reconstruction；
- B GREEN：direct sun 是必要的高对比激励源，但仍不能直接推导“应删除 direct sun”；转入
  per-Probe transport 和 shadow-origin 诊断；
- B 只降低 contrast、不消除 band count：太阳是放大器，空间查询仍是形成层带的机制。

该 A/B 尚未在本文工作树执行；权威 `saves/terrain_snapshot.rflterrain` 和历史 capture
artifact 都不在该独立研究 worktree 中，本文不以历史截图代替新的 matched run。

### A/B 2：如果太阳是必要条件，拆开 hit direct 的三个中间量

对层带上下最相关的 Probe 记录，不做逐像素全量日志，只记录：

- Probe index、actual/nominal position；
- 256 rays 中 miss/front-face/backface 数量；
- front-face hits 中 `NdotL > 0`、sun visible、sun occluded 的数量；
- sky、direct-sun、previous-DDGI 三项各自的 RGB 能量和；
- sun-visible hits 的 `result.position`、`center_position` 派生 origin 与第一遮挡 voxel。

这会区分：

1. 合法的 aperture sampling 差异；
2. canonical shadow origin 的遮挡误判；
3. angular undersampling；
4. 后续 spatial weighting 对正常 Probe 差异的放大。

### A/B 3：只有观察到 origin 误判后，才比较 receiver

比较 `center_position` canonical origin 与精确 `result.position + normal * offset` 时，必须同时
记录 self-hit、漏光和 repeated-run stability。不能因为精确 hit position 让当前截图变平滑，就
忽略它可能重新引入同一体素内变化或薄墙漏光。

## 修复决策边界

- **不要直接删除 Probe hit direct sun。** 这会删除设计要求的 sun-driven diffuse bounce，
  与 DDGI 论文、RTXGI 官方集成和本仓库 indirect-transport spec 都相违背。
- **不要用降低 sun luminance、提高 albedo 暗部、模糊 Irradiance Map 或提高默认 Probe 密度
  作为修复。** 这些只能降低症状对比，不能建立连续性契约。
- 如果 zero-sun A/B 仍 RED，优先完成 spatial reconstruction 的连续性修复与真实截图门槛；
  不再追 sky disk。
- 如果 zero-sun A/B GREEN，保留 direct-sun transport 的物理角色，先修被证实的 sun-shadow
  origin/visibility 错误；若所有 hit 分类都正确，则把问题定性为高频 lighting 对低频稀疏 DDGI
  的压力场景，并为 query reconstruction 或可负担 density 建立明确质量界限。
- 任一修复必须让 `exact-irradiance` 与 normal view 的内部 band metric 共同 GREEN，同时保留
  真实 elongated beam，且 sealed/thin-wall/terrain-edit gates 不回归。

## 与官方 DDGI/RTXGI 的对照结论

| 行为 | DDGI/RTXGI 一手资料 | Re: Flora | 判断 |
| --- | --- | --- | --- |
| Probe ray miss 存 sky radiance | RTXGI sample lines 131-137 | `getAuthoredSkyRadiance()` | 一致 |
| Front-face hit 算 direct lighting | 2019 §4/§4.3；Integration lines 68-70 | exact voxel sun shadow | 一致的架构，项目自定义可见性 |
| Hit 上采 previous Probe irradiance | 2019 previous-frame recursion；RTXGI sample lines 165-199 | `source.irradiance` | 一致 |
| Probe 存低频 diffuse irradiance | RTXGI Algorithms limitations | 8x8 octahedral Irradiance Map | 一致 |
| 最终 receiver direct 单独计算 | 2019 direct + indirect composition | VSM/leaf/cloud direct + DDGI environment | 一致 |
| canonical voxel-center sun receiver | 官方资料以实际 surfel hit 为输入 | `center_position` 派生 | Re: Flora 特有，需独立证明 |
| binary exact voxel visibility | 官方 RTXGI 以应用 RT shadow 为例 | custom voxel marcher 0/1 | Re: Flora 特有，需独立证明 |

NVIDIA 官方还明确把 DDGI 的限制描述为低频 GI，不能复现高频 radiometric/geometric detail。
这支持把墙边窄开口 + 强太阳视为压力场景；它不支持把内部规则层带自动视为“算法本来就这样”。

一手来源：

- [RTXGI Algorithms, limitations](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Algorithms.md#L27-L35)；
- [Majercik et al. 2021, §8.1](https://jcgt.org/published/0010/02/01/paper-lowres.pdf#page=25)，小而亮的光源会暴露 indirect convergence/ghosting 限制；
- [Roháček 2022, §§3.3-3.4](https://cescg.org/wp-content/uploads/2022/04/Rohacek-Improving-Probes-in-Dynamic-Diffuse-Global-Illumination.pdf#page=4)，relocation weight 与低分辨率 directional depth 可产生 cage-scale artifact。该文是非同行评审的实现研究，仅作辅助证据。

## 来源清单

所有外部来源均为论文、NVIDIA 官方文档或 NVIDIA 官方源码；RTXGI 链接固定到
`f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6`，避免 `main` 漂移。

1. Zander Majercik, Jean-Philippe Guertin, Derek Nowrouzezahrai, Morgan McGuire,
   *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*, JCGT 2019：
   <https://jcgt.org/published/0008/02/01/>。
2. Zander Majercik, Adam Marrs, Josef Spjut, Morgan McGuire,
   *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production*, JCGT 2021：
   <https://jcgt.org/published/0010/02/01/>。
3. NVIDIA RTXGI, *Integration Guide*：
   <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md>。
4. NVIDIA RTXGI sample, `ProbeTraceRGS.hlsl`：
   <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/samples/test-harness/shaders/ddgi/ProbeTraceRGS.hlsl>。
5. NVIDIA RTXGI, *Algorithms*：
   <https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Algorithms.md>。
6. Dominik Roháček, *Improving Probes in Dynamic Diffuse Global Illumination*, CESCG 2022：
   <https://cescg.org/cescg_submission/improving-probes-in-dynamic-diffuse-global-illumination/>。
