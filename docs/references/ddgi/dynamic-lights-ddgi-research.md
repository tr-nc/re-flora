# 动态太阳与多局部光源的 DDGI 支持调研

调研日期：2026-08-23

代码基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`

范围：只审计和设计，不实现。

## 结论先行

1. **当前不是通用多光源渲染器。** 生产照明入口是一个全局方向光太阳：实时 shading 用一个 `U_SunInfo`，DDGI builder 用一个在完整 epoch 开始时锁存的 `U_DdgiRadianceSun`。仓库没有 point/spot/area light 的运行时数据、light count、GPU light buffer、选样或 shadow/visibility 路径。[`U_SunInfo`](../../../shader/slang/tracer_types.slang#L98-L107)；[`U_DdgiRadianceSun`](../../../shader/slang/ddgi_types.slang#L39-L47)；[锁存代码](../../../src/ddgi/resources.rs#L1260-L1300)
2. **DDGI 图集本身没有 1-light 限制。** 当前每个 probe 存的是八面体参数化的 diffuse irradiance atlas，不是 SH；即使换成 SH，表示也只编码所有入射 radiance 的和，不能从 basis 推断 light count。真正决定 0/1/N 的位置，是 probe ray 命中点是否能读取 N 个灯、选样并计算各自 visibility。[当前 atlas 格式](../../../src/ddgi/resources.rs#L24-L25)；[probe hit shading](../../../shader/slang/ddgi_probe_trace.slang#L86-L123)；RTXGI Integration [S3]
3. **太阳现在可以动，也可以改颜色/强度，但 direct 与 indirect 的响应完全不同。** 方向由 time-of-day、纬度、季节计算，颜色与 luminance 有 GUI 参数；direct terrain/raster 在当前帧读 live sun，DDGI indirect 则读跨多帧锁存的 snapshot。[GUI](../../../config/gui.toml#L818-L914)；[方向计算与上传](../../../src/app/core/mod.rs#L2937-L2943)；[snapshot](../../../src/tracer/mod.rs#L2774-L2814)
4. **当前连续太阳不稳定的根因不是“DDGI 不支持动态灯”，而是 revision/temporal policy 不适合连续变化。** 太阳方向、颜色或强度任何 bit 级变化都会生成新 radiance revision；新 revision 的 irradiance history retention 被强制为 0，仍然要扫完整个 volume。默认 spacing 32 是 `17³ = 4,913` probes，每帧最多 512 probes，因此一个 epoch 至少 10 个 batch。启用自动日夜循环且 simulation 运行时，默认每 0.05 秒推进太阳；常见 60 fps 下，一个 epoch 期间会积压多个 revision。scheduler 能保证最新请求合并、in-flight snapshot 不被改写、旧完整场继续可见，但结果会长期落后，并在完整 epoch 发布时发生全场低样本跳变，且很难进入同 revision 的后续 convergence epoch。[精确 identity](../../../src/environment_lighting.rs#L58-L85)；[history 规则](../../../src/ddgi/resources.rs#L292-L335)；[probe 数](../../../src/ddgi/atlas.rs#L213-L226)；[预算](../../../src/ddgi/config.rs#L4-L24)；[coalescing](../../../src/ddgi/scheduler.rs#L1-L5)
5. **direct shadow 另有独立连续移动风险。** 开启 shadows 时 shadow camera/map 每帧跟随最新太阳，但自动日夜循环没有像手动拖动时间那样清空 terrain/leaf temporal history；现有 VSM/leaf filter 又按相同 texel 坐标直接混合 previous/current，没有光空间重投影。因此“新太阳矩阵 + 旧光空间历史”存在拖影/错位风险。这是代码推断，尚缺专门动态图像证据。[更新条件](../../../src/app/core/mod.rs#L2994-L2997)；[仅手动变化清历史](../../../src/app/core/mod.rs#L2857-L2875)；[shadow camera](../../../src/tracer/mod.rs#L2628-L2651)；[VSM blend](../../../shader/slang/vsm_filtering.slang#L88-L101)
6. **推荐先解决动态太阳，再做局部灯。** 保持 direct current-frame 独立；把 live-light revision 与 DDGI transport revision 分开；光照变化只更新 irradiance，不碰 geometry visibility/relocation；用持续 round-robin 保底、相机/影响区 priority、按实际时间定义的 adaptive hysteresis 和 change detection。对大 step 可完整双缓冲并 crossfade；对连续小变化应限速/量化 transport snapshot，而不是每 0.05 秒 hard-reset 全场 history。
7. **局部灯路线从 0/1/小 N 开始，不应直接上 ReSTIR。** 先做稳定 ID 的 `LightGpu[]`、point light、live direct、voxel segment shadow、probe first-bounce injection；再做 spot、小 N spatial list 与影响区 priority。大 N 时，raster primary surface 才考虑 ReSTIR DI，probe/secondary hit 更匹配 world-space ReGIR。area light 需要形状采样、PDF 和 visibility；只有同时组合 light sampling 与 BSDF sampling 时才需要 MIS。[RTXDI/ReGIR](https://github.com/NVIDIA-RTX/RTXDI/blob/main/Doc/Integration.md) [S7]

## 审计边界与术语

本文把以下三层严格分开：

| 层 | “支持多光源”实际意味着什么 | 当前状态 |
| --- | --- | --- |
| 纯 voxel tracer | 每个 surface hit 能读取 0/1/N 个灯，计算类型、衰减/形状、材质响应，并对选中样本做 hit-to-light visibility | **一个方向太阳；0 个局部灯** |
| raster consumer | terrain 之外的 flora/leaves/props/particles 能读取 current-frame lights，获得 light culling/list 与 shadow 数据，把 direct 与 DDGI indirect 独立相加 | **一个方向太阳 + DDGI 环境项；0 个局部灯** |
| DDGI transport | probe ray 命中点能估计 N 灯的 direct radiance，再把聚合 radiance 过滤进 probe；scheduler 能在动态灯下及时、稳定地更新 | **只注入一个锁存太阳；atlas 表示可容纳多灯的和，但入口和时序不支持** |

**不要把“图集/SH 能表示多个灯的合成低频场”写成“渲染器支持多个灯”。** RTXGI 的官方集成流程正是由应用在 probe hit 上执行 direct lighting，SDK 只接收最终 radiance [S3]。SH 或八面体 atlas 限制的是方向频率、存储精度和重建质量，不是 light count；Lambertian irradiance 的低频 SH 表示可见 Ramamoorthi 与 Hanrahan [S8]。当前项目已在 DDGI migration 中移除生产 SH，shared sampler 的守卫也明确禁止旧 SH 资源重新进入生产 seam。[迁移说明](../../ddgi_migration_plan.md#L458-L461)；[当前守卫](../../../src/environment_lighting.rs#L299-L367)

## 当前实现审计

### 1. Probe trace 输入与单太阳注入

一个完整 DDGI build 锁存以下 transport 输入：太阳方向/颜色/luminance、terrain ray origin offset、DDGI receiver bias、voxel palette。identity 使用规范化太阳方向和各 `f32::to_bits()`，任一输入精确变化都会增加 revision。[snapshot 与 identity](../../../src/environment_lighting.rs#L38-L109)；[revision 更新](../../../src/environment_lighting.rs#L123-L143)

GPU builder 的 `U_DdgiRadianceSun` 只有一组 direction/color/luminance，注释明确禁止 build 跨帧期间读取 live per-frame sun。[`ddgi_types.slang`](../../../shader/slang/ddgi_types.slang#L39-L47) DDGI probe trace 每个 probe 发 64 条旋转射线：

- miss：用锁存太阳方向查询 authored sky；authored sky 的色阶/halo 会随太阳方向变化，但这里没有读取 sun color/luminance。[probe miss](../../../shader/slang/ddgi_probe_trace.slang#L164-L175)；[sky radiance](../../../shader/slang/skylight.slang#L61-L95)
- front-face hit：对单太阳做 cosine、above-horizon 和一次 exact voxel visibility march；再采上一完整 DDGI field 作为 recursive indirect，最后乘 voxel albedo。[hit radiance](../../../shader/slang/ddgi_probe_trace.slang#L73-L123)
- back-face hit：只记录负距离，供 visibility/relocation 语义使用。[ray record](../../../shader/slang/ddgi_probe_trace.slang#L177-L194)

因此当前 probe transport 包含 `Sun -> surface A -> probe -> receiver B`，并通过前一完整 field 跨 epoch 反馈更多 bounce，但没有 point/spot/area light，也没有 light list、light PMF、MIS 或 reservoir。

### 2. 存储不是 SH：irradiance 与 visibility 分离

默认 spacing 32 的 512³ voxel world 得到 `17 × 17 × 17 = 4,913` probes。[grid 公式](../../../src/ddgi/atlas.rs#L38-L72)；[精确尺寸测试](../../../src/ddgi/atlas.rs#L213-L226) 每个 probe 使用：

- irradiance：8×8 interior、1 texel gutter，`RGBA32F` 八面体 atlas；
- visibility：16×16 interior、1 texel gutter，`RG32F` directional distance first/second moments；
- transient ray data：每条 ray 一个 `float4(radiance.rgb, signed_distance)`；
- irradiance、visibility 和 global sky 都有 source/destination ping-pong 资源。[尺寸与格式](../../../src/ddgi/atlas.rs#L4-L8)；[资源字节](../../../src/ddgi/resources.rs#L567-L645)；[volume ownership](../../../src/ddgi/resources.rs#L749-L801)

irradiance filter 对每个 oct texel 遍历 64 rays，形成均匀球 Monte Carlo 的 diffuse irradiance/π，并与 history 混合。[irradiance filter](../../../shader/slang/ddgi_irradiance_filter.slang#L97-L131) visibility filter 独立形成距离矩，并可保留不同的 history。[visibility filter](../../../shader/slang/ddgi_visibility_filter.slang#L97-L165) 消费端按 surface normal 从 irradiance atlas 取方向值，再以八个邻近 probes 的空间、surface-side 与 moment visibility 权重组合。[atlas sample](../../../shader/slang/ddgi_query.slang#L127-L172)

这一区分对动态灯很重要：**光源移动使 radiance/irradiance 过时，但静态 terrain 下不会使 probe-to-geometry distance、relocation 或 classification 过时。** 当前代码已经允许 radiance revision 改变时继续认定 visibility history 有效，不过仍每个 batch 重写 visibility，后续可以跳过这项成本。[history validity](../../../src/ddgi/resources.rs#L292-L335)

### 3. History、完整 epoch 与连续太阳的滞后

当前默认 `ddgi_history_retention = 0.99`。[GUI](../../../config/gui.toml#L1061-L1070) 但实际规则是：

- source spacing 或 radiance revision 不同：irradiance retention = **0**；
- geometry 不变且 spacing 相同：visibility history 仍有效；
- same-revision accumulating epoch：retention 上限为 `epoch / (epoch + 1)`；
- stable：使用配置值；terrain topology recovery：最多 0.93。[规则](../../../src/ddgi/resources.rs#L292-L335)

新 radiance revision 因而不是在 0.99 history 上缓慢跟随，而是用一轮 64 rays/probe 的结果替换整个 irradiance field。scheduler 的安全性是好的：一个 work 是不可变的 full-volume epoch；新 radiance 只更新 `latest_radiance_revision`，不会改写或抢占 in-flight epoch；完成后直接选择最新 revision。[work identity](../../../src/ddgi/scheduler.rs#L23-L27)；[observe/claim](../../../src/ddgi/scheduler.rs#L332-L371)；[coalescing test](../../../src/ddgi/scheduler.rs#L675-L698)

默认每帧 ray budget 32,768，64 rays/probe，故 batch 为 512 probes；4,913 probes 需要 10 个 batch submission 才能形成一轮完整场。[预算常量](../../../src/ddgi/config.rs#L4-L9) 每个 batch 还要等后续帧读取并验证 trace stats，最后一批再读 atlas reduction 后才 publication。[batch/readback lifecycle](../../../src/ddgi/resources.rs#L1458-L1535)；[GPU pass 与 readback](../../../src/tracer/mod.rs#L3261-L3359) 所以 60 fps 下理论下限已约 167 ms，实际还要包含 readback/frame-in-flight 调度。

启用自动日夜循环且 simulation 运行时，太阳每 world tick 更新一次；默认 tick 为 0.05 s，即通常 20 Hz。默认一天 30 分钟，步幅虽小，bitwise identity 仍变化。[clock](../../../src/game_time.rs#L1-L2)；[advance](../../../src/game_time.rs#L60-L101) 连续运动时会出现：

1. 当前完整 field 保持可用，不会出现“半张 atlas”或黑场；
2. in-flight field 对内部所有 probes 使用同一个锁存太阳，因此没有 mixed-time epoch；
3. 最新太阳在 epoch 期间继续前进，过期请求被合并；
4. 完成时发布的是内部一致但已经落后的场；紧接着又以最新 revision 启动 epoch 0；
5. 因 revision 每次不同，irradiance history 每轮为 0，正常 same-revision convergence 很难发生；
6. 全场切换到单 epoch 的 Monte Carlo 估计，可能表现为全局亮度 pop、旋转采样噪声或周期闪烁，而不是 terrain edit 的局部更新波前。

这是**全局 radiance 失效**，不是 geometry 全量失效：不会重做 terrain relocation，也不会让 last complete field 消失，但所有 probe irradiance 最终都受太阳改变影响。

### 4. Direct sun 与 shadow history

live `U_SunInfo` 每帧更新，[`BufferUpdater` 调用附近](../../../src/tracer/mod.rs#L2763-L2788)；terrain voxel shading 将 current direct sun 与 DDGI environment 独立相加。[direct](../../../shader/slang/tracer.slang#L131-L185)；[最终组合](../../../shader/slang/tracer.slang#L462-L495) terrain-only path tracing reference 同样只支持一个太阳，并绕过 DDGI 做显式 sun sample 与多 bounce terrain trace。[reference](../../../shader/slang/tracer.slang#L188-L271)

shadow sources 分 terrain VSM、leaf opacity、cloud transmittance 三份历史；terrain/leaf 可局部失效，cloud history 独立。[runtime state](../../../src/tracer/direct_sun_shadow_runtime.rs#L24-L89) 当前行为：

- shadows 开启时每帧用最新太阳重算 directional shadow camera，重新 raster/trace/filter terrain 与 leaf shadow；
- 手动 time-of-day 或 VSM blur 变化时清 terrain/leaf history；terrain/flora 动态 occluder 也有各自失效调用；
- **auto day/night 的返回值被忽略，没有触发太阳变动 history reset**；
- VSM history 直接按 texel `lerp(history, current, alpha)`，leaf history 亦如此，无 previous light matrix/reprojection。[shadow pass](../../../src/tracer/mod.rs#L3373-L3479)；[leaf temporal](../../../shader/slang/leaf_shadow_temporal.slang#L32-L44)

因此 direct RGB/方向本身是 current-frame，但 shadow temporal term 可能滞后。简单地每 tick 清空 history 会消除 ghost，却可能暴露原始 VSM/leaf 噪声；更合理的是太阳角度 change detection、有效的 light-space reprojection/validation，或连续移动时提高 current weight。

### 5. Terrain edit 的局部刷新与当前防闪烁控制

最新六个 localized terrain update 提交已经形成一套值得复用但不能原样套到每 tick 动态灯的安全边界：

- edit bounds 合并，并扩一格 probe cell 形成 conservative invalidation bound；不宣称它是完整光传输影响域。[request/bound](../../../src/ddgi/terrain_refresh.rs#L105-L137)；[扩张](../../../src/ddgi/terrain_refresh.rs#L292-L303)
- 只有一个 staging candidate；旧 candidate 可完成但只有 exact latest token 能 promotion，避免 stale edit 暴露。[claim](../../../src/ddgi/terrain_refresh.rs#L149-L185)；[promotion gate](../../../src/ddgi/terrain_refresh.rs#L232-L268)
- consumer 在 staging build 期间继续用最后完整 active field；descriptor 与 physical volume 原子 promotion。[consumer policy](../../../src/tracer/mod.rs#L2794-L2812)；[publication](../../../src/tracer/mod.rs#L2374-L2476)
- dirty local probes 首 epoch 清 history；non-dirty irradiance/visibility tile byte-for-byte copy，避免局部 edit 变成全局 temporal noise。[trace reset](../../../shader/slang/ddgi_probe_trace.slang#L133-L145)；[irradiance copy](../../../shader/slang/ddgi_irradiance_filter.slang#L58-L89)；[visibility copy](../../../shader/slang/ddgi_visibility_filter.slang#L60-L89)
- 第一批从 edit 附近开始，但仍 round-robin 扫完所有 batches；当前并未省掉 non-dirty probes 的 trace，只是在 filter 时保留其 tile。[priority batch](../../../src/ddgi/resources.rs#L510-L548)；[完整 batch 迭代](../../../src/ddgi/resources.rs#L1458-L1517)
- local candidate 至少到 epoch 4（五次旋转），并连续两轮 atlas delta ≤ 0.1 才可见；promotion 后以 retention ≤ 0.93 继续全 volume topology recovery，最终发现非局部 bounce 变化。[门槛](../../../src/ddgi/config.rs#L11-L24)；[promotion](../../../src/ddgi/resources.rs#L1225-L1249)
- gate 还要求非空 dirty/preserved partition、不能整域 invalidation、promotion 有明确 history source、promotion 后不得出现 >0.1 的 high-delta epoch，closed scene 残留 luminance ≤ 0.00005。[验证脚本](../../../scripts/check_ddgi_local_terrain_convergence.sh#L66-L128)

对动态照明可直接复用的原则是：last-complete、immutable snapshot、latest-wins、局部 priority、非受影响 history 保留、后台全 sweep。不能直接复用的是“每个太阳 tick 建私有 staging volume并等五轮”：这会让连续太阳永远赶不上。

### 6. 全部 voxel/raster consumers

| Consumer | 当前 direct | 当前 indirect/environment | 动态局部灯的缺口 |
| --- | --- | --- | --- |
| terrain compute tracer | live 单太阳；VSM + leaf + cloud transmittance | DDGI terrain smooth query | 没有 light buffer、局部灯 shadow 或选样 |
| terrain path-tracing reference | live 单太阳，exact voxel shadow | 自己的 terrain-only multi-bounce + authored sky，绕过 DDGI | 同样只有太阳，不能作为 N-light reference |
| flora / flora LOD | live 单太阳、shared stylized shadow | compute cache 中预采 DDGI irradiance | cache 只含 environment；无局部 direct/list/shadow |
| leaves / leaves LOD | live 单太阳 + leaf transmission | tree-leaf cache 中预采 DDGI irradiance | transmission 绑定太阳；无局部 direct |
| dynamic fruit / sprinkler / particles / water droplets | shared `applyStylizedVoxelLighting` 的单太阳 | 直接 sample DDGI | 无局部 direct/list/shadow |
| probe visualization | 无 | 直接看 atlas | 仅诊断 |
| sky/cloud/lens flare/terrarium glass/panels | live 单太阳的各自表现 | 不采 DDGI | 若局部灯应影响这些材质，需要单独定义，不会自动获得 |

shared environment seam 在 [`environment_lighting.slang`](../../../shader/slang/environment_lighting.slang#L1-L20)；flora direct/indirect 合成在 [`flora_shadow.slang`](../../../shader/slang/flora_shadow.slang#L132-L151)；flora 与 leaf cache 只写 DDGI irradiance，[flora cache](../../../shader/slang/flora_lighting_cache.comp.slang#L60-L72)、[leaf cache](../../../shader/slang/tree_leaf_lighting_cache.comp.slang#L37-L48)。实际 consumer descriptor 列表覆盖 flora/LOD/leaves/LOD/sprinkler/fruit/particle/water-droplet 与 probe visualization。[descriptor list](../../../src/tracer/mod.rs#L2209-L2298)

## A）太阳：现状、稳定性与推荐策略

### A1. 当前支持矩阵

| 变化 | 能否运行时改变 | direct 响应 | DDGI indirect 响应 |
| --- | --- | --- | --- |
| 方向 | **能**：manual time、auto cycle、latitude、season | 当帧 `U_SunInfo`；shadow camera/map 当帧重算，但 temporal history 可能拖影 | 新全局 radiance revision；当前 epoch 完成后才发布，irradiance history 为 0；sky miss 与 hit direct 都改变 |
| 颜色 | **能**：GUI `sun_color` | 当帧改变 direct RGB | 新全局 revision；只改变 surface-hit sun injection，authored sky miss 不读此颜色 |
| 强度 | **能**：GUI `sun_luminance` | 当帧改变 direct 强度 | 新全局 revision；只改变 surface-hit sun injection，authored sky miss 不读此强度 |
| apparent disc/display | **能**：`sun_size`、`sun_display_luminance` | 影响 visible sun/path-trace disk/glass 等 presentation | 不在 `DdgiRadianceSnapshot`；不改变 DDGI authored-sky filter 或 probe hit 注入 |

方向变化是全局支持变化：全场 surface-to-sun visibility 可能变，所以不存在有限的物理 dirty AABB。颜色/强度也是全场 irradiance 变化；当前 atlas 没有按光源分解，无法从聚合值中精确抽出旧太阳分量再缩放。visibility moments 只取决于 probe ray 到 terrain 的距离，可继续保留。

### A2. 理想 temporal/update policy

推荐把太阳分成两个时钟：

```text
LiveSun(frame/tick)
    -> raster/voxel direct + current shadow

