# DDGI Probe 直接太阳光摄入路径与墙边层带根因研究

研究日期：2026-08-09

源码基线：`f9d783d8`（`agent/ddgi-sun-research`）

状态：研究完成；生产修复与最终验收见 `41a89a6f` 和
[`ddgi_probe_seam_fix_plan.md`](../../ddgi_probe_seam_fix_plan.md)

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
   根因。后续 `sun_luminance 1.65 -> 0` matched `exact-irradiance` A/B 已完成：保持 sky model
   不变时，zero-sun 变为 `contrast=2.762, bands=0`。这确认 explicit direct term 是该场景的
   必要高对比激励源，不代表应删除这条合法的 bounced-indirect 路径。

## 后续现场验证与最终判断

研究提出的关键 A/B 后来在权威 saved terrain、固定 camera、固定 sun direction 和收敛场上
执行完毕：

- 只把 `sun_luminance` 从 `1.65` 设为 `0` 后，sky miss 输出不变，内部层带消失；
- 把 Probe-hit sun shadow origin 改为精确 `result.position` 后仍为 RED，说明 canonical sun
  origin 不是完整根因；
- 给每个 Probe 加确定性 SO(3) ray rotation 没有改善；spacing 16 反而更糟，密度或角采样
  不是可单独成立的修复；
- S2 capture 中，固定墙列上的每个亮度台阶与 terrain voxel Y row 切换精确重合；
- 让 spatial position 与 surface-side weight 使用连续 hit position，同时把 visibility、
  support、invalidation 和 zero class 留在 canonical receiver 后，重复 exact capture 从 RED
  变成两次 `bands=0`，真实 cached Final view 也变为 GREEN。

因此最终分层判断是：**太阳是合法能量源和必要放大器；形成可见规则层带的项目缺陷，是
terrain consumer 把连续 Probe spatial reconstruction 量化到 canonical voxel receiver，
并叠加旧 relocation-aware cell-face 不连续。** 修复保留 Probe-hit direct sun，只拆分
visibility receiver 与 spatial-weight position 的职责。完整证据与性能代价见上述修复计划。

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

研究阶段把两个 Re: Flora 特有行为列为候选；后续实验已表明它们能增加离散性，但不是本次
层带的充分根因：

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

### 假设的最终状态

- H-SUN-NECESSARY：**确认是必要激励源**。Probe-hit `directIrradiance` 归零后内部层带消失，
  但该项仍是正确的 bounced indirect；
- H-SUN-ORIGIN：**不是充分根因**。精确 `result.position` sun origin 对照仍为 RED；
- H-SPATIAL-REMAINING：**确认**。必须让 spatial reconstruction 使用连续 hit position，同时
  保留 canonical visibility 与 fail-closed 分类；
- H-RESOLUTION：**否定为单独修复**。spacing 16 没有消除症状，部分对照反而更差。

## 判别实验协议与已执行结果

### A/B 1：只把 `sun_luminance` 从 1.65 改为 0

保持 terrain、camera、sun direction、sky source、spacing、ray count 和 debug view 全部不变，
重新 build 到 converged，然后比较 `exact-irradiance`。该实验后来已按此协议执行：

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

现场结果为：A 稳定复现高对比层带；B 为 `contrast=2.762, bands=0`。这确认太阳是必要
激励源。它没有改变 sky miss，也没有提供删除 direct-sun transport 的依据。

### A/B 2：原计划中的 per-Probe transport 拆分

对层带上下最相关的 Probe 记录，不做逐像素全量日志，只记录：

- Probe index、actual/nominal position；
- 256 rays 中 miss/front-face/backface 数量；
- front-face hits 中 `NdotL > 0`、sun visible、sun occluded 的数量；
- sky、direct-sun、previous-DDGI 三项各自的 RGB 能量和；
- sun-visible hits 的 `result.position`、`center_position` 派生 origin 与第一遮挡 voxel。

该拆分原本用于区分：

1. 合法的 aperture sampling 差异；
2. canonical shadow origin 的遮挡误判；
3. angular undersampling；
4. 后续 spatial weighting 对正常 Probe 差异的放大。

后续 S2 receiver/voxel-row 对齐证据和 spatial-position matched A/B 已直接确认第 4 条；因此
生产修复没有继续增加全量 per-ray instrumentation。

### A/B 3：sun shadow receiver 对照

比较 `center_position` canonical origin 与精确 `result.position + normal * offset` 后，
`exact-irradiance` 仍为 RED，因此没有把 exact hit 用于 sun transport，也避免重新引入
self-hit、薄墙漏光和同一体素内不稳定。

## 修复决策边界

- **不要直接删除 Probe hit direct sun。** 这会删除设计要求的 sun-driven diffuse bounce，
  与 DDGI 论文、RTXGI 官方集成和本仓库 indirect-transport spec 都相违背。
- **不要用降低 sun luminance、提高 albedo 暗部、模糊 Irradiance Map 或提高默认 Probe 密度
  作为修复。** 这些只能降低症状对比，不能建立连续性契约。
- zero-sun A/B 最终为 GREEN，但 sun-shadow origin 对照仍为 RED；因此保留 direct-sun
  transport，并修复已被 receiver-row 证据确认的 spatial reconstruction 量化。
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
