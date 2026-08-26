# 树叶阴影 alpha/depth 与移动稳定性研究

> 日期：2026-08-23
>
> 分支：`agent/foliage-shadow-research`
>
> 基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`
>
> 范围：只调研、诊断与设计，不修改生产 shader、Rust、配置或生成文件。
>
> 证据标记：**【项目事实】**来自本基线代码；**【外部事实】**来自论文、API 规范或厂商/引擎官方技术文档；**【推导】**由已列事实直接推出；**【建议】**仍需在固定场景、release-mode 应用中验证。

## 结论先行

**【结论】当前“非实心”树影不是普通 alpha-cutout 的边缘插值，而是一条独立的、分层透明度合成管线：** 每个叶片 LOD billboard 在光空间写入固定 opacity，所有重叠层在**没有 depth test、没有 depth write**的颜色附件中做 premultiplied alpha 累积；随后按同一光空间 texel 与上一帧线性混合，再经低分辨率 binary influence mask、receiver 侧 3×3 `max` 采样和最小透射率 clamp，最后才与当前启用的 terrain EVSM 和 cloud transmittance 相乘。PCSS 实现仍在代码中，但当前 terrain caller 固定选择 VSM/EVSM。它有意把很多二值叶片的覆盖变成连续冠层 opacity；默认值还明确是 `fragment_opacity = 0.4`、`temporal_alpha = 0.4`、`filter_radius = 2 texels`、`min_transmittance = 0.14`。见 [`leaves_shadow.vert.slang:131-216`](../shader/slang/leaves_shadow.vert.slang)、[`leaves_shadow.frag.slang:7-12`](../shader/slang/leaves_shadow.frag.slang)、[`graphics_pipeline.rs:176-196`](../crates/re-flora-vkn/src/pipeline/graphics_pipeline.rs)、[`leaf_shadow_temporal.slang:32-44`](../shader/slang/leaf_shadow_temporal.slang)、[`leaf_shadow_mask.slang:23-51`](../shader/slang/leaf_shadow_mask.slang)、[`tracer_shadowing.slang:223-265`](../shader/slang/tracer_shadowing.slang) 和 [`gui.toml:1094-1147`](../config/gui.toml)。

**【结论】“真实树影就是 binary”混淆了三个层次。** 单条 infinitesimal ray 遇到完全不透明叶片时，可见性确实是 0/1；一张叶片 cutout 内的几何覆盖也可用 binary mask 表示。但地面像素接收的是有限 footprint、有限太阳圆盘方向和许多叶层的积分：太阳被部分遮挡时产生 penumbra，冠层开口形成 sunfleck/dappled light；真实叶片还具有可测量的反射与透射（BRDF/BTDF），不是只剩 0/1。植物冠层光学实验明确把 direct irradiance 分成全日照、全阴影和 penumbral irradiance，并指出太阳不是点光源，叶片距离越远越可能只遮住太阳圆盘的一部分。[Smith, Knapp and Reiners, *Penumbral Effects on Sunlight Penetration in Plant Communities*, Ecology 1989](https://doi.org/10.2307/1938093)；真实叶片 BRDF/BTDF 与组织内散射的图形学模型见 [Wang et al., *Real-Time Rendering of Plant Leaves*, SIGGRAPH 2005](https://graphics.cs.yale.edu/publications/real-time-rendering-plant-leaves)。

**【结论】把叶影改成 per-fragment opaque/binary 很可能先强化 aliasing，而不是消除闪烁。** 固定阈值 alpha test 对过滤后的 alpha 仍输出二值结果；Wyman 与 McGuire 的论文直接指出这种 binary query 会在 alpha 边界 alias，普通几何 AA 无法处理纹理空间边界，远处还会侵蚀/丢失覆盖。[Wyman and McGuire, *Hashed Alpha Testing*, I3D 2017](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf) 当前项目的叶片本来就是细小、离散、随风移动的光空间覆盖；把每层 alpha 从 0.4 提到 1，会让一个 texel 在“全亮/最暗”之间切换得更剧烈，并让第一层覆盖立即饱和。若再加单一深度，只会把信号变成更高对比的 binary nearest-caster field，除非先做足够的 prefilter/supersampling 或降低 caster 频率。

**【推荐】最稳妥的第一目标不是“实心叶片”，而是“稳定、可控频率、保留冠层开孔与有限 penumbra 的连续 direct-sun transmittance”。** 先用稳定 grass leaf-shadow anchor 的小范围 A/B 移除 receiver 半边高频相对运动；主修复则以 shadow-only coarse caster LOD / proxy canopy 降低空间频率和叶片独立运动频率：从现有 leaf spray/voxel 数据确定性聚合成较粗的 canopy clusters，近景保留若干稳定 dapple 结构，远景只保留低频 crown transmittance；shadow caster 只跟随 trunk/spray 级风摆，不复制每叶 paddling。继续使用当前独立 leaf transmittance map 作为最小架构接点，但把 receiver 的 `max` 扩张与无条件 fixed-coordinate history 逐步换成 coverage-preserving 空间过滤及 change-aware temporal resolve。这样比直接把叶子塞进 terrain EVSM/PCSS、全局启用 MSAA 或引入 ray-traced multi-layer transmittance 更小、更可测，也更匹配当前资源和 consumer 接口。

## 1. “binary 阴影”应先定义清楚

### 1.1 单叶 coverage 可以 binary，但像素照度不因此 binary

对完全不透明、零厚度叶片，单条 shadow ray 的几何可见性可写成：

```text
V(x, omega) = 0  ray(x, omega) hits an opaque leaf
              1  otherwise