TransportSun(snapshot revision, rate-limited/latest-wins)
    -> DDGI probe indirect only
```

1. **direct 独立即时。** terrain、flora、props 先用 current sun 与 current shadow；DDGI 只补 diffuse indirect。原始 DDGI 也把 direct 每帧精确更新、probe indirect 跨帧历史更新 [S1, §4]。
2. **分离 revision domain。** live sun 每 tick 可变；transport revision 只在固定 cadence 或超过角度/色度/相对 luminance 阈值时生成。仍保留 latest-wins immutable snapshot。
3. **光照变化只使 irradiance 过时。** 不重算 relocation/classification；geometry 静态时保留 visibility atlas，最好跳过 visibility filter 与 gutter。
4. **持续 round-robin，不能进入“converged 后睡眠”。** 每个 probe 都有 starvation-proof age；相机可见 probes、最近高 change probes优先，但太阳最终覆盖全 volume。
5. **按时间定义 hysteresis。** 当前 filter 用 `result = lerp(current, history, h)`，建议 `h = exp(-Δt / τ)`，其中 `Δt` 是该 texel/probe 距上次真实更新的秒数。这样改变 batch size 不会无意改变响应时间常数。
6. **per-texel/probe change detection。** 小变化保留高 hysteresis，大变化临时降低 irradiance hysteresis；visibility 保持高 hysteresis。Production DDGI 报告了 25%/80% 变化的示例启发式，但这些不是本项目默认阈值，必须由固定场景标定 [S2, §4.3]。RTXGI 还提供 irradiance threshold、brightness impulse clamp 与 variability 指标 [S5]。
7. **step 与 continuous 分开。** manual noon→night 之类大 step 可完整 back-buffer 重建后 crossfade；连续小步更适合 rate-limited snapshots + EMA，避免每个 tick 都 epoch-0 hard reset。
8. **限制 recursive feedback 的旧能量。** 大幅变暗时可暂时降低多 bounce feedback 或先快速收敛 first bounce，再平滑恢复；否则 0.99 history 会留下长光尾。这个开关本身也必须 crossfade。

### A3. 候选更新方式与取舍

| 候选 | 优点 | 风险/代价 | 建议用途 |
| --- | --- | --- | --- |
| 当前 full epoch + history=0 | 内部一致、无 mixed-time atlas | 低样本全场 pop；连续运动永不收敛 | 仅保留作 correctness fallback，不作连续太阳默认 |
| 持续 RR + 原位 per-probe EMA | 最低延迟、内存少、易加 priority | 不同 probe age 形成更新波纹/batch seam | 小连续变化，需 age-aware hysteresis 与空间验证 |
| ping-pong 完整 epoch、非零 history、原子 publish | 没有半场新旧；现有资源接近可用 | 一轮延迟；publish 仍可能全局小 pop | 推荐的基础安全路径 |
| 上述再做 old/new crossfade | 最能压低 publication pop | consumer 需同时绑定两场；增加带宽/状态，响应更慢 | 大 step 或视觉敏感太阳变化 |
| local/camera priority + RR 保底 | 可见处先响应且保证最终一致 | 太阳物理影响仍全局；队列更复杂 | 所有 budgeted 方案的默认调度 |
| change detection + adaptive hysteresis | 稳定区不扰动，变化区快收敛 | 阈值过低把随机噪声当变化，过高拖影 | 与 RR/双缓冲组合 |
| brightness clamp | 抑制 impulse/firefly | 有偏，亮暗转换更慢 | 小而亮局部灯更重要；太阳慎用 |
| direct 独立 | 玩家先看到正确光位与阴影，GI 可容忍少量 lag | direct/indirect 短时不完全能量一致 | 必须采用 |

### A4. 预算

当前基础预算：

```text
rays/probe = 64
rays/frame = 32,768
probes/frame = 512
default probes = 4,913
full sweep = ceil(4,913 / 512) = 10 batch submissions
```

应把目标写成 GPU 时间预算加响应指标，而不只是 probe 数：

```text
Pupdate = floor(ray_budget_per_frame / rays_per_probe)
Tsweep  = ceil(Paffected / Pupdate) / fps
太阳角滞后 ≈ angular_velocity * Tsweep
```

建议起点仍维持现有 32,768 probe rays/frame 上限，先测 `ddgi.probe_trace`、`ddgi.irradiance_filter`、`ddgi.visibility_filter`、`ddgi.atlas_gutters`、`ddgi.atlas_reduce` scopes；确认静态 geometry 的 radiance-only 更新能否跳过 visibility。60 fps 理想下 full sweep 已约 167 ms，transport snapshot cadence 不应高于系统能完成一轮的速度；可先试 5 Hz 上限，再由角滞后、p95 luminance jump 与 GPU ms 调整，而不是把 5 Hz 写成最终常量。

## B）可移动局部光源：0/1/N 与实现路线

### B1. 当前能力

- point light：**0 个**；
- spot light：**0 个**；
- area/emissive light：**0 个**；
- directional sun：**固定一个结构实例**，可动态改参数；
- DDGI atlas：能保存未来 N 灯贡献的**聚合低频 irradiance**，但当前 probe trace 不会读取任何局部灯。

必须把 `0` 当正式支持状态：empty light buffer/count=0 不产生未初始化读、虚假 ambient 或无效 descriptor。然后实现 `1` 的完整闭环，最后扩到小 N；不要从“单灯 special-case shader 常量”直接跳到 many-light reservoir。

### B2. GPU/CPU 数据

建议建立稳定 ID 的 CPU light domain 和 `StructuredBuffer<LightGpu>`，至少包含：

```text
kind, flags, stable_id/generation
position, range
direction, spot_inner/outer_cos
RGB intensity or radiance
area shape axes/extents
shadow/visibility handle
conservative influence bound
```

另有 frame header/count、world/chunk/cluster 到 light-index range，以及只有 temporal reservoir 需要的 previous-frame light mapping。transform/direct revision 与 DDGI transport revision 分开；稳定 ID 防止排序或 buffer compaction 被误认为所有灯都改变。

对小 N，CPU 或 compute 按 chunk/probe cell 建 compact light lists；对 raster 用 screen tile/cluster list，对 probe hits 用 world-space cell list。可以共享 light records 与 influence builder，但不要强迫 raster 和 DDGI 使用同一种候选布局。

### B3. Direct lighting、sampling、MIS/reservoir

按规模递进：

1. **单 point。** receiver 到 light 的方向、inverse-square/项目定义的 finite-range falloff、N·L；一条有限 segment voxel shadow ray。terrain 与所有 raster consumer 必须先得到 current-frame direct，DDGI injection 是第二步。
2. **单 spot。** 在 point 基础上加 inner/outer cone；dirty bound 用 previous/current swept cone 的 conservative AABB/frustum。
3. **小 N。** 对空间裁剪后的 lights 精确求和，最稳定、确定、易建立 reference；最坏成本约为 `shaded hits × candidate lights × shadow traversal`。
4. **中 N。** 按 power、distance、solid angle 或 conservative contribution bound 选择 1–K 个灯，贡献除以 light-selection PMF。随机选一灯并做 `1 / p(light)` 校正已经可以无偏，不自动需要 MIS。[PBRT light sampling [S9]](https://www.pbr-book.org/4ed/Light_Sources/Light_Sampling)
5. **area light。** sample emitter position/direction，正确处理 area→solid-angle PDF，并对最终样本做 visibility。只有同时组合 explicit light sampling 与 BSDF sampling 时才用 MIS；point/directional 是 delta light，不能靠连续 BSDF 方向命中。[PBRT light interface / path sampling [S9]](https://www.pbr-book.org/4ed/Light_Sources/Light_Interface)
6. **many light。** raster primary surfaces 可考虑 ReSTIR DI；probe ray hits 没有稳定 screen G-buffer/motion-vector 邻域，优先 world-space light grid/alias/RIS，实测仍不足再引入 ReGIR。RTXDI 官方明确把“shading surfaces for RTXGI probes”列为 world-space sampling 用例；ReGIR 负责每帧构建 world cells 与候选 reservoir，最终 shadow visibility 仍由应用发射 [S7]。原始 ReSTIR 的百万动态 emissive lights 是研究算法能力，不是本项目预算承诺 [S6]。

**当前 raster shader 的 shadow 资源只有方向太阳的 VSM/leaf/cloud maps。** 即使绑上 `LightGpu[]`，也不会自动得到局部 visibility。可选路线：

- terrain compute tracer：直接复用 scene voxel segment march，最自然；
- raster 小 N：shadow cubemap/atlas（point 为 6 faces，动态成本高）；
- raster ray/voxel query：per-fragment/vertex 访问 scene 结构，精确但带宽与遍历成本大；
- 独立 compute direct-light/shadow cache，再由 flora/props 采样，适合 voxel/stylized receiver，但会牺牲小灯的高频边缘；
- 中大 N：只对选中 1–K 个样本发 visibility ray，避免每灯 shadow map。

推荐先让 terrain exact、raster 用同一 light evaluator 但采用可测的 compute/cache 或单灯 shadow 方案；以 terrain reference 校验 raster 能量与遮挡，不要在首阶段同时引入 ReSTIR。

### B4. Probe 注入与 visibility

在 [`ddgiTransportHitRadiance`](../../../shader/slang/ddgi_probe_trace.slang#L86-L123) 中把当前单太阳 evaluator 抽成共享 direct-light evaluator：

```text
hit direct radiance = sun contribution
                    + estimate(local light contributions)
hit output radiance = albedo * (hit direct + previous DDGI indirect)
```

- 小 N：遍历 hit 所在 light cell 的候选，对每盏做 exact segment visibility；
- 中 N：选择 K 个 candidate，以 PMF 校正；最终每个被选 sample 仍需 shadow ray；
- area：每个 light sample 还要 shape PDF；与 BSDF sampling 组合时才做 MIS；
- sky miss 不应被局部灯改变，除非未来把 emissive environment 明确定义为另一类 infinite light；
- light movement 不重算 distance moments/relocation/classification；只更新 irradiance；
- probe 的 recursive history 会传播 multi-bounce，必须为移动速度、history 与 feedback 设上限。

性能风险很直接：当前每条 front-face probe ray 对太阳最多做一次 voxel shadow march；若 naïve 遍历 N 灯，最坏 shadow traversal 近似乘 N。ray-record budget 不包含这些二级 visibility marches，所以必须新增 `light_candidates`、`local_light_samples`、`shadow_rays` 和 traversal GPU scope，不能只看 32,768 records。

### B5. 影响范围 dirtying

局部灯移动的 priority region 至少是：

```text
union(previous influence bound,
      current influence bound,
      swept bound over the frame/update interval)
expanded by one probe cage + safety margin
```

- 必须包含旧位置，否则旧亮斑只靠 0.99 history 慢慢衰减；
- point 用 swept sphere，spot 用 swept cone/frustum，area 用 swept shape bound；
- 这只是**优先集合，不是完整影响证明**：较远 probe 的射线仍可能命中影响区内被照 surface，recursive bounce 也会向外传播；
- 因而调度必须同时保留全 volume round-robin，或从 priority region 按 ring/age 外扩；
- direct current-frame 不等待 dirty probes；dirtying 只控制 indirect transport；
- 若灯的 range 接近全世界或是方向光，退化为 global radiance update。

当前 terrain local refresh 只把靠近 edit 的 batch 放前面，却仍 trace 所有 probes；真正想让局部灯预算随影响范围缩小，需要 sparse probe work list/mask 或多个局部 batch，而不仅是 filter 阶段 copy tile。

### B6. 移动速度与更新预算

定义：

```text
Tsweep_local = ceil(Pdirty * rays_per_probe / rays_budget_per_frame) / fps
motion_ratio = light_speed * Tsweep_local / probe_spacing
```

`motion_ratio > 1` 表示灯在高优先 probes 刷完之前已跨过一个 probe cage，indirect 亮斑落后/拉尾很可能可见。可先以高优先区 `motion_ratio <= 0.25–0.5` 作为实验目标，不作为发布常量。

超预算时的退化顺序建议：

1. direct 始终保真；
2. 保住局部 first-response probes，降低每 probe 首轮 rays 或 multi-bounce feedback；
3. 对高速灯降低/冻结 DDGI indirect contribution，停稳后用 2–5 个稳定 epoch 平滑 ramp-in；
4. 保留 RR 配额避免永久 stale；
5. 只有 profiler 证明 light selection 成为主要瓶颈，再上 ReGIR/ReSTIR，而不是限制 gameplay 光速。

“高速灯 direct-only，停稳后恢复 indirect”是项目设计建议，不是一手文献的固定规则；它牺牲短时能量完整性换取没有脱离光源的明显 GI 拖尾。

## 防 terrain-edit 式明显闪烁：推荐组合

推荐默认不是单一技巧，而是以下组合：

1. **direct 独立、current-frame；**
2. **last complete DDGI field 永不因新请求消失；**
3. **immutable snapshot + latest-wins；**
4. **局部 priority 处理 old/current/swept influence；**
5. **持续全局 RR 保证 eventual consistency；**
6. **radiance-only update 保留 visibility；**
7. **time-aware hysteresis + per-texel change detection；**
8. **大 step 用双缓冲完整场 + crossfade；**
9. **brightness clamp 只在小亮灯实测有 impulse 时开启；**
10. **每 probe 记录 age/change/queue reason，禁止 starvation。**

RTXGI/Production DDGI 的依据边界：hysteresis 与 change response、brightness clamp、variability、probe states、scroll plane history preservation 都有公开先例 [S2, S4, S5]；“任意 moving-light swept bound 能精确找出所有 affected probes”没有公开 DDGI 证明，必须视为 priority heuristic。现有 terrain 方案保留后台全 sweep，正好补这个正确性缺口。[现有一手资料综述](localized-geometry-edit-probe-update-research.md)

## 推荐分阶段路线与文件边界

工作量是“一名熟悉当前 renderer 的图形工程师”的粗估，不是文献数字；不含跨平台性能回归造成的大返工。

### Phase 0：动态照明可观测性（3–5 天）

- 增加 direct-only / indirect-only HDR capture；
- 记录 live revision、transport revision、active/in-flight age、每 probe last-update/change/queue reason；
- GPU scopes 拆出 probe hit light sampling 与 shadow visibility；
- 建 sun step、continuous sweep、moving point light 的固定相机 reference harness。

文件边界：`src/environment_lighting.rs`、`src/ddgi/runtime.rs`、`src/ddgi/resources.rs`、`src/tracer/mod.rs`、`src/app/core/environment_lighting_test_scene.rs`、`scripts/check_*`。此阶段不引入灯模型。

### Phase 1：稳定动态太阳（1–2 周）

- live/transport sun revision 分离与 snapshot rate limit；
- radiance-only skip visibility；
- continuous RR + age priority；
- nonzero time-aware irradiance history、change detection；
- direct shadow 的 sun-motion history validation/current-weight policy；
- step crossfade 只在指标证明 full-epoch pop 仍可见时加入。

文件边界：`src/game_time.rs`、`src/environment_lighting.rs`、`src/ddgi/{scheduler,runtime,resources}.rs`、`shader/slang/ddgi_{probe_trace,irradiance_filter,types}.slang`、`src/tracer/direct_sun_shadow_runtime.rs`、`shader/slang/{vsm_filtering,leaf_shadow_temporal}.slang`。

### Phase 2：一个 point light 的 end-to-end tracer bullet（2–3 周）

- CPU `LocalLight` 与稳定 ID；`LightGpu[]` ABI/upload，count=0/1；
- terrain voxel direct + exact segment shadow；
- raster consumer direct，先选一个可控 shadow 方案；
- probe hit first-bounce injection；
- old/current swept influence priority，visibility atlas 保留；
- 双灯加法测试先放在测试工具中，即使生产 cap 仍为 1。

文件边界：建议新增 `src/lighting/` 与 `shader/slang/local_lighting.slang`；修改 `src/tracer/{resources,pipeline_builder,buffer_updater,mod}.rs`、`shader/slang/tracer_types.slang`、`tracer.slang`、`flora_shadow.slang`、`flora_vertex.slang`、两个 lighting-cache shader、`ddgi_types.slang`、`ddgi_probe_trace.slang`。`src/auto-generated/gpu_structs.rs` 只能由 `cargo check` 生成，不手改。

### Phase 3：spot 与小 N（2–4 周）

- world/chunk/probe-cell light lists；raster tile/cluster lists；
- small-N exact loop 与硬 cap/overflow telemetry；
- point/spot shared attenuation/cone reference；
- priority/RR 配额、per-probe age、动态灯停稳 ramp-in；
- N=0/1/2/8/16 release benchmark。

文件边界：light list builder 独立 deep module；DDGI scheduler只消费“probe work priorities”，不拥有 gameplay light lifecycle。

### Phase 4：area lights 与 sampled N（2–4 周）

- emitter shape sampling、PDF、visibility；
- weighted light selection/RIS，必要时 light/BSDF MIS；
- firefly/change detection/brightness clamp；
- 独立 direct area light path，避免把 sharp direct 全塞进低频 probe。

### Phase 5：many-light（4–8+ 周，仅 measure 证明需要）

- raster ReSTIR DI；
- probe/secondary shading ReGIR；
- previous-frame light mapping、reservoir validity、bias/visibility 处理；
- 动态灯与异步 probe 的专项噪声/延迟治理。

## 测试、可观测指标与验收

### Correctness

- `N=0/1/2/8`，在线性 HDR、tone mapping 前做两灯加法性：`L(A+B)` 对 `L(A)+L(B)`；
- point range 边缘、spot cone inner/outer、area emitter front/back；
- occluded/unoccluded、跨 chunk、贴 terrain、穿 terrain、旧位置清除；
- terrain compute 与每类 raster consumer 的相同 receiver/albedo 对照；
- probe hit light estimator 对 small-N exact loop reference；
- 光变化不得改变 visibility/relocation revision；terrain edit 仍必须走现有 localized gate。

### 动态稳定性

- sun step：direct response frame、indirect T50/T90/T95、最大单帧 HDR luminance jump；
- sun continuous sweep：太阳角滞后、固定 ROI frame-to-frame RMSE/p99 delta、atlas publication pop；
- local light 以 `0 / 0.25 / 1 / 4 probe-spacing/s` 运动：ghost trail world length、旧位置衰减时间、peak lag；
- batch boundary heatmap：不能出现 512-probe batch 对应的亮度波；
- step/crossfade 与 in-place EMA A/B；
- 连续运动期间不得出现全黑/全清 atlas、mixed revision、stale candidate promotion 或 direct 等待 DDGI。

### 调度 telemetry

- 每 probe：`last_update_frame/time`、radiance epoch、visibility epoch、change magnitude、priority reason、starvation age；
- 每帧：dirty/priority/RR/updated/starved probe 数、coalesced revisions、active-vs-live lag；
- light：candidate count histogram、selected samples、PMF/weight max、shadow rays、occluded ratio；
- publication：old/new revision、field age、crossfade fraction、最大 atlas delta；
- current terrain logs 继续验证 `active_token_serial`、exact revision 与 consumer set。

### 性能

- 只认 release hidden app benchmark；固定 workload/camera；报告 median/p95；
- scopes：light upload/cull/list build、terrain direct、raster direct、`ddgi.probe_trace` 内 local sample 与 visibility、irradiance filter、visibility filter、consumer cache；
- N=0/1/2/8/16/64 scaling，分别报告 candidates 与实际 shadow rays，避免只用 light count 解释；
- 静态高 ray/full update 作为 converged reference，动态策略报告 RMSE/SSIM 与 time-to-convergence；
- 性能 gate 应同时限制 frame GPU ms、probe transport GPU ms、shadow ray count、VRAM 与 publication latency。

## 主要风险

1. **历史拖影与 flicker 是同一旋钮两端。** 直接把 retention 从 0 改回 0.99 会消除 pop，却可能留下秒级旧光；必须有时间常数与 change detection。
2. **当前 local refresh 不减少 trace cost。** filter copy 不等于 sparse tracing；移动局部灯若只照少量 probes，仍可能支付全 volume ray cost。
3. **probe filter 也很重。** irradiance 每 probe 约 `8×8×64` 次 ray accumulation，visibility 约 `16×16×64`；radiance-only skip visibility 是优先测量项。
4. **多个灯的二级 shadow traversal 可能比 ray record 数更快爆炸。** 预算必须以实际 candidates/visibility rays 计。
5. **raster 与 voxel scene visibility seam 不同。** terrain compute 可直接 march，现有 raster pipeline 只有太阳 shadow maps；统一光能量不代表统一 shadow 实现。
6. **聚合 atlas 无法单独移除某盏灯。** 灯关闭/离开时必须刷新旧 influence；history 太高会留下正能量 tail。
7. **小而亮灯增加方差。** oct atlas 不限制灯数，但 64 rays/probe 可能错过小 influence surface 或形成 impulse；需要 priority、importance sampling 与 clamp 的测量。
8. **recursive transport 没有有限局部影响域。** swept bound 只能排优先级，RR/外扩传播是 eventual consistency 的保障。
9. **ReSTIR/ReGIR 是深集成。** light mapping、previous data、bias correction 与 final visibility 缺一不可；不应拿算法名称替代当前小 N 的测量。

## 一手资料

| ID | 来源 | 本文使用点 |
| --- | --- | --- |
| S1 | Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*, JCGT 2019：[publisher page](https://jcgt.org/published/0008/02/01/) / [PDF](https://www.jcgt.org/published/0008/02/01/paper-lowres.pdf) | direct 每帧精确、probe history/hysteresis、跨帧 multi-bounce、动态 time-of-day 与剧烈变化延迟 |
| S2 | Majercik et al., *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production*, JCGT 2021：[publisher page](https://jcgt.org/published/0010/02/01/) / [PDF](https://jcgt.org/published/0010/02/01/paper-lowres.pdf) | per-texel change response、irradiance/visibility hysteresis 差异、probe state、局部 affected-only 的公开边界 |
| S3 | NVIDIA, [RTXGI DDGI Integration Guide](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/Integration.md) | 应用拥有 probe-hit direct lighting、materials、scene trace；最终 radiance 写入 DDGI |
| S4 | NVIDIA, [RTXGI DDGIVolume Reference](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md) | lower-frequency/background scheduling、random rotation、scroll history、classification、variability |
| S5 | NVIDIA, [`DDGIVolume.h`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/include/rtxgi/ddgi/DDGIVolume.h) / [`ProbeBlendingCS.hlsl`](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl#L350-L570) | 默认 hysteresis、irradiance threshold、brightness clamp、history blend 与局部 scroll clear |
| S6 | Bitterli et al., [*Spatiotemporal Reservoir Resampling for Real-time Ray Tracing with Dynamic Direct Lighting*](https://research.nvidia.com/labs/rtr/publication/bitterli2020spatiotemporal/) | ReSTIR DI 的 screen-space many-light 能力与研究规模边界 |
| S7 | NVIDIA, [RTXDI Integration / World-Space Sampling and ReGIR](https://github.com/NVIDIA-RTX/RTXDI/blob/main/Doc/Integration.md)；Boksansky et al., [*Rendering Many Lights with Grid-Based Reservoirs*](https://research.nvidia.com/labs/rtr/publication/boksansky2021rendering/) | ReSTIR DI、world-space ReGIR、RTXGI probe 用例、最终 visibility 仍归应用 |
| S8 | Ramamoorthi and Hanrahan, [*An Efficient Representation for Irradiance Environment Maps*](https://graphics.stanford.edu/papers/envmap/) | diffuse irradiance 的低频 SH 表示；basis 不规定 light count |
| S9 | Pharr, Jakob, Humphreys, [PBRT 4e Light Sampling](https://www.pbr-book.org/4ed/Light_Sources/Light_Sampling) / [Light Interface](https://www.pbr-book.org/4ed/Light_Sources/Light_Interface) / [A Better Path Tracer](https://pbr-book.org/4ed/Light_Transport_I_Surface_Reflection/A_Better_Path_Tracer) | light selection PMF、area-light PDF、light/BSDF sampling 与 MIS 边界 |

## 最终建议

当前最值得做的下一步不是“把 `U_DdgiRadianceSun` 改成一个数组”，而是先完成 Phase 0，并用动态太阳证明新的 temporal contract：**live direct 即时、transport snapshot 有界、visibility 保留、RR 不饿死、history 不全清、publication 不闪。** 这条 contract 稳定以后，一个 point light 才能沿相同 seam 进入 terrain、raster 和 DDGI；否则 N 个局部灯只会把现在太阳的全局 revision lag、shadow history ghost 与低样本 publication pop 放大 N 倍。