```

但有限 area light 的 direct irradiance 包含对光源立体角的积分：

```text
E_direct(x) = integral_over_sun L(omega) * V(x, omega) * cos(theta) d omega
```

**【推导】即使每个 `V` 都是 binary，只要太阳圆盘不同方向的一部分被叶片挡住，积分结果仍连续落在 0 与全日照之间。** 这就是物理 penumbra，不是透明贴图伪造的“灰影”。Smith 等人的冠层研究以 gap diameter / caster distance 描述这种效应：小角尺寸开口的全日照区很少，penumbral spreading 会显著扩大受部分直射光的区域。[Smith et al. 1989](https://doi.org/10.2307/1938093)

**【外部事实】经典 PCF 论文同样把 depth comparison 先变成 binary，再过滤周围 comparison，得到“多少比例 footprint 被遮挡”的连续值；作者以 9 个 binary test 过滤得到 55% shadow 说明这一区别。** [Reeves, Salesin and Cook, *Rendering Antialiased Shadows with Depth Maps*, SIGGRAPH 1987](https://graphics.pixar.com/library/ShadowMaps/paper.pdf)

因此本任务至少应区分：

- **binary leaf coverage**：叶片轮廓/几何样本是命中或未命中；
- **binary hard-shadow output**：每个 receiver pixel 只输出全亮或全暗；
- **filtered coverage / area-light visibility**：许多 binary samples 的平均，连续但仍由不透明叶片造成；
- **material transmittance**：光穿过叶组织后的波长相关衰减，属于 BTDF/体散射；
- **stylized stable canopy shadow**：为了风格与稳定性主动降低叶影频率、限制运动和对比度，不声称逐叶物理精确。

前三者不能互换。把“单叶 opaque”直接推成“最终 ground shadow 必须 binary”，在物理和采样上都不成立。

### 1.2 真实叶片也不只是 opacity mask

**【外部事实】Wang 等人用真实叶片测量数据构建空间变化的 BRDF 与 BTDF，模型包括叶组织内部的 subsurface scattering 和叶表粗糙散射。** 这证明“叶片表面 coverage 可以 binary”与“叶片对光能完全不透射”是两个独立假设。[Wang et al. 2005](https://graphics.cs.yale.edu/publications/real-time-rendering-plant-leaves)

这并不要求游戏实时算完整光谱 leaf transport。对 Re: Flora，可信的最小近似可以是：

- 空间 coverage 由稳定的 canopy proxy / filtered leaf coverage 决定；
- 阴影最低透射率与颜色是艺术/材质参数；
- penumbra 由有限滤波 footprint 或 PCF/PCSS/area-light sampling 提供；
- 不把一张低分辨率灰 opacity 图称为“逐叶物理精确”。

## 2. 当前精确管线：非实心效果来自哪里

### 2.1 caster geometry 与运动

**【项目事实】visible leaf 并没有 alpha texture 或 alpha test。** 叶簇由固定 seed 的 Perlin noise 在 8–16 voxel 的空心壳中选择离散 voxel；每个 leaf voxel 是独立 instance。visible LOD0 用 unit cube，LOD1 用单 quad；公共 fragment shader 始终输出 alpha 1。见 [`leaves_construct.rs:12-15, 79-127, 154-163`](../src/tracer/leaves_construct.rs)、[`voxel_geometry.rs:3-34`](../src/tracer/voxel_geometry.rs)、[`leaves.vert.slang:76-78`](../shader/slang/leaves.vert.slang)、[`leaves_lod.vert.slang:76-78`](../shader/slang/leaves_lod.vert.slang) 与 [`flora.frag.slang:6-10`](../shader/slang/flora.frag.slang)。因此当前并不存在一张可把阈值从 0.4 改成 0.5 的“leaf alpha mask”；`leaf_shadow_fragment_opacity` 是 shadow-only 每层衰减参数。

**【项目事实】叶影使用专门的 `leaves_shadow` vertex/fragment pipeline，不进入 terrain depth map。** shadow frame plan 对所有非空 leaf/apple batch 强制 `LodState::Lod1`；vertex shader 将每个 leaf voxel 展开成始终朝向 shadow camera、宽度为 `1.225 voxel` 的 billboard。见 [`mod.rs:3381-3423`](../src/tracer/mod.rs)、[`flora_frame_plan.rs:380-397`](../src/tracer/flora_frame_plan.rs) 与 [`leaves_shadow.vert.slang:131-143`](../shader/slang/leaves_shadow.vert.slang)。

**【项目事实】shadow caster 复制可见树叶的 wind-volume sample 和 per-leaf paddling。** `leaves_shadow.vert.slang` 先算 `windOffset = wind * gradient^2`，再加 `leafWindPaddling(...)`；时间还经 4-bucket world-tick 更新。可见 flora 则通过共享的 `prepareFloraVertex()` / `flora_motion.slang` 走同类运动。见 [`leaves_shadow.vert.slang:153-216`](../shader/slang/leaves_shadow.vert.slang)、[`flora_vertex.slang:240-325`](../shader/slang/flora_vertex.slang) 与 [`flora_motion.slang:59-85, 197-213, 272-307`](../shader/slang/flora_motion.slang)。

### 2.2 opacity/depth 写入不是普通 shadow depth

**【项目事实】leaf shadow pass 的 Vulkan pipeline 设置为 `depth_test_enable = false`、`depth_write_enable = false`，且 `rasterization_samples = 1`、`alpha_to_coverage_enable = false`。** 所以所有重叠 billboard 都进入同一颜色附件，当前没有“最近叶片深度赢”的 Z buffer。见 [`pipeline_builder.rs:850-869`](../src/tracer/pipeline_builder.rs) 与 [`graphics_pipeline.rs:176-196`](../crates/re-flora-vkn/src/pipeline/graphics_pipeline.rs)。

**【项目事实】fragment 输出 `float4(lightDepth * opacity, 0, 0, opacity)`；全项目默认 blend 是 premultiplied color 的 `ONE, ONE_MINUS_SRC_ALPHA`，alpha 是 `ONE_MINUS_DST_ALPHA, ONE`。** 从清零附件开始，重叠层的 alpha 递推为：

```text
A_new = a * (1 - A_old) + A_old
      = 1 - (1 - A_old) * (1 - a)
```

所以 n 层相同 opacity `a` 后：

```text
A_n = 1 - (1 - a)^n
```

默认 `a = 0.4` 时，1/2/3/4 层分别约为 `0.40 / 0.64 / 0.784 / 0.8704`。经默认 strength 1.15 和 min transmittance 0.14 后，1/2/3 层对应约 `0.54 / 0.264 / 0.14`；第三层已经触发最暗 clamp。**【推导】这正是当前“薄处浅、厚处深”的连续冠层效果，也解释了为什么很小的重叠层数变化会在草面形成明显的三档亮度跳变；它不是 leaf texture 边缘半透明，而是层数聚合。** 见 [`leaves_shadow.frag.slang:7-12`](../shader/slang/leaves_shadow.frag.slang)、[`graphics_pipeline.rs:183-196`](../crates/re-flora-vkn/src/pipeline/graphics_pipeline.rs) 和 [`gui.toml:1094-1147`](../config/gui.toml)。

**【项目事实】R 通道也按 premultiplied color 合成，receiver 用 `R/A` 恢复一个 blended caster depth。** alpha 合成对同 opacity 层的顺序不敏感，但 R 的 premultiplied source-over 递推依赖 draw order；因此 `R/A` 是顺序加权的近似，不是 nearest depth，也不是按深度排序的 deep transmittance function。见 [`leaves_shadow.frag.slang:7-12`](../shader/slang/leaves_shadow.frag.slang) 与 [`tracer_shadowing.slang:237-245`](../shader/slang/tracer_shadowing.slang)。

Pixar 的 deep shadow map 论文给出了真正 multi-layer transmittance 的定义：沿每条 light ray 收集所有 surface hits，把每次 opacity 作为 `1-opacity` 相乘，得到随深度变化的 visibility function；它还区分 semitransparent surfaces、opaque blockers 的 partial pixel coverage 与 volume attenuation。[Lokovic and Veach, *Deep Shadow Maps*, SIGGRAPH 2000](https://graphics.pixar.com/library/DeepShadows/paper.pdf) 当前单个 `R8G8B8A8` 的 `depth*opacity + opacity` 只存一个压缩深度与一个最终 opacity，不能等价表示这条函数。

### 2.3 temporal、mask 与 receiver composition

**【项目事实】每次 shadow update 都先把 raw leaf opacity attachment 清零并重画；之后 temporal pass 在同一 texel 直接执行 `lerp(previous, current, temporal_alpha)`。** 它没有 motion vector、light-space reprojection、depth/normal rejection、neighborhood clamp 或 variance/confidence。默认 60 fps alpha 0.4，即每帧 history retention 0.6、半衰期约 1.36 帧；应用会按 frame delta 等效调整 alpha。见 [`mod.rs:3373-3423, 3979-3988`](../src/tracer/mod.rs)、[`leaf_shadow_temporal.slang:32-44`](../shader/slang/leaf_shadow_temporal.slang) 与 [`app/core/mod.rs:1562-1569, 3616-3626`](../src/app/core/mod.rs)。

**【项目事实】influence mask 的分辨率是 opacity map 的 1/8；每个 mask texel 检查 3×3 邻域、每格 2×2 sub-samples，取 maximum opacity，再以 `0.003` 阈值写成 binary mask。** receiver 侧先看这个 mask；有效时又在 opacity map 上做 3×3 taps 并取 `max(opacity)`，而不是 coverage average 或 Gaussian/PCF。最后：

```text
leaf_transmittance = clamp(
    1 - max_3x3(opacity) * leaf_shadow_strength,
    leaf_shadow_min_transmittance,
    1)
```

默认 strength 1.15、min transmittance 0.14。raw/history/blended/mask 都是 `R8G8B8A8_UNORM`、linear sampler、只采 LOD 0；opacity/depth-opacity 因而还有 8-bit 量化。见 [`resources.rs:1043-1064, 1756-1845`](../src/tracer/resources.rs)、[`leaf_shadow_mask.slang:23-51`](../shader/slang/leaf_shadow_mask.slang)、[`tracer_shadowing.slang:223-265`](../shader/slang/tracer_shadowing.slang) 和 [`gui.toml:1094-1147`](../config/gui.toml)。

**【推导】当前非实心观感由五项共同造成：** per-layer 0.4 opacity、重叠层 alpha 合成、同 texel temporal average、线性 texture filtering、最终不低于 0.14 的透射率。相反，binary mask 与两级 `max` 不是让影子“更透明”，而是在空间上扩大任何微小 opacity 的影响范围，并倾向保留局部最暗值。

### 2.4 当前 terrain EVSM 与保留的 PCSS 都不会过滤 leaf opacity

**【项目事实】terrain shadow path 是另一张 depth map：dynamic fruit 先 raster depth，compute copy 后与 terrain tracer depth 取近者，再转为 EVSM 正/负指数的一、二阶 moments，做 separable Gaussian blur 和同 texel temporal blend。** PCSS 函数仍保留 16-tap blocker search、16-tap PCF 和 blue-noise rotation/jitter，但当前 `tracer.slang` 和 moisture caller 都传 `DIRECT_TERRAIN_SHADOW_VSM`；没有当前生产 caller 选择 PCSS。leaf shadow 始终单独采样 opacity/mask，再与 terrain、cloud transmittance 相乘。见 [`mod.rs:3425-3465`](../src/tracer/mod.rs)、[`tracer.slang:131-144`](../shader/slang/tracer.slang)、[`tracer_shadowing.slang:123-220, 328-352`](../shader/slang/tracer_shadowing.slang)、[`vsm_creation.slang`](../shader/slang/vsm_creation.slang)、[`vsm_filtering.slang`](../shader/slang/vsm_filtering.slang) 和 [`vsm.slang`](../shader/slang/vsm.slang)。

**【推导】因此调 `vsm_blur_radius` 不会从根本上消除 leaf-opacity field 自身的细碎时变结构。** 将 terrain caller 改回 PCSS 也只改变乘积中的 terrain 项，除非另行把 coarse leaf caster 纳入同一 depth 语义。

## 3. 高频噪声与草上闪烁的根因

### 3.1 空间频率超过 shadow/receiver 采样带宽

叶 voxel billboard 的光空间 footprint 接近或小于 shadow texel 时，raw field 是大量高对比 sub-texel islands。光空间 raster 在创建 map 时已经发生一次离散采样，receiver 又按自身移动的位置采样这张 map；这对应经典 shadow mapping 的两次 aliasing：创建 depth/coverage map 与随后重采样它。Reeves 等人把 shadow-map aliasing 明确分为创建和采样两个问题，PCF 只处理 comparison 后的 footprint filtering。[Reeves et al. 1987](https://graphics.pixar.com/library/ShadowMaps/paper.pdf)

**【项目事实＋推导】当前尺度足以让这种问题成为一阶项。** 世界是 `2×2×2 chunks`、每 chunk `256³ voxels`，flora scale 是 `1/256 world unit per voxel`，即世界边长 2 world units；shadow camera 用这整个 AABB 的 sphere-fit 稳定投影，并按 1024 terrain map 做 texel snapping。由 [`app/core/mod.rs:519-520`](../src/app/core/mod.rs)、[`leaves_shadow.vert.slang:41, 131-143`](../shader/slang/leaves_shadow.vert.slang)、[`shadow.rs:15-63`](../src/gameplay/camera/shadow.rs) 与 [`tracer/mod.rs:130-135, 2634-2644`](../src/tracer/mod.rs) 可算出投影直径约 3.671 world units：terrain texel 约 0.003585 world / 0.918 voxel，2048 leaf texel 约 0.001793 world / 0.459 voxel。shadow billboard 宽 `1.225 voxel`，投影仅约 **2.67 leaf texels**；一个 1-voxel leaf paddling 位移约 **2.18 texels**，草的默认 0.5-voxel vibration 约 **1.09 texels**。也就是说，caster 与 receiver 的单次相对运动本来就足以越过多个叶影采样格。

**【推导】当前 3×3 `max` 并不是低通滤波。** 它是 morphological dilation：会扩张单个高频峰，却不按 footprint 求平均；细岛跨过 texel/sample boundary 时，max 仍可能突变。1/8 mask 又以很低的 0.003 threshold 扩张“可能有叶影”的区域，不能恢复丢失的 sub-texel coverage。

### 3.2 caster 与 receiver 双重运动

当前至少有两套独立时变量：

1. **caster motion**：树叶 shadow billboard 随 wind volume、bucket time 和每叶 paddling 移动；
2. **receiver motion**：草的 vertex positions 随 bend/vibration/wind 移动，导致同一 grass point 每帧投到不同 shadow UV/depth；
3. **camera/display sampling**：若最终 raster 没有可靠 per-vertex motion history，移动草边缘与其着色还会再经历屏幕空间采样。

**【推导】即使树叶 shadow map 在两帧间只平移半个 texel，草尖若同时向相反方向移动，其相对 leaf-shadow coordinate 变化会叠加；高频 transmittance field 的梯度越大，同样位移产生的亮度 delta 越大。** 所以“树影在静态地面尚可、落到摇摆草上明显闪”符合双重运动放大，而不是必须由某一个 alpha 参数独立造成。

**【项目事实】这种运动还是分桶阶跃，不是连续逐帧相位。** 默认 world tick 是 0.05 s、bucket 数是 4；每一 bucket 每 0.2 s 才更新一次（5 Hz），但四组交错后全场每 0.05 s 有一组发生变化（20 Hz）。默认草振动速度为 40/80 rad·s⁻¹、振幅 0.5 voxel；默认叶 paddle 速度为 20/20 rad·s⁻¹、振幅 1 voxel，频率 multiplier 最大 3。见 [`gui.toml:174-183, 1442-1506, 1574-1594`](../config/gui.toml)、[`tracer/mod.rs:130-135`](../src/tracer/mod.rs) 和 [`flora_motion.slang:59-85, 197-213, 272-307`](../shader/slang/flora_motion.slang)。**【推导】一个 bucket 的 0.2 s 间隔会让默认草两项相位分别跳 8/16 rad；叶片在 full frequency response 时可跳到 12 rad。高角速度被量化成 5 Hz 位置阶跃，恰好给高频 shadow coverage 提供了强烈的 temporal step。**

Epic 的官方 Virtual Shadow Maps 文档也把 deformation/foliage 作为专门问题：WPO/PDO 或 skeletal deformation 使 shadow geometry 每帧变化；官方建议在远处关闭 deformation，或切换到不带 WPO 的 mesh LOD/其他低频阴影替代。[Epic, *Virtual Shadow Maps — Deformation and Foliage*](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine)

### 3.3 当前 history 会 smear，但不会正确“跟随叶子”

当前 leaf history 是 shadow-atlas texel-to-same-texel 的 EMA。移动叶片从 texel A 到 B 时：

- A 保留旧 opacity 尾迹；
- B 只逐步长出新 opacity；
- blended depth 也在旧/新 caster 深度之间混合；
- receiver depth gate 用这个混合值做一次阈值判断。

**【推导】这能降低单帧闪烁幅度，却把运动变成 ghost trail、滞后和错误 depth gate；当移动速度大于 history 的可积累速度时，输出不会收敛到稳定细节。** 更高 history retention 会更稳但拖影更长，更低 retention 会更及时但重新暴露 binary/high-frequency switching。

**【项目事实】history validity 目前只有全局布尔状态。** local invalidation 会一起丢弃 terrain 与 leaf history；GUI 中 time-of-day 或 VSM blur 改变会触发它，但 `leaf_shadow_fragment_opacity` 改变不会单独 reset leaf history。正常的 per-leaf wind movement 也没有 per-texel rejection——这不是遗漏一个简单 reset 就能修好的，因为每帧 reset 会完全失去 temporal benefit。见 [`direct_sun_shadow_runtime.rs:24-59`](../src/tracer/direct_sun_shadow_runtime.rs) 与 [`app/core/mod.rs:2857-2867`](../src/app/core/mod.rs)。

生产时域算法通常需要显式处理 reprojection、disocclusion 与 history validity。Epic 的 TAA 课程把 edges in motion、flickering、ghosting 列为基本算法之外必须解决的问题；NVIDIA SVGF 用 temporal accumulation 增加有效样本数，再以几何/variance 引导过滤。[Karis, *High-Quality Temporal Supersampling*, SIGGRAPH 2014](https://www.advances.realtimerendering.com/s2014/index.html)；[Schied et al., *Spatiotemporal Variance-Guided Filtering*](https://research.nvidia.com/labs/rtr/tag/svgf/)

### 3.4 不是所有“噪声”都来自 stochastic sampling

当前 leaf opacity pass 没有 stochastic threshold；raw 闪烁首先是确定性几何/采样 aliasing。terrain PCSS 的 Poisson pattern 会旋转并有 shadow-UV jitter，但 leaf term不走它。**【推导】若没有分别可视化 raw leaf opacity、temporally blended opacity、mask、terrain-only 与 final multiplied transmittance，就容易把 leaf raster alias、PCSS sampling noise、grass motion 和最终 composition 淵称为“噪声”。**

### 3.5 raster depth/composition 之后也没有替 leaf shadow 兜底的 TAA

**【项目事实】raster flora 与 tracer terrain 各自产生 color/depth，composition 以两者 depth 选前景，再按 raster premultiplied alpha 合成；visible leaf alpha 本身是 1。** 默认 internal render scale 是 0.5，post-processing 用整数坐标从 internal texture 取一个 texel映射到输出，没有 reconstruction filter。见 [`composition.slang:90-106`](../shader/slang/composition.slang)、[`composition_scene.slang:16-42`](../shader/slang/composition_scene.slang)、[`app/core/mod.rs:797-809`](../src/app/core/mod.rs) 与 [`post_processing.slang:36-51`](../shader/slang/post_processing.slang)。

**【历史＋项目事实】commit `929d0cb8c446dd1a0286bbf2544974b7e9bdc6ce`（`remove main terrain radiance denoiser`）删除了 terrain motion/normal/position/voxel-id buffers、temporal/spatial denoise，并把 terrain direct shadow 从 stochastic PCSS 改成当前 VSM。当前没有对 moving raster flora 做 screen-space TAA 的 motion vector/history resolve。** 因此 leaf map 的 fixed-light-space EMA 是这类阴影唯一的 temporal 平滑；最终 0.5× 单帧 raster/composition 不会修复移动草上的 shadow scintillation，nearest up-map 还会保留内部像素的亮度跳变。

## 4. 为什么直接 binary alpha test 可能更糟

### 4.1 固定阈值不等于 prefilter

**【外部事实】传统 alpha test 在 alpha boundary 上进行 binary query；即便先对 alpha texture 做 bilinear/mipmap filtering，后面的 threshold 仍把结果变回 binary。** Wyman 与 McGuire 还展示了 coarse mip 下平均 alpha 低于固定阈值时，细线/叶片随距离侵蚀甚至完全消失；传统 alpha test 的 subpixel motion 稳定性最多只是“稳定地 alias”。[Wyman and McGuire 2017](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)

当前 leaf geometry 没有 alpha texture cutout；每个 LOD billboard 的几何 footprint 本来就是 binary covered/uncovered。把 `leaf_shadow_fragment_opacity` 设 1 的直接结果是：

- 第一个覆盖层就把 alpha 累积到 1，失去 canopy layer/open-fraction 信息；
- 低分辨率 mask 和 receiver max filter 会把完全不透明 island 向周围扩张；
- texel 切换的亮度振幅从部分阴影扩大到 `1 - min_transmittance`；
- blended R/A depth 更容易退化为 draw-order 近似，而不是可靠 nearest caster；
- 仍然没有 MSAA、coverage mip 或 PCF footprint average。

所以这只能作为诊断开关，不能直接当修复。

### 4.2 “opaque depth caster + PCF”是另一套完整方案

若 binary 的真实意图是“每条 light sample 遇到一片叶就完全遮挡”，正确的 shadow-map 实现至少要：

1. 叶片写入有 depth test/write 的 shadow depth；
2. receiver 对多个邻域/area-light samples 做 depth comparison；
3. 对 caster minification 提供足够 shadow resolution、MSAA/supersampling 或 coarse LOD；
4. 处理 bias、two-sided geometry、thin-leaf motion和 temporal stability。

PCF 先 comparison 后平均，可产生 filtered binary coverage；PCSS 再用 blocker search 估 penumbra size，按 blocker-receiver separation 扩大 PCF kernel。[Reeves et al. 1987](https://graphics.pixar.com/library/ShadowMaps/paper.pdf)；[Fernando, *Percentage-Closer Soft Shadows*, SIGGRAPH 2005](https://download.nvidia.com/developer/presentations/2005/SIGGRAPH/Percentage_Closer_Soft_Shadows.pdf)

这会比“alpha=1”合理，但 raw per-leaf depth 的空间频率和运动仍在；只有 16 taps 的稀疏 PCF/PCSS 在细密冠层上也可能显示 sampling noise，且代码中保留的 terrain PCSS receiver seed/blue-noise 路径仍需证明能在移动草上稳定。

## 5. 方案比较

### 5.1 总表

| 方案 | 能解决什么 | 主要风险 | 对当前架构适配 | 判断 |
|---|---|---|---|---|
| Opaque / fixed binary cutout | 明确单 sample 遮挡；nearest depth 可正确 gate | 强化高对比 alias；丢失 layer coverage；需重做 depth path | 中：可复用现有 shadow depth/PCF，但 leaf 目前是独立 alpha target | 不单独采用；只配 coarse caster + filtering |
| Alpha-to-coverage + MSAA | 把 alpha 量化成 per-sample coverage，改善 cutout 边缘和动画 scintillation | 只对 multisampled target 有效；coverage mask算法实现相关；重叠层可能相关；需要 per-sample depth/resolve 设计 | 低：当前 pipeline/target 固定 1 sample，A2C off | 非第一阶段 |
| 稳定阈值 / coverage-preserving mip | 保持不同 LOD 的 aggregate covered area，减小远处消失/popping | 不能独自恢复 area-light penumbra；固定 threshold 仍 binary；当前叶影无 alpha texture | 中：概念可用于 canopy occupancy/proxy mip，而非直接套 texture tool | 与 coarse proxy 合用 |
| Coarse shadow-caster LOD / proxy canopy | 在采样前降低空间与运动频率；可保留受控 dapple/open fraction | proxy 过粗会成团块/“云影”；需校准覆盖与 LOD 过渡 | 高：现有 shadow-only pass、frame plan、独立 leaf map 是自然接点 | **首选** |
| Filtered VSM/EVSM/moments | moments 可线性过滤，低成本获得平滑 shadow；EVSM 减轻普通 VSM 部分 leaking | moments 是深度分布近似；light bleeding/numeric instability；不能直接表达多层 opacity-depth function | terrain 已有 EVSM；leaf 直接混入不正确，opaque proxy 才较合适 | 可作后续 coarse depth proxy filter |
| PCF / PCSS | 过滤 binary comparisons；PCSS 提供 contact-hardening penumbra | 多 taps；稀疏采样噪声；raw leaf frequency/双重运动仍在 | 中高：terrain path已有实现，但需 leaf depth integration | coarse binary proxy 后的候选 |
| Stochastic alpha + temporal | 期望上保留 coverage/多层 transparency；样本多时收敛 | 主动引入空间/时间噪声；依赖稳定 hash、reprojection与history acceptance | 低中：有 temporal 资源但无 reprojection/rejection，问题场景正是移动 foliage/grass | 暂缓 |
| Ray-traced transmittance | 可沿 ray 处理多层 alpha/BTDF，配多条 sun rays 得 area-light visibility | leaf AS、any-hit/multi-hit、动态更新、排序/累计、denoise 成本高 | 低：当前 leaf raster opacity 是独立系统，无现成 leaf RT transmittance path | 长期参考，不是本修复 |

### 5.2 Alpha-to-coverage / MSAA

**【外部事实】Vulkan 规范定义 A2C 为：根据 fragment output 0 的 alpha 生成临时 coverage mask，再与原 fragment coverage 相 AND；alpha-to-mask 的具体算法未规定，只要求 1 bits 数量大致与 alpha 成比例，算法甚至可随 framebuffer coordinate 改变。** [Khronos, Vulkan Specification — Multisample Coverage](https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html)

Microsoft 的官方 D3D 文档把 dense foliage 作为 A2C 的主要用例，但也明确：它需要 n-sample render target，硬件可能用 area dithering 换取更好的 alpha quantization，因此带来空间噪声；没有 multisampling 时效果等同单 sample。[Microsoft, *Configuring Blending Functionality — Alpha-To-Coverage*](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-blend-state)

NVIDIA 的 SpeedTree 章节报告 A2C 显著减少动画 cutout 的 scintillation，但同时指出重叠的 50% A2C surfaces 使用相同 mask 时不会像 alpha blending 一样累积，层间相关会使结果偏透明。[NVIDIA GPU Gems 3, *Next-Generation SpeedTree Rendering — Alpha to Coverage*](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)

**【项目适配】当前 graphics pipeline 把所有 raster target 固定为 `TYPE_1`，leaf shadow attachment 也是单 sample R8G8B8A8。** 因此只把 `alpha_to_coverage_enable` 改 true 没有收益；需要扩展 pipeline descriptor、建立 multisampled color/depth、定义 sample resolve 到 transmittance/depth，以及验证其它 pass。对一个 shadow-only opacity target，这已经是中等范围 renderer 变更，不应先做。

### 5.3 Coverage-preserving mip / stable threshold

NVIDIA Texture Tools Exporter 的官方功能说明明确提供 “mipmapped alpha cutout correction”，目标是在不同 LOD 保持近似相同的 cutout covered area。[NVIDIA, *Texture Tools Exporter*](https://developer.nvidia.com/texture-tools-exporter)

Wyman 与 McGuire 则给出更一般的 hashed alpha test：coarse LOD 时逐步把固定 threshold 过渡为 object-space stable hashed threshold，维持 aggregate coverage；论文强调 stochastic threshold 会引入高频时空噪声，而 object-space、离散尺度 hash 可得到接近传统 alpha test 的时间稳定性。[Wyman and McGuire 2017](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)

**【项目适配】当前叶影没有 opacity texture/mip chain，coverage 来自许多几何 leaf voxels。** 最合适的等价物不是凭空做 alpha mip，而是把 leaf voxels 聚合成 object-space canopy occupancy/coverage pyramid，LOD 切换时保存低频 covered area 与平均 transmittance。threshold 要锚定 tree/spray/object space，不能锚定 screen pixel 或 frame index。

### 5.4 Coarse caster LOD / proxy canopy

这是唯一在**产生 shadow samples 之前**直接削弱高频输入的方案。可选 proxy 从轻到重：

- 每个 leaf spray 1–N 个稳定光空间/世界空间 billboard 或 ellipsoid；
- object-space 低分辨率 3D occupancy/density grid，沿太阳方向投影 coverage；
- crown hull/cluster mesh，加低频 holes/noise 控制 dapple；
- near/mid/far 三档：较近保留 cluster holes，中远只保留 aggregate crown opacity。

**【外部事实】Epic 官方 foliage 路线也在远处停止 per-element animation，并用聚合 voxel representation 保存 disconnected foliage 的总体 silhouette/volume；Virtual Shadow Maps 文档建议 foliage 的远处 LOD 关闭 WPO/PDO 或关闭动态细节阴影。** [Epic, *Nanite Foliage*](https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-foliage)；[Epic, *Virtual Shadow Maps — Deformation and Foliage*](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine)

**【项目适配】Re: Flora 已有 `TreeFoliageFramePlan::for_shadow()`、每树 leaf instances、leaf spray/voxel local position、shadow-only draw pass和独立 transmittance consumer。** 因此 proxy 可限制在 foliage shadow producer 边界，不必改可见叶片材质、grass geometry 或 terrain shadow mode。关键是让聚合规则 deterministic、object-space anchored、coverage-preserving，并显式控制 shadow wind topology。

### 5.5 VSM / EVSM / moments

VSM 存一、二阶深度 moments，用 Chebyshev upper bound 估计可见性，因而 moments 可先做普通 texture filtering；原论文目标就是解决 shadow map 难以像颜色纹理一样过滤的问题。[Donnelly and Lauritzen, *Variance Shadow Maps*, I3D 2006](https://doi.org/10.1145/1111411.1111440)

Lauritzen 的官方 GPU Gems 章节同时列出限制：VSM 会 light bleed；深度导数导致过大/不稳 variance 时可能出现随机闪光；variance 的差分计算有数值稳定性问题，推荐线性深度和 32-bit float。[Lauritzen, *Summed-Area Variance Shadow Maps*, GPU Gems 3](https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-8-summed-area-variance-shadow-maps)

EVSM 对正/负指数 warp 后的深度分别做 variance bound，以缓解 VSM 的互补弱点，但仍是 filtered depth-distribution approximation，并带 exponent/precision 约束。[Lauritzen, *Rendering Antialiased Shadows using Warped Variance Shadow Maps*, 2008](https://uwspace.uwaterloo.ca/items/57f02644-02bf-41bf-86e9-8a8dd2858f22)

**【项目适配】当前 terrain 已实现 4-channel EVSM 与 Gaussian/temporal filter。** 若先把 canopy 变成 coarse opaque proxy depth，可以复用或并行复用这类 filtered depth path；但把 raw layered leaf opacity 直接按现有 blend 写进 EVSM moments 没有概率意义。标准 VSM/EVSM 也不能表示“同一 light ray 在多个深度逐层衰减”的 deep transmittance。

### 5.6 PCF / PCSS

PCF 的优点是语义清楚：每个 depth comparison 仍是 binary visibility，最终只是 receiver footprint 内的比例。PCSS 再用 blocker depth 估计 filter radius，产生随 blocker-receiver separation 增大的 plausible penumbra。[Reeves et al. 1987](https://graphics.pixar.com/library/ShadowMaps/paper.pdf)；[Fernando 2005](https://download.nvidia.com/developer/presentations/2005/SIGGRAPH/Percentage_Closer_Soft_Shadows.pdf)

**【项目适配】当前 terrain PCSS 已有 16+16 taps、receiver-plane correction、Poisson rotation 与 shadow UV jitter。** 最小复用方式是让 coarse leaf proxy 写 depth，再让同一 shadow comparison filter包含该 caster；但要先解决 leaf/terrain depth ownership、bias、two-sided billboard、dynamic fruit、god-ray/DDGI consumers，以及当前 leaf min-transmittance/colored transmission 是否仍需要。直接把每个 raw leaf voxel加入 PCSS 会把几何成本与 sampling variance同时推高，不是首选。

### 5.7 Stochastic alpha + temporal

Stochastic Transparency 把 fragment alpha 转成随机 subpixel coverage；期望上得到正确 alpha compositing，并可用于 deep shadow maps，但作者明确说明它引入噪声，需要 alpha correction 与 accumulation。[Enderton et al., *Stochastic Transparency*, IEEE TVCG 2011](https://research.nvidia.com/publication/2011-08_stochastic-transparency)

Hashed alpha 把随机性锚定 object-space，可减少随帧闪烁；但它仍有 spatial noise，论文用 TAA 才能保留远处 aggregate opacity。[Wyman and McGuire 2017](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)

**【项目适配】当前 leaf temporal 只有 fixed-coordinate EMA，没有 caster motion reprojection、disocclusion rejection 或 variance estimate；receiver grass 又独立运动。** 在这条路径上先加入 per-frame stochastic alpha，会把 deterministic alias 变成 history 无法稳定接收的噪声。除非先建立稳定 hash、shadow-space motion/history confidence 和 reference convergence test，否则暂缓。

### 5.8 Ray-traced transmittance

DXR 规范说明 non-opaque geometry 可通过 any-hit shader 执行 alpha test、`IgnoreHit()` 或把 opacity 累加进 payload；但 any-hit 交点执行顺序未定义。Opacity Micromaps 可把 micro-triangle 编成 opaque/transparent/unknown，减少 vegetation alpha any-hit 成本，但它主要加速 coverage classification，不自动提供多层排序、太阳 area sampling 或 denoise。[Microsoft, *DirectX Raytracing Functional Spec*](https://microsoft.github.io/DirectX-Specs/d3d/Raytracing.html) Vulkan 的 OMM 同样定义 2-state microtriangle 为 fully opaque/fully transparent。[Khronos, `VkOpacityMicromapFormatEXT`](https://registry.khronos.org/vulkan/specs/latest/man/html/VkOpacityMicromapFormatEXT.html)

**【项目适配】可信 ray-traced canopy direct light 需要：** leaf geometry/opacity 进入可动态更新的 acceleration structure；每条 shadow ray 正确处理多层 coverage/transmission；多条 sun-disk rays 或其他 area-light estimator；motion-aware temporal/spatial denoiser。当前 leaf shadow producer 是 raster opacity map，尚无这些边界，因此工作量和风险显著高于问题本身。可作为长期 path-tracing reference 或离线 ground truth，不作为第一修复。

## 6. 推荐方案：稳定的 filtered canopy transmittance

### 6.1 目标语义

推荐把“实心/binary”重写为一个可验收 contract：

> 近处每个 shadow sample 仍可来自 opaque leaf coverage；最终 receiver 使用 band-limited canopy visibility/transmittance。shadow caster LOD 在 object space 保持 crown coverage 与主要 holes，过滤掉低于 shadow/receiver Nyquist 的逐叶细节；shadow animation 保留 crown/spray 级摆动，丢弃不可稳定采样的 per-leaf paddling。阴影允许连续 penumbra 与最低 leaf transmission，不强制最终 pixel 0/1。

这保留了可信树影的三项关键 cue：

- crown silhouette 与主要空隙随太阳方向移动；
- 接触较近处可较清晰，远离 canopy 的 dapple/penumbra 更低频；
- 冠层厚处更暗、开口处更亮，但不让 subpixel 叶片在草面上逐帧开关。

### 6.2 分阶段工作

#### Phase 0：隔离证据与基准（0.5–1.5 人日）

不改算法，增加或使用可视化/readback 逐层记录：

- raw leaf opacity/depth-opacity；
- temporal blended opacity；
- 1/8 binary influence mask；
- leaf-only receiver transmittance；
- terrain-only current EVSM；若另做 PCSS 候选，再单独捕获 PCSS-only；
- final terrain×leaf×cloud；
- leaf wind off / grass wind off 的 2×2 matrix；
- fixed camera、固定 sun、normal/high wind 和 camera pan。

先用测量回答：主要 flicker 出现在 raw raster、history、receiver max filter，还是 grass receiver sampling；不要用最终 composite 主观猜。

#### Phase 1：先移除 grass receiver 的高频相对运动（1–2 人日）

只对 **leaf-shadow term** 做一个小范围 receiver policy A/B：草以 rest/root/object-space anchor（或每 instance 一个稳定 anchor）采样 leaf transmittance，而 visible vertex、normal、terrain shadow 与 cloud term继续按现有运动。需要按 blade 高度比较 root anchor、rest voxel center 与低频 bend anchor，避免整株草在冠层 shadow 边界明显“贴图锁死”。

这一步不声称物理更精确；它是风格化的 temporal bandwidth control。优势是直接去掉“moving grass 在 2.7-texel moving leaf islands 上重采样”的一半相对运动，改动面比 renderer/MSAA/history 小，也能通过 Phase 0 的 B/C/D 矩阵明确归因。若 B（leaf on/grass off）仍占主导，Phase 1 不应无限调参，应直接进入 coarse caster。

#### Phase 2：shadow-only coarse caster LOD（2–5 人日）

从现有 per-tree leaf placements 建 deterministic clusters：

- object-space 固定 cell 或现有 leaf spray 为聚合单位；
- 每 cluster 保存 center/radius、covered-area/opacity、可选 depth range；
- 近档保留多个 cluster hole，中远档合并成更粗 crown cells；
- LOD 之间以 coverage-preserving transition，而不是随机逐帧丢叶；
- caster motion 只使用 tree/spray coherent bend；per-leaf paddling 不参与 shadow proxy，或随 projected footprint平滑衰减；
- 继续写当前 leaf transmittance target，暂不动可见叶、grass 或 terrain EVSM/PCSS。

初始 opacity 应由聚合 coverage 校准，不直接等于 1。对独立叶层可用 `1 - product(1-a_i)` 作为离线/CPU 聚合目标，但 proxy overlap 相关性需用固定场景 reference 校准。

#### Phase 3：coverage filter 与 change-aware history（2–4 人日）

- 将 receiver 3×3 `max` 与 mask dilation 拆成明确职责：mask 只做 conservative culling，transmittance 用归一化 coverage filter；
- filter radius 绑定 shadow texel footprint / caster LOD，而不是单一全局魔数；
- history 至少对 opacity/depth 大变化加快 current response，对小变化保留 history；
- 若需要 depth gate，分开存 nearest/front depth 与 aggregate opacity，避免继续把 `R/A` 当严格 caster depth；
- history reset 继续响应 shadow camera/light/extent/scene discontinuity；不在没有 motion field 时声称“正确 reprojection”。

#### Phase 4：按美术目标选择 filter backend（3–7 人日）

- 若希望保留 continuous canopy density：继续 filtered transmittance map，可考虑更稳定的低频 proxy depth/opacity moments；
- 若希望“opaque leaves + contact hardening”：让 coarse proxy 写 depth，复用 PCF/PCSS；
- 若 fixed-width soft shadow 已足够：coarse opaque proxy + EVSM/Gaussian 是可评估候选；
- raw per-leaf binary depth、A2C/MSAA、stochastic alpha 和 ray tracing 只在 Phase 1–3 仍达不到 reference 时另立实验，不与主方案同时扩张。

### 6.3 文件边界（设计预估，不是本轮修改）

优先限制在以下 producer/consumer seam：

- shadow caster policy / batches：`src/tracer/flora_frame_plan.rs`，必要时新增纯 CPU `leaf_shadow_proxy` 模块；
- proxy 数据上传、资源生命周期与 dispatch/draw：`src/tracer/mod.rs`、`src/tracer/resources.rs`、`src/tracer/buffer_updater.rs`；
- pipeline/attachment：`src/tracer/pipeline_builder.rs`；
- caster geometry/motion：`shader/slang/leaves_shadow.vert.slang`，最好新增 proxy 专用 shader，而不是污染 visible `leaves.vert.slang`；
- grass leaf-shadow receiver anchor：`shader/slang/flora_vertex.slang`、`shader/slang/flora_shadow.slang`；只拆 leaf term，不冻结 visible grass motion；
- temporal/filter/mask：`shader/slang/leaf_shadow_temporal.slang`、`shader/slang/leaf_shadow_mask.slang`；
- receiver composition/depth gate：`shader/slang/tracer_shadowing.slang`；
- 参数源只在算法选定后改 `config/gui.toml`，由 `cargo check` 生成绑定，不手改 generated files；
- 固定场景采集/指标建议放 `scripts/` 或 ignored benchmark，不进普通 `cargo test` 的长时 GPU 路径。

避免第一阶段改 visible leaf/grass geometry 与材质、terrain EVSM/PCSS、DDGI consumer 或全局 graphics pipeline multisampling；Phase 1 只拆出 grass 的 leaf-shadow sampling anchor。

## 7. 验收设计

### 7.1 固定测试矩阵

至少保存以下相机/太阳/风状态；所有比较使用相同 release binary、resolution、camera path、sun、seed、warmup 与帧区间：

| 场景 | leaf wind | grass wind | camera | 用途 |
|---|---:|---:|---|---|
| A | off | off | fixed | 证明算法本身确定、无 history drift |
| B | on | off | fixed | 隔离 caster temporal instability |
| C | off | on | fixed | 隔离 moving receiver sampling |
| D | on | on | fixed | 双重运动主验收 |
| E | on | on | slow pan/strafe | camera/display stability |
| F | high | high | fixed + pan | 压力场景，不用于调默认美术值 |

另选三类 receiver：平坦地面、短草、长草；三类 canopy distance：接近枝叶、树下中距、crown 边缘/远距。

### 7.2 量化指标

在固定 light-space 与 screen-space ROI 同时记录：

- `mean transmittance` 与 5/50/95 percentile：防止“更稳”只是整体抹黑或抹亮；
- consecutive-frame absolute delta 的 median/p95/p99；
- temporal variance / temporal power spectrum；
- binary flip ratio：跨阈值像素比例；
- spatial power spectrum：重点比较接近 Nyquist 的 high-band energy 与保留的 mid-band dapple energy；
- 64-frame temporal mean 对高样本 reference 的 RMSE/SSIM；reference 可用超采样 shadow raster、许多 sun-disk rays 或离线 CPU/ray trace；
- shadow centroid/coverage/open-fraction 随 LOD 的连续性；
- GPU scopes：`leaf_shadow_opacity.pass`、temporal/mask、`tracer.shadow_prepass` 与总 `tracer.render` median/p95。

建议先从 Phase 0 baseline 冻结阈值，再设相对 gate；初始候选 gate 可为：

- D 场景 shadow-only ROI 的 frame-delta p95 和 high-band temporal energy **至少降低 50%**；
- A 场景 warmup 后 atlas hash/metrics 完全稳定；
- 相对高样本 reference 的 low-pass transmittance RMSE 不劣化，且平均 crown transmittance 偏差不超过 **5% absolute**；
- 保留 crown 主要 holes：mid-band energy 与 reference 的相对偏差不超过 **20%**，不能用全模糊过关；
- LOD transition 前后 coverage/transmittance 跳变不超过 **3% absolute**；
- release-mode `tracer.render` median/p95 不回退；若画质明显提升但局部 shadow scope增加，必须预先给出 frame budget，而不是用 debug/unit test 证明性能。

这些百分比是第一轮可证伪 gate，不是未经测量的永久标准；Phase 0 若显示 noise floor/自然光变明显不同，应记录基线后调整一次并冻结。

### 7.3 视觉验收

- 静止相机、正常风：草面不出现逐像素“盐胡椒”亮暗开关或沿叶 texel grid 爬行；
- 慢速移动：shadow pattern 连续平移/变形，不出现 history 拖尾双影；
- 树冠边缘仍有可辨识的 dapple/openings，不变成均匀圆形云团；
- 冠层厚处比薄处暗，但不因单片 proxy 进入一个 texel突然整块变黑；
- shadow wind 可以比 visible leaf paddling 更低频，但 trunk/spray 的主要摆动方向和相位不能完全脱节；
- 地面与草上的同一低频 shadow feature 运动一致，允许草自身 shading/normal 改变，不允许 leaf term 额外 scintillate；
- 检查 above-canopy/self-shadow receiver，防止 coarse depth gate 把叶上方物体错误遮蔽。

## 8. 风险与粗略工作量

| 风险 | 触发条件 | 控制方式 |
|---|---|---|
| proxy 变成“云影” | 聚合过粗、只保留 crown hull | 多级 clusters；以 reference 的 mid-band/open-fraction gate 约束 |
| shadow 与 visible leaf 脱节 | 完全冻结 caster wind | 保留 trunk/spray coherent motion，只衰减 per-leaf paddling |
| grass shadow “贴根/锁死” | stable receiver anchor 过低频 | 只稳定 leaf term；比较 root/rest/low-frequency bend anchor；以 blade-height 边界场景验收 |
| 过度变暗 | max dilation + proxy overlap + strength>1 | 改 normalized coverage filter；跟踪 mean transmittance |
| history 拖影 | fixed texel EMA 继续吃大变化 | change-aware alpha/depth confidence；大变化快速采 current |
| depth gate 错误 | blended `R/A` 当 nearest depth | 分离 nearest/front depth 与 aggregate opacity，或限定 receiver scope |
| LOD popping | 按距离突然换 proxy | coverage-preserving cross-fade / deterministic transition |
| EVSM light bleed/闪光 | moments 混合多深度、precision不足 | 只对 coarse opaque depth 使用；32-bit/线性深度；固定 artifact 场景 |
| PCSS sampling noise | raw 高频 caster + taps太少 | 先 coarse caster；固定 pattern/temporal reference；profile taps |
| A2C 范围扩张 | 为局部 leaf target改全局 MSAA | 独立实验管线；不改全局默认；明确 resolve/depth语义 |
| RT 动态更新成本 | per-leaf AS 每帧 deform | 只作为 reference；长期再评 proxy BLAS/OMM |

粗略工作量：Phase 0 0.5–1.5 人日；Phase 1 1–2 人日；Phase 2 2–5 人日；Phase 3 2–4 人日；Phase 4 的单个 EVSM/PCF/PCSS 候选 3–7 人日。A2C/MSAA 独立试验约 4–8 人日；完整动态 ray-traced multi-layer transmittance + sun sampling + denoise 至少 2–4 周，且可能更长。均不含美术反复与跨平台性能回归。

## 9. 推荐决策

1. **不接受“真实树影必须是 final binary pixel”前提。** 接受的是 opaque leaf samples；最终 canopy irradiance/transmittance 应允许 filtered coverage、penumbra 和受控 transmission。
2. **不把 `leaf_shadow_fragment_opacity = 1` 当修复。** 它只适合作为 root-cause discriminator：若闪烁振幅明显增大，反而验证 high-frequency binary switching。
3. **先做稳定 grass leaf-shadow anchor 的小范围 A/B，再以 shadow-only coarse caster LOD/proxy + coverage-preserving filter 作为主修复。** 前者低成本移除 receiver 半边相对运动；后者从源头把 caster 信号带宽压到 shadow/receiver 能稳定采样的范围。不能只做 root-lock 然后把静态地面/camera 下的 caster alias 留着。
4. **在 receiver/caster 输入稳定后再修 temporal/depth 语义。** 当前 fixed-coordinate EMA 和 blended `R/A` depth 是已知近似；先让输入低频，再决定是否值得加入 change-aware history或独立 front depth。
5. **PCF/PCSS 或 EVSM 只对 coarse depth proxy 做候选 A/B。** 前者语义更接近 opaque leaf coverage + area filtering，后者更贴合当前 filtered shadow 资源；两者都不应直接吞 raw per-leaf layered opacity。
6. **A2C、stochastic alpha、ray tracing 暂不进入主线。** A2C 与当前 1× target 不兼容；stochastic 会在 history 不完备时增加噪声；RT 是架构级扩展。

## 10. Git history：最可能的“移到现在这个时间版本之前”

### 10.1 精确边界是 `1ebb4f89` 的父提交

`git log/show` 给出一个非常清楚的语义断点：

| commit | 日期 | 语义变化 |
|---|---|---|
| `226ee2b19e4f007c675c5ccd44c18fb4fdc74c5d` | 2026-06-03 00:33 +08:00 | `1ebb4f89` 的直接父提交；`git describe` 为 `v0.2.11-3-g226ee2b1`，仍是旧 binary depth 行为 |
| `1ebb4f895ca6cf770e026ad9844dd48c650863fc` | 2026-06-03 00:50 +08:00 | `add leaf shadow opacity path`：叶片退出主 depth render pass，改写独立 color opacity target，关闭 depth test/write，并增加 1/8 influence mask |
| `08a7a63e` | 2026-06-03 | `add leaf shadow temporal blending`：加入当前 fixed-coordinate EMA 的前身 |
| `b4d443e2` → `36bd676d` | 2026-06-03 | temporal alpha 先从 0.9 降到 0.08（当时记录“0.9 更 flicker”），再定为 0.4 |
| `32c68f0e` | 2026-06-03 | 加入 `depth*opacity` 与 `R/A` approximate depth gate |
| `c60b5946` | 2026-06-03 | terrain VSM 从 2048 降到 1024，leaf opacity 保持 2048、mask 256 |

**【历史事实】在 `226ee2b1`，`shader/foliage/leaves_shadow.frag` 是空 `main()`；`leaves_shadow_lod_ppl` 画进 `render_pass_depth`，公共 pipeline descriptor 开启 depth test/write。** 随后 `shadow_depth_copy` 把最近的 raster foliage depth复制到 R32F，terrain tracer 与它取 `min`，再进入 EVSM creation、Gaussian blur 与 temporal history。也就是说，旧版本的“binary”精确含义是：**每个 LOD1 billboard 的 light-space coverage 写一个 opaque nearest depth；最终仍经 EVSM 空间/时间过滤，绝不是最终 receiver pixel 强制 0/1。**

因此若用户口中的“现在这个时间版本之前”是指独立 opacity 迁移之前，最可复现的 A/B 基线就是 `226ee2b1`，而不是任意更老 tag。它位于 `v0.2.11` 之后、`v0.2.12` 之前。

### 10.2 旧 binary 路径并不是无风稳定参考

历史还排除了一个容易误判的假设：

- `55ce18c9b711f549c4b9edd960f97e93c6bdb9a1`（2026-04-28，`bucket leaf wind by voxel`）已经让叶片按 voxel 使用独立 wind seed；
- `c550b5dd5cd2b326ee9a1f0d08cb74a6186836e1`（2026-06-02，`bucket flora vibration timing`）已经把 leaf 与 grass motion 时间冻结到 bucket update；
- `226ee2b1` 的旧 `leaves_shadow.vert` 已明确调用 per-voxel `sample_wind_volume(...)` 与 `leaf_wind_paddling(...)`；
- `7a5cf0fca68ee8dd853c948e1aad0643b63d41db` 让 shadow map 每帧重画，`ede62a26eca3d54fdf720565d0fb3a45d1558dea` 随后增加 VSM temporal history；`c0165711ac06f53f0fbdad6aa89561157210176c` 则启用了 linear VSM sampling，`e68e5e4c1b94f09d9b0c138409953cb9d8b5138c` 稳定了 shadow projection bounds。

**【结论】恢复 `226ee2b1` 的 opaque depth 只能回答“旧的 filtered binary coverage 风格是否更喜欢”，不能证明它天然消除当前的高频/双重运动。** 它仍有同一类 per-leaf caster motion 与 moving grass receiver；其可能更稳的部分来自当时统一 2048 depth→EVSM blur/history 的带宽限制，也可能更糟，因为 opaque coverage 的单次切换振幅更大。正确实验应在当前代码做受控 A/B，并同时捕获 raw depth/coverage 与 final transmittance，而不是回退整个版本后凭印象比较。

### 10.3 后续历史解释了当前没有 screen denoiser 的原因

`929d0cb8c446dd1a0286bbf2544974b7e9bdc6ce`（2026-07-29，`remove main terrain radiance denoiser`）删除 terrain temporal/spatial denoiser及 motion/geometry history，并把 terrain direct light 的 caller 从 stochastic sun-disk PCSS 改成 VSM。PCSS helpers 保留为后端能力，但当前生产 terrain caller不再使用。这与第 2.4、3.5 节的当前代码一致。

### 10.4 本轮未做的取证

本轮边界是调研与设计，没有加入 instrumentation、没有跑可见游戏，也没有生成新的 release benchmark。以下量仍应由 Phase 0 测量，而不能从静态代码伪装成实测结论：raw/blended/mask 各自对最终 flicker 的百分比、`R/A` draw-order 对具体场景的误差、以及 proxy 候选的实际 GPU 代价。这不影响已由代码和历史确认的因果链：**高频 moving caster → 1×/8-bit light-space raster → 无 motion-aware history → moving grass 重采样 → 无 screen TAA 兜底。**

## 一手资料索引

- W. K. Smith, A. K. Knapp, W. A. Reiners, *Penumbral Effects on Sunlight Penetration in Plant Communities*, Ecology 70(6), 1989：<https://doi.org/10.2307/1938093>
- L. Wang et al., *Real-Time Rendering of Plant Leaves*, SIGGRAPH 2005（Yale 作者页与论文）：<https://graphics.cs.yale.edu/publications/real-time-rendering-plant-leaves>
- W. T. Reeves, D. H. Salesin, R. L. Cook, *Rendering Antialiased Shadows with Depth Maps*, SIGGRAPH 1987（Pixar 作者库）：<https://graphics.pixar.com/library/ShadowMaps/paper.pdf>
- T. Lokovic, E. Veach, *Deep Shadow Maps*, SIGGRAPH 2000（Pixar 作者库）：<https://graphics.pixar.com/library/DeepShadows/paper.pdf>
- C. Wyman, M. McGuire, *Hashed Alpha Testing*, I3D 2017（NVIDIA Research）：<https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf>
- E. Enderton et al., *Stochastic Transparency*, IEEE TVCG 2011（NVIDIA Research）：<https://research.nvidia.com/publication/2011-08_stochastic-transparency>
- W. Donnelly, A. Lauritzen, *Variance Shadow Maps*, I3D 2006：<https://doi.org/10.1145/1111411.1111440>
- A. Lauritzen, *Summed-Area Variance Shadow Maps*, GPU Gems 3：<https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-8-summed-area-variance-shadow-maps>
- A. Lauritzen, *Rendering Antialiased Shadows using Warped Variance Shadow Maps*, University of Waterloo thesis, 2008：<https://uwspace.uwaterloo.ca/items/57f02644-02bf-41bf-86e9-8a8dd2858f22>
- R. Fernando, *Percentage-Closer Soft Shadows*, SIGGRAPH 2005（NVIDIA）：<https://download.nvidia.com/developer/presentations/2005/SIGGRAPH/Percentage_Closer_Soft_Shadows.pdf>
- Vulkan Specification，Multisample Coverage：<https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html>
- Khronos `VkOpacityMicromapFormatEXT`：<https://registry.khronos.org/vulkan/specs/latest/man/html/VkOpacityMicromapFormatEXT.html>
- Microsoft Direct3D 11，Alpha-To-Coverage：<https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-blend-state>
- Microsoft DirectX Raytracing Functional Spec：<https://microsoft.github.io/DirectX-Specs/d3d/Raytracing.html>
- NVIDIA Texture Tools Exporter，mipmapped alpha cutout correction：<https://developer.nvidia.com/texture-tools-exporter>
- NVIDIA GPU Gems 3，*Next-Generation SpeedTree Rendering*：<https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering>
- B. Karis, *High-Quality Temporal Supersampling*, SIGGRAPH 2014（课程作者页）：<https://www.advances.realtimerendering.com/s2014/index.html>
- C. Schied et al., *Spatiotemporal Variance-Guided Filtering*（NVIDIA Research）：<https://research.nvidia.com/labs/rtr/tag/svgf/>
- Epic Games，*Virtual Shadow Maps — Deformation and Foliage*：<https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-shadow-maps-in-unreal-engine>
- Epic Games，*Nanite Foliage*：<https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-foliage>
