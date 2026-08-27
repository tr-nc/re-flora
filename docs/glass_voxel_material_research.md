# 玻璃体素材质可行性与渲染架构调研

状态：调研与目标架构建议；不包含实现
日期：2026-08-23
审计基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`
主线复审：2026-08-27，合入 `main@400794afca0b8d7dd3fbf44e2804a53354cca29f`

> 面向实施的中文 HTML 指南：[`glass_voxel_software_rt_implementation_guide.html`](glass_voxel_software_rt_implementation_guide.html)。
> 外部引擎、论文、DDGI 与 pinned source 的逐条调研：
> [`glass_voxel_software_rt_external_research.md`](glass_voxel_software_rt_external_research.md)。

> 2026-08-27 技术选型：不采用 Vulkan/DXR 硬件光追、BLAS/TLAS 或 ray query。目标路线固定为
> 当前 raster + compute/software ray tracing。文中硬件 ray-query 内容只保留为被否决方案的能力
> 边界，不属于实施路线。

## 结论

在当前世界中加入“厚度为一个体素、可反射和折射的玻璃”**可行，但不属于好加的材质开关**。

- **编码和存档容易**：权威 atlas 的低 4 bit 能表达 16 个 type，当前使用
  `0, 2..8`，其中主线已把 ID 8 分配给 Emissive；因此 Glass 的下一个可用 ID 是 9，仍不需要扩大
  每体素 1 byte。编辑统计数组当前有 9 项，加入 Glass 时至少需要 10 项，建议直接扩到 16；存档
  schema 是否升级必须按透明材质的旧版本误读风险明确决策，不能只增加一个 shader 常量。
- **纯体素玻璃中等偏难**：现有 Contree 只保存 active surface leaf，`marchScene` 也只返回
  第一个表面命中。它足以加速找入射面，却不能可靠找连续玻璃的出射面。最干净的做法是
  “Contree 找第一个界面 + 权威 `chunk_atlas` dense DDA 穿越介质 + 再回到场景查询”。
- **厚玻璃不能当 alpha 混合**：一体素厚仍是有入射面、传播距离和出射面的体积；需要
  Snell、介电 Fresnel、全反射（TIR）和按实际路径长度计算的 Beer-Lambert 衰减。相邻同材质
  玻璃体素应被视作一个连续体，不能在内部格子面重复反射。
- **混合 raster 场景最难**：当前 raster vegetation/objects 先与 terrain depth 共用一个
  RGBA/depth，再由 composition 只按一层 raster depth 与一层 compute depth 合成。若 compute
  depth 写玻璃前表面，terrain depth prefill 会在 raster 绘制前把玻璃后的植被/物件剔掉；之后
  再做 screen-space 折射已经没有颜色可采。
- **推荐路径**：先生成完整的 opaque voxel + raster HDR/depth，再用独立玻璃 resolve 读取它并
  写另一个 HDR target；体素反射/折射用世界空间查询，raster 内容先用完整 opaque scene 的
  screen-space 采样，离屏/被遮挡 raster 内容以体素/天空回退。这个 hybrid 有明确近似边界，
  能落地但不是统一正确解。
- **选择的正确性边界**：在不建设硬件 TLAS/ray-query 场景的前提下，软件 voxel query 能正确处理
  voxel 世界的离屏/被遮挡内容；raster flora/objects 的 secondary visibility 只能做 screen-space
  近似并在缺失时回退到 voxel/sky，不能宣称为统一正确解。这是已接受的产品边界，不再把硬件 RT
  列作后续实施阶段。

以一位熟悉本渲染器的工程师估算：可演示的 voxel-only 厚玻璃约 **1.5–2.5 周**；推荐的可发布
hybrid（含存档、opaque composition 重排、raster 近似、阴影/DDGI 一致性与验证）约
**3–6 工程周**。这里的工时是设计量级，不是排期承诺；GPU 性能必须按仓库约定以固定场景
release-mode 实测，硬件 ray-query 路线不计入计划。

## 范围与正确性目标

这里的“一体素厚”指一个有体积的 voxel cell/slab，而不是没有厚度的窗片。即使只有一个 cell，
斜射线在其中的距离也可能显著大于一个 voxel edge；折射光线还会发生横向位移。PBRT 的 thin
dielectric 模型会聚合薄片内部反射，但明确假设忽略这种空间位移，因此不满足本任务的厚介质目标
（[PBRT, Dielectric BSDF](https://pbr-book.org/4ed/Reflection_Models/Dielectric_BSDF)）。

第一版正确性建议定义为：

1. 支持空气与一种均匀玻璃介质；玻璃材质提供固定 IOR 与 RGB 吸收系数。
2. 对 voxel 世界做几何正确的进入/退出、Snell、介电 Fresnel、TIR 和传播距离衰减。
3. 同材质 face-connected 玻璃不产生内部界面；空气夹层和分离的玻璃壳按实际 material transition
   处理。
4. specular caustics、粗糙玻璃、色散、参与介质散射以及多种重叠介质不进入首版。
5. raster 几何的折射/反射允许明确标注的 screen-space 近似，但绝不能宣称它能看见离屏或被
   遮挡的 raster 内容。

## 当前权威体素编码与存档

### 8-bit atlas 的实际布局

`shader/slang/voxel_data.slang` 的 `VOXEL_TYPE_MASK = 0x0F`，低 4 bit 是 type；`0x30` 与
`0xC0` 分别是 2-bit moisture 和 fertility。`packVoxelAtlasData()` 将三者装入同一个 byte。
CPU 侧在 `src/builder/plain/mod.rs` 以 `VOXEL_TYPE_MASK`、`VOXEL_MOISTURE_MASK`、
`VOXEL_FERTILITY_MASK` 和 `pack_voxel_atlas_byte_with_fertility()` 重复同一 ABI。

当前 material ID 定义在 `shader/slang/voxel_types.slang` 和
`src/builder/plain/mod.rs`：

| ID | 含义 | 状态 |
|---:|---|---|
| 0 | Empty | 已用 |
| 1 | 未定义 | 空洞 |
| 2 | Dirt | 已用 |
| 3 | Sand | 已用 |
| 4 | Stucco | 已用 |
| 5 | Cherry wood | 已用 |
| 6 | Oak wood | 已用 |
| 7 | Rock | 已用 |
| 8 | Emissive | 已用（2026-08-27 主线） |
| 9..15 | 未定义 | 可编码 |

因此 byte 宽度**无需改变**。建议分配 `Glass = 9`，而不是回填 ID 1：schema v1 最初加入时
ID 1 就已经空着（`938cbe5156babd02f63b1d3a11dc6117d03368ca` 同时引入
`src/terrain_persistence.rs`，该提交的 `shader/slang/voxel_types.slang` 已使用 `0, 2, 3,
5, 6, 7`）。主线随后把 ID 8 分配给 Emissive；保留历史空洞并顺序使用 ID 9，比回填 ID 1 更
清楚。

但 type nibble 的容量不等于系统已经支持 16 种材质：

- `src/builder/plain/mod.rs::EDIT_STATS_VOXEL_TYPE_COUNT` 与
  `shader/slang/chunk_writer_types.slang::EDIT_STATS_VOXEL_TYPE_COUNT` 目前都是 9；若用 ID 9，统计
  buffer 至少要扩到 10，建议直接扩到 16，令数组容量与编码域一致。现有
  `max_removed_counts_8_11` 已提供第二个四项 ABI 槽，但 readback 数组和有效计数仍需同步扩大。
- `shader/slang/surface_extraction.slang::isSolid` 将任何非零 type 当 solid；
  `isOccluded` 只看六邻域 occupancy。这会让玻璃参与几何遮挡，却不会自动赋予透明语义。
- terrain connectivity、CPU occupancy/collision 与其他“非零即实心”的调用点会把玻璃当实体。
  这通常符合玻璃墙碰撞，但必须成为材质属性，而不能继续依赖散落的 `type != 0`。
- 种植、soil state 和 smoothing 多数使用 Dirt/Sand/Rock 白名单；玻璃不会自动可种植，这是好事。
  inventory/backpack、编辑统计、harvest particles 和 palette 则需要显式加入。
- `src/builder/contree/mod.rs::acoustic_material_for_voxel` 对已知 type 做映射；Glass 若不加入会
  落入默认声学行为。

moisture/fertility 是 soil state，不应挪作每个玻璃 voxel 的 IOR/roughness。玻璃的 IOR、
吸收和渲染策略应在 material table 中按 type 查找；Glass 的高 4 bit 建议规范为 0 并在写入路径
保证，避免通用 non-empty fill 把默认 fertility 写进无意义状态。这样仍保留未来将 type nibble 与
state nibble分别演进的空间。

### schema 与向前/向后兼容

`src/terrain_persistence.rs` 当前定义：

- `TERRAIN_FORMAT_VERSION = 1`；
- `TERRAIN_VOXEL_SCHEMA_ID = 1`；
- `TERRAIN_BYTES_PER_VOXEL = 1`；
- `TerrainSnapshotMetadata::validate()` 要求 schema 与当前常量完全相等。

`docs/terrain_persistence_v1.md` 规定 payload 是权威 atlas 的每一个 byte，包含全部 type 与 state；
Surface/Contree、scene/collision、water、DDGI 等都是 load 后重建的派生物。当前格式有 schema
字段，但 v1 明确不实现跨 schema migration。

只在 schema 1 中新增 Glass 会形成危险的单向兼容：新程序可以读取所有旧存档，因为旧 ID 与
byte 不变；但旧程序也会接受新存档，并把未知的 Glass byte 当作非空实体，通常呈现为黑色/默认
材质或在不同子系统产生不一致语义。这不是安全的 forward compatibility。

推荐策略：

1. 二进制容器布局不变，因此 `TERRAIN_FORMAT_VERSION` 仍可为 1；writer 改写
   `voxel_schema_id = 2`。
2. reader 支持 schema 1 和 schema 2 两个 adapter：schema 1 接受当前主线已有 ID（含
   Emissive = 8）并逐 byte 映射到相同 ID；schema 2 接受 Glass = 9，并验证 type 与该 type 允许的
   state bits。
3. 当前 schema 1 文件不需要重采样，也不需要改 payload；载入新程序后首次保存自然写 schema 2。
4. 老程序看到 schema 2 会按既有 `validate()` 逻辑拒绝，而不是误解释玻璃。这是有意且安全的
   forward-compatibility 边界。
5. load 的预校验继续在 atlas mutation 前完成；未知 ID、Glass 非规范 state 和截断文件都必须
   保持当前世界不变。

这需要修改当前“只接受一个编译期 schema 常量”的 reader，但不需要改变一体素一 byte 的
权威数据模型。

## 当前几何、加速与 tracer 路径

### Surface/Contree 是表面壳，不是介质体

权威数据是 filled `chunk_atlas`。派生链路为：

```text
chunk_atlas byte
  -> surface_extraction.slang::extractSurfaceVoxel
  -> contree_leaf_write.slang
  -> Contree node/leaf buffers
  -> update_scene_tex.slang 的 chunk -> node/leaf offset
  -> scene_marching.slang::marchScene
```

`surface_extraction.slang::isOccluded` 会删除六面都被非空 voxel 包围的 cell；不同非空 type 相邻
时也仍按 occupancy 处理。写进 leaf 的 surface word 只有 type、平滑 occupancy normal 和 2-bit
hash（由 `voxel_data.slang::packVoxelSurfaceData` 打包）。`scene_tex` 是 R32G32_UINT 的 3D chunk
索引纹理，不是 Vulkan acceleration structure。

`shader/slang/marching_result.slang::MarchingResult` 只含首个命中的 position、center、distance、
平滑 normal、type、hash 与 leaf address。`shader/slang/scene_marching.slang::marchScene` 做 chunk
DDA 后调用自定义 `marchContree`，最多 `MAX_DDA_ITERATIONS = 256`，命中一个 active leaf 即返回。
它没有：

- axis-aligned 几何 face normal；
- incident/current medium；
- entering/exiting 标志；
- 介质内传播距离；
- 下一个 material transition；
- 多界面事件列表。

因此 Contree 适合找“射线遇到的第一个可见 cell”，不适合单独承担玻璃 entry 后的 exit。连续
玻璃内部 cell 甚至可能没有 surface leaf；同样，从当前 leaf 上做 epsilon 后再次调用 first-hit
也容易自交、跳错 cell 或把平滑法线误当几何界面法线。

### 当前主 tracer 是 opaque first-hit

`shader/slang/tracer.slang::generalSceneMarching` 调用 `marchScene`。正常 primary path 一次命中后
以 palette albedo、direct sun/shadow 和 DDGI 做 opaque shading；miss 输出黑/天空路径与 depth 1。
文件里的 `pathTracingIndirectIrradiance` 是最多 8 bounce 的 diffuse cosine reference path：每次仍
只取 opaque first hit，throughput 乘 albedo，不包含 dielectric BSDF、medium state 或 raster
geometry。

`shader/slang/tracer_shadow.slang` 同样取 first hit 并写单一 shadow depth；
`shader/slang/tracer.slang::shadowRayColor` 的正常 direct path 组合 VSM、leaf 与 cloud shadow。
Glass 若只加入 type table，会自动变成完全不透明 shadow caster，而不是带色透射。

### 现有 Vulkan RT 辅助代码不是生产能力

`crates/re-flora-vkn/src/rtx/acceleration_structure/` 有 BLAS/TLAS 构建辅助类型，descriptor reflection
也识别 `ACCELERATION_STRUCTURE_KHR`。但生产 logical device 的
`crates/re-flora-vkn/src/context/device.rs::device_extension_requirements()` 只要求 swapchain、
maintenance4 与 deferred-host-operations；同文件的 `create_device()` 只 push maintenance4 和
buffer-device-address features，并且**没有启用**
`VK_KHR_acceleration_structure`、
`VK_KHR_ray_query` 及其 feature structs，生产 shader 也没有 `rayQueryEXT` 路径。

所以不能把 `rtx/` 目录当作“已有硬件 RT，只差调用”。本项目已明确不采用这条路线；这些辅助类型
不应进入 Glass 实施依赖，也不应为它预留生产 interface。

## 当前 raster、composition、depth 与 post-processing

### 单层 raster target 的信息损失

`src/tracer/mod.rs::record_trace_after_shadow_prepass` 的相关顺序是：

```text
voxel tracer
  -> one graphics render pass
  -> god rays / clouds / lens flare
  -> composition
  -> post_processing
```

`record_all_graphics_passes()` 开始 graphics render pass 后，首先用
`terrain_depth_prefill_ppl` 把 compute terrain depth 写入 D32 hardware depth，再画 flora、leaves、
apples、pipes/sprinklers、preview、probe visualization、dynamic fruit 与 particles。多数实体是
opaque；water droplets 使用预乘 alpha，并在 CPU 按距离排序，但仍与其他内容共享同一个 raster
RGBA/depth。`crates/re-flora-vkn/src/pipeline/graphics_pipeline.rs::new()` 为 graphics pipeline
配置 `ONE / ONE_MINUS_SRC_ALPHA` blend，depth compare 为 LESS；各 pipeline 的 depth-write 选择
决定最终最前层 depth。

`src/tracer/extent_dependent_resources.rs` 中 raster color 是 `R8G8B8A8_UNORM`、raster depth 是
D32、voxel color/depth 是 R32_UINT RGBE + R32F，最终 `composited_tex` 才是 RGBA16F。
`shader/slang/composition_scene.slang::combineSceneColors` 只比较一层 raster depth 与一层 compute
depth，然后以 raster alpha 做一次 over；`shader/slang/composition.slang` 把结果写成 alpha 1。

这意味着 raster pass 一旦把多层颜色混成一个 RGBA，只留下最前 depth，后续无法恢复各层的
depth、材质与运动。特别是：

- 若 voxel tracer 把 Glass 前表面写为 compute depth，terrain depth prefill 会在 raster shading
  前剔掉玻璃后的 flora/object；玻璃 resolve 无颜色可折射。
- 若 tracer 完全跳过 Glass，raster 后方内容能画出来，但 composition 不再知道 glass front/exit
  位于哪里，也无法正确区分 raster 在玻璃前还是后。
- 一个 raster depth 无法同时表达“玻璃前的水滴、玻璃、玻璃后的水滴”。任何现阶段方案都必须
  对 raster translucency 给出限制，或保存更多层。

### 旧 terrarium glass 当前不可达

`src/tracer/mod.rs::record_trace_after_shadow_prepass` 虽注释“terrarium glass 在 composition
解析合成”，实际固定 `let enable_glass = false`；`shader/slang/composition.slang` 虽 import
`composition_terrarium_glass`，却没有调用
`shader/slang/composition_terrarium_glass.slang::applyTerrariumGlass`。因此当前基线没有可达的
terrarium glass 渲染路径。

该文件里的 screen-space offset sampling/SSR 与解析 Fresnel 只能作为历史原型证据，不能当成现成
通用玻璃方案；它没有 authoritative voxel medium traversal，也不能恢复已被 depth prefill 剔掉的
raster 内容。

### post 与 temporal

`shader/slang/post_processing.slang` 从 `composited_tex` 读取 scene-linear HDR，随后 tone-map 和
dither。玻璃 resolve 必须在 tone mapping 前；折射最终 display color 会重复 tone-map，也破坏能量
关系。

当前没有统一 scene TAA。已有 history 分散在 DDGI、VSM/leaf/cloud shadow、cloud/god ray/lens
等路径。若玻璃使用随机 Fresnel branch 而没有玻璃专用 accumulation/reprojection，会直接闪烁；
screen-space refraction/SSR 还会在屏幕边缘、depth discontinuity、disocclusion 与运动植被处 popping。
若以后做 reprojection，至少需要 glass front depth/normal、前后帧矩阵、terrain/material revision 和
raster motion/validity；terrain edit 必须使相关 history 失效。第一版应使用确定性的有界 split，
不依赖 temporal accumulation。

## 厚玻璃的物理与离散规则

### 界面：Snell、Fresnel 与 TIR

每次 material transition 先用**格子几何面法线**确定朝向和 `eta_i / eta_t`。PBRT 给出的向量折射
关系会在进入/退出时交换相对 IOR；当折射解不存在时即发生 total internal reflection，只有反射分支
（[PBRT, Specular Reflection and Transmission](https://www.pbr-book.org/4ed/Reflection_Models/Specular_Reflection_and_Transmission)）。

对 dielectric 应计算 s/p 偏振平均的精确 Fresnel（最终只保留非偏振标量即可），得到 `F`：

```text
L = F * L_reflection + (1 - F) * T_medium * L_refraction
```

TIR 时 `F = 1`。平滑 occupancy normal 可以继续用于现有 diffuse 外观，但 Snell/TIR 若使用它会
改变 cell 边界拓扑，造成错误退出与漏光；介质求交必须使用 dense DDA 给出的 axis-aligned face
normal。未来若要 rough glass，应在已正确的几何界面上另加 microfacet BTDF，而不是先模糊 traversal；
Walter 等人的原始论文给出了 rough surface reflection/transmission 模型
（[Walter et al., 2007](https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.pdf)）。

### 体内：Beer-Lambert

均匀吸收介质沿实际世界距离 `d` 的 RGB transmittance：

```text
T_medium = exp(-sigma_a * d)
```

transmittance 沿连续区段相乘；不能用固定 alpha 代替。PBRT 从 extinction 推导了这个指数形式
（[PBRT, Transmittance](https://pbr-book.org/4ed/Volume_Scattering/Transmittance)）；Khronos
`KHR_materials_volume` 同样要求按光线在体内的实际距离计算衰减，并指出 ray-traced entry/exit
优于 thickness texture 近似
（[KHR_materials_volume](https://raw.githubusercontent.com/KhronosGroup/glTF/main/extensions/2.0/Khronos/KHR_materials_volume/README.md)）。

`sigma_a` 应以 world-unit 为单位。美术参数可用“attenuation color at attenuation distance”表达，
加载时转换成 `sigma_a`，但 shader 内部保持统一物理单位。

### 连续、嵌套与起点在玻璃内

采用 atlas material transition 而非 surface leaf transition：

- 从 Air cell 进入 Glass cell：发生 air→glass 界面。
- Glass→相邻 Glass：只累计距离，不发生 Fresnel；整片 connected glass 是一个 union。
- Glass→Air：发生 glass→air，可能 TIR。
- Glass→Opaque：结束 transmitted path，并在 opaque hit 求值；首版不试图模拟不透明基底内部。
- Glass→Air gap→Glass：按四个实际界面依次处理，因此一体素空气夹层也成立。

对当前“Air + 一种 Glass、cell 互不重叠”模型，ray state 只需当前 medium ID，不需要公开一个通用
stack；当前位置所属 atlas cell 就是事实来源。camera 起点在 Glass 内时，从 atlas 初始化 medium，
第一次 transition 自然是 exit。未来支持多种 IOR、重叠 mesh volume 或优先级介质时才引入
medium stack/priority；`KHR_materials_volume` 对 closed/manifold volume 与 nested medium 的约束可作
后续参考。开放/破损玻璃结构则需要最大事件数、最大距离和 fail-safe，不允许无限内部反射。

### 分支预算

在每个界面同时递归 reflection + refraction 会令 ray 数指数增长。推荐：

- realtime primary：确定性求一个 reflected 与一个 transmitted 贡献；transmitted branch 内允许
  有界的 TIR/exit 事件（例如 4–8），用 throughput cutoff 停止；不要在每个二次界面继续完整二叉树。
- reference path mode：按 Fresnel 概率随机选 reflection/refraction，并配合 Russian roulette；这可
  保持估计无偏，但在当前无玻璃 history 的画面里会噪声/闪烁，所以只用于参考验证。

## voxel tracer 需要的数据与 pass

### `VoxelMaterial` 深 Module

当前 material knowledge 分散在 CPU type 常量、shader palette、surface solid 判断、soil 白名单、
collision、sound、shadow 与 DDGI。建议把 seam 放在“encoded byte → declared semantics”：

```text
VoxelMaterial interface
  decode/validate byte
  surface class: empty / opaque / dielectric
  occupancy: collision / surface extraction / probe relocation
  optical: IOR, sigma_a, roughness policy
  lighting: DDGI visibility, direct-shadow policy
  gameplay: soil/edit/inventory/acoustic policy
```

实现可在 Rust 与 Slang 各有一个生成自同一权威声明的 adapter，或先保持两个镜像并加 ABI 测试；
重点不是创建一层 pass-through，而是让所有调用者不再各写 `type != 0` / `type == glass`。其 interface
应只暴露材质语义，不暴露 bit hack。

**删除测试**：如果删除该 Module，glass skip/solid/state/optical 分支会重新散落到 tracer、surface、
shadow、DDGI、persistence、collision 和 editor，复杂度在多处重现，因此它有 Depth、Locality 与
Leverage，不是浅封装。

### `SceneQuery` 深 Module

第二个 seam 是“给定 ray 和查询策略，返回可继续的场景事件”：

```text
trace_first_surface(ray, mask) -> SurfaceHit | Miss
walk_voxel_medium(ray, starting_medium, budget) -> MediumSegment / InterfaceEvent / Hit
trace_radiance_branch(ray, policy) -> bounded radiance result
trace_sun_transmittance(ray, policy) -> RGB transmittance
```

其 implementation 内部组合 Contree adapter（快速 entry/opaque surface）与 dense atlas DDA adapter
（material transition/几何法线/距离）。调用者不应知道 entry 用哪个结构、epsilon 如何推进或连续
同介质如何合并。测试通过同一 interface 喂 authored atlas，断言事件序列和颜色结果；
DDA/Contree 之间的 seam 留在 implementation 内部，不为已否决的 TLAS 建立假想 interface。

**删除测试**：若删除它，primary、reflection、refraction、shadow、DDGI 会分别复制 traversal、
medium state 与 epsilon 规则，正是本改动最大的长期风险。当前只有 voxel adapter 时不必公开一个
假想的通用 port；等生产 TLAS 与测试 atlas 确实形成两个 adapter 后，外部 seam 才值得扩展。

### GPU 数据

最低需要：

1. shader material table：`surface_class`、IOR、`sigma_a.rgb`、shadow/DDGI policy；type ID 作为索引。
2. dense atlas 的只读访问与 world↔chunk/cell 转换；不能只绑定 Contree leaf buffer。
3. ray medium state：current medium、origin/direction、throughput、累计距离、interface count；DDA hit
   需返回 cell coordinate、from/to material、face normal 和 segment distance。
4. primary 输出拆分：first opaque voxel color/depth，外加 nearest glass event（front depth、entry
   position/normal/material，或足够在 resolve 中确定性重建）。
5. hybrid resolve 所需 unified opaque scene HDR/depth；若需 glass 后 raster screen sample，还要保持
   未被 glass depth 剔除的 raster opaque color/depth。
6. debug/统计：glass pixels、DDA steps、interface events、TIR count、secondary rays、budget exhaustion。

### 推荐 frame passes

```text
1. voxel primary
   - Glass 不作为 opaque depth 终点
   - 输出 first opaque terrain RGBE/depth
   - 输出 nearest GlassFront/Event

2. raster opaque
   - terrain_depth_prefill 使用 first opaque terrain depth
   - 画 flora / objects，得到 opaque raster HDR/depth

3. opaque scene composition
   - 合成 terrain + raster + sky/cloud，得到 UnifiedOpaqueHDR + depth provenance

4. glass resolve（独立 compute/fullscreen pass）
   - 若 front 前有 raster opaque，直接保留前景
   - 对 glass pixel 做 voxel medium walk + reflected/transmitted voxel query
   - 对能映射回屏幕的 raster 后景采 UnifiedOpaqueHDR/depth
   - 对离屏/被遮挡 raster 缺失使用 voxel/sky fallback，并标记这是近似
   - 读取 UnifiedOpaqueHDR，写另一个 HDR target（ping-pong，禁止同图读写 hazard）

5. raster translucency / screen effects（明确受支持的排序）
6. tone map + dither
```

玻璃必须在 unified opaque composition **之后**，否则 raster 背景不完整；必须写另一 target，否则
常规 sampled-image + storage-write 会产生反馈 hazard。当前 raster 是 RGBA8，若反射/折射要保留
高亮，opaque raster target 也应评估 RGBA16F，而不是等到 composition 才第一次进入 HDR。

## raster / hybrid 方案比较

alpha blending 不是厚介质。Khronos transmission 规范明确区分 coverage alpha 与物理 transmission，
并指出实时渲染至少应让 opaque objects 能透过 transmission 看见；thin transmission 也不产生宏观
折射位移
（[KHR_materials_transmission](https://raw.githubusercontent.com/KhronosGroup/glTF/main/extensions/2.0/Khronos/KHR_materials_transmission/README.md)）。

| 方案 | 能解决什么 | 不能解决什么 | 屏幕外/被遮挡内容 |
|---|---|---|---|
| 排序 forward transparency | 对不相交、可正确 back-to-front 排序的 alpha surface 做标准 over；可在 opaque pass 后采 scene-color copy 做折射近似 | intersecting layers、逐像素顺序、厚度/exit、物理 Fresnel/Beer；object-level 排序不等于 fragment 排序 | **不能**；只采已有屏幕颜色 |
| deferred 后独立 forward transparent pass | 是当前架构最干净的 raster seam：先完整 opaque/deferred，再 forward shade transparent；可独立控制 depth write 与 resolve | 仍需排序/OIT；screen-space refraction 仍不补几何；传统 deferred G-buffer 不适合直接混合透明层 | **不能**，除非另接 world-space query |
| Weighted Blended OIT | 固定少量 MRT + resolve、无需严格排序，适合大量非折射 alpha 粒子/叶片的平滑近似 | 原论文明确是 heuristic，不能准确表达 colored/refractive glass，紧密的不同色深度层会失败；没有 entry/exit | **不能**；只重组当前视图 fragments（[McGuire & Bavoil 2013](https://www.jcgt.org/published/0002/02/09/)） |
| Depth peeling | 每 pass 剥一层；N pass 得到当前视图最多 N 个排序 fragment layers，能做更准确的 alpha composite | 成本约随层数/重画增长；原始方法针对 non-refractive transparency，不自动生成折射 ray 或体积距离 | 只看当前相机实际 rasterized 的 layers；**不能**看视锥外内容（[Everitt 2001](https://developer.download.nvidia.com/assets/gamedev/docs/order_independent_transparency.pdf)） |
| per-pixel linked-list OIT | 保存当前视图所有/有界 fragments 后排序，层信息比单 RGBA/depth 完整 | 内存上限、overflow、同步与带宽；仍只是 primary-view fragments，不是 secondary-ray scene | **不能**；Khronos sample 也把它定义为 OIT fragment capture（[Vulkan OIT sample](https://docs.vulkan.org/samples/latest/samples/api/oit_linked_lists/README.html)） |
| 当前 custom voxel secondary trace | 可查询视锥外、被 raster 前景遮挡的 **voxel terrain/sky**；dense atlas 可给真正 exit/thickness | 看不到未进入结构的 flora/objects；Contree-only 又缺介质内部 | 对 voxel **能**，对 raster **不能** |
| Vulkan ray query + unified TLAS（已否决） | 若建设完整场景结构，理论上可查询离屏/被遮挡 raster geometry | 与既定软件光追路线冲突，且必须建设 BLAS/TLAS、动态 flora 更新和 hit shading；不进入实施 | 不适用；仅作方案边界对照（[VK_KHR_ray_query](https://registry.khronos.org/vulkan/specs/latest/man/html/VK_KHR_ray_query.html)、[Vulkan AS spec](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html)） |
| 推荐 hybrid | voxel 介质 world-space 正确；完整 opaque raster 可在 screen-space 折射；缺失时有 voxel/sky fallback | raster secondary visibility 仍是近似；透明 raster 多层必须限制或另加 OIT/layers | voxel **能**；raster 仅当前完整 opaque screen **能**，离屏/被遮挡 **不能** |

结论：WBOIT/depth peeling 解决的是 raster transparency ordering，不是厚玻璃的 secondary visibility。
它们可以服务于第 5 pass 的 droplets/particles，却不能替代 `SceneQuery`。只有 world-space geometry
query 才能看到屏幕外或 primary view 被遮挡的内容。

## DDGI、shadow 与其他派生路径

### DDGI

当前三条规则互相绑定：

- `shader/slang/ddgi_probe_trace.slang` 用 `marchScene` 的 first hit 作为 opaque radiance surface；
- `shader/slang/ddgi_voxel_visibility_pack.slang` 对每个非 Empty atlas voxel 设置 exact visibility bit，
  `ddgi_voxel_visibility.slang` 将占用变成 0 visibility；
- `shader/slang/ddgi_probe_relocate.slang` 只把 Empty 当可用空间。

若只让 probe trace 跳过 Glass，却仍在 exact visibility bits 中把它视为 opaque，会产生不一致漏光/暗边。
第一阶段应选择并集中声明一种近似：

- 推荐：Glass 对 diffuse probe transport 与 exact visibility **透过/跳过**，但 probe relocation 仍视为
  solid，避免 probe 被放进玻璃 cell。这近似为无折射、无吸收的 diffuse 光通过玻璃。
- 可接受但更暗的临时诊断：三条 visibility 路径都把 Glass 当 opaque；不得只改其中一条。

真正的 dielectric indirect、彩色衰减与 caustics 是独立研究项目，不应塞进首版。Glass 也不能以
黑色 diffuse albedo 参与 bounce，那会错误吸收全部间接光。

### direct shadow

`tracer_shadow.slang` 的单一 nearest depth 和现有 VSM 无法表达多层玻璃的 RGB transmittance。
阶段一建议 Glass 对 direct shadow 一致跳过，即“无玻璃阴影”的声明式近似。下一阶段再选择：

1. receiver-side `trace_sun_transmittance`，沿 sun ray 累积 Fresnel/Beer；质量高但每 receiver 增加查询；
2. 独立 RGB transmittance shadow map/pass；适合主光，但需要处理多层与分辨率泄漏。

specular caustics 不属于两者；不能把一张带色 shadow map 称为 caustics。

### acceleration rebuild、collision 与编辑

Glass 仍是 filled atlas 中的实体，现有 terrain mutation/load revision 会触发 Surface/Contree、scene、
collision/water terrain cache、DDGI/shadow 派生重建。需要通过 `VoxelMaterial` 明确：

- surface extraction：Glass 是可见界面，但 Glass-Glass 内部 face 不产生介质事件；
- physics/water：Glass 默认 solid；
- terrain connectivity：Glass 是否计入“支撑地形”必须由 gameplay 决定，不能因非零偶然继承；
- probe relocation：solid；diffuse visibility 与 direct shadow 则按阶段策略。

## 分阶段实施建议

以下工时按一位熟悉当前 renderer 的工程师、已有 review 环境估算，包含相应测试与性能仪器但不含
等待产品评审；阶段 gate 未通过就不继续扩大范围。

### Phase 0：语义与存档 seam（3–5 天）

目标：Glass byte 能被安全创建、保存、载入和拒绝，不改变渲染。

文件边界：

- material authority：`shader/slang/voxel_types.slang`、`shader/slang/voxel_data.slang`、
  `src/builder/plain/mod.rs`，以及建议新增的窄 material declaration/generation 文件；
- edit/inventory/acoustic 明确映射：`shader/slang/chunk_writer_types.slang`、相关 builder UI/inventory
  调用点、`src/builder/contree/mod.rs::acoustic_material_for_voxel`；
- persistence：`src/terrain_persistence.rs` 与其 tests、`docs/terrain_persistence_v1.md` 的后继 schema
  文档（不要改写 v1 历史含义）。

Gate：schema 1 fixture 原样载入；schema 2 Glass round-trip；旧/未知 ID、非法 state 在 mutation 前
失败；old binary 通过 schema mismatch 安全拒绝新存档。风险低，性能可忽略。

### Phase 1：voxel-only 厚介质（7–12 天）

目标：固定测试场景中，terrain/sky 的 reflection/refraction 具有正确 entry/exit、TIR 与 Beer 衰减；
暂不承诺 raster secondary visibility。

文件边界：

- traversal：`shader/slang/scene_marching.slang`、`shader/slang/marching_result.slang`，建议新增
  `voxel_medium_marching.slang` / `scene_query.slang`；
- shading：`shader/slang/tracer.slang`，material table/ABI；
- CPU resource binding 与 profiler：`src/tracer/mod.rs`、pipeline/resource builder；
- focused shader/CPU reference tests 放入现有 test convention，不把 implementation details 暴露为
  新公共 interface。

Gate：一 cell slab 正/斜入射、camera inside、连续 2+ cells、空气夹层、TIR、玻璃贴 opaque、最大
event budget 全部 deterministic；与 CPU double-precision reference 在容差内一致。主要风险是 DDA
epsilon、自交、chunk seam 与 ray 数爆炸。

### Phase 2：opaque hybrid composition（8–15 天）

目标：能透过玻璃看到屏幕内 opaque flora/objects；前景 raster 不被玻璃覆盖；scene-linear HDR 中
解析玻璃。

文件边界：

- outputs/resources：`src/tracer/extent_dependent_resources.rs`；
- pass orchestration：`src/tracer/mod.rs::record_trace_after_shadow_prepass`、
  `record_all_graphics_passes`；
- shader：`shader/slang/composition.slang`、`composition_scene.slang`，建议新增独立
  `glass_resolve.slang`，不要复活 terrarium-specific `applyTerrariumGlass` 作为通用 interface；
- pipeline manifest/reflection source 与 profiler scopes；生成文件只由正常生成流程产生。

Gate：玻璃前/后的 raster opaque occlusion 正确；折射 UV 越界稳定回退；HDR highlight 不被 RGBA8
提前裁掉；terrain depth prefill 使用 first opaque depth；所有 read/write image transitions 明确。
主要风险是 pass 重排、带宽、raster/voxel depth provenance 和屏幕边缘 artifact。

### Phase 3：lighting、translucency 与 temporal hardening（5–10 天）

目标：把声明的 DDGI/direct-shadow policy 在所有消费者中做一致；规定 droplets/particles 与 Glass
的排序；编辑、相机切换与分辨率变化无 history ghost。

文件边界：

- `shader/slang/ddgi_probe_trace.slang`、`ddgi_voxel_visibility_pack.slang`、
  `ddgi_voxel_visibility.slang`、`ddgi_probe_relocate.slang`、
  `ddgi_exact_sun_visibility.slang`；
- `shader/slang/tracer_shadow.slang`、`tracer_shadowing.slang` 及 shadow pass orchestration；
- particle/water transparent draw split、history invalidation 与 debug views。

Gate：Glass edit 后所有 derived revisions 收敛；无一条 DDGI/shadow consumer 私自使用不同 solid
规则；透明 raster 的受支持排序写进测试/文档。若产品要求“玻璃前后多层水滴都正确”，此 phase
必须升级为 depth peeling/PPLL 等独立项目，工时另计。

### 不进入路线：统一硬件 ray-query scene

用户已明确否决 Vulkan/DXR 硬件 RT、BLAS/TLAS 和 ray query。Phase 0–3 的 interface、资源生命周期、
测试与工期不得依赖该路线，也不为假想的未来 adapter 增加抽象。由此接受的产品边界是：voxel
secondary visibility 由现有软件场景查询负责；raster geometry 仅保证当前完整 opaque screen 中的
近似，离屏或被遮挡内容稳定回退到 voxel/sky。

## 风险与性能预算

### 主要风险

1. **语义散落**：某个 DDGI/shadow/collision path 忘记 Glass 特例。以两个深 Module 收敛，并用
   deletion test 审查，优先级最高。
2. **退出面错误**：拿 Contree smooth surface normal 或 repeated first-hit 代替 atlas transition。
   必须以 authored cell sequence 测试 dense DDA。
3. **raster 背景已丢失**：Glass depth 过早进入 prefill。架构 gate 是“先完整 opaque scene，后
   glass resolve”。
4. **secondary ray 爆炸**：界面递归二叉树。固定 event/ray budget、throughput cutoff、debug counter。
5. **screen-space 过度承诺**：折射 UV 越界、被遮挡 raster、屏幕后方永远没有数据。必须提供显式
   fallback/debug mask，并把能力写入产品验收。
6. **temporal popping**：随机 branch、动态 flora、编辑 invalidation。第一版确定性，不加无 motion
   vector 的积累。
7. **schema 半升级**：新 Glass 写进 schema 1 会让旧 binary 误读。writer schema 2 与 dual reader
   是上线前硬 gate。

### 粗略 GPU 成本

- 无玻璃像素应保留现有 fast path；玻璃像素最坏为 primary + reflection + transmission 三次场景
  traversal，再加介质内 DDA/TIR 事件。成本与屏幕覆盖率、平均 interface count 近似线性，而不是
  只与玻璃 voxel 数量相关。
- 额外 full-screen glass resolve 带来至少一次 HDR read/write。1920×1080 RGBA16F 单图约
  15.8 MiB，两个 ping-pong target 约 31.6 MiB 常驻，并增加每帧带宽；不含 event/depth targets。
- WBOIT 需要多个 MRT + resolve；depth peeling 大致按 peeled layers 重画 geometry；PPLL 消耗按
  fragments 增长。这些不应为只有少量玻璃像素的首版默认开启。
- ray query 还包含 BLAS/TLAS build/update、存储与 traversal divergence，动态 flora/wind 的更新策略
  是最大未知数。

必须新增 `tracer.glass`、`composition.glass`、DDA steps/event count 与 glass screen coverage 的
GPU/统计 scope。以固定 camera、固定 snapshot、固定 internal resolution 做 release A/B；记录 GPU
frame time 与 relevant pass 的 median/p95，不把 debug build 或单次帧当性能证据。

## 验证计划

### 纯逻辑/reference

- exact Fresnel 在 normal incidence、grazing 与 `eta_i == eta_t`；Snell 向量与临界角/TIR；
- Beer-Lambert：0 distance、已知 attenuation distance、分段相乘等于总距离；
- atlas DDA：正/负方向、轴平行、chunk seam、起点在 face、起点在 Glass、最大距离；
- material sequence：Air-G-Air、Air-G-G-Air、Air-G-Air-G-Air、G-Opaque；同材质内部 event 数为 0；
- schema 1 fixture、schema 2 round-trip、schema mismatch/unknown ID/非法 state 的 no-mutation failure。

### end-to-end 视觉与日志

固定 snapshot/camera 至少包括：一 cell 窗、斜角长路径、两 cell 连续玻璃、空气夹层、camera inside、
TIR 角、玻璃后 terrain、玻璃后 flora、raster 在玻璃前、屏幕边缘折射、terrain edit。每个场景保存
确定性截图/hash 与 counter；hybrid fallback 用 debug color 显示 screen-space miss，避免把缺失内容
误判成物理结果。

实现阶段按仓库规则运行 `cargo fmt --check`、`cargo check`、focused tests 和 hidden muted release
smoke，并检查同 worktree 的 run log。性能判断另跑足够长的固定 release workload；shader ABI/生成
文件只通过正常 `cargo check` 更新。

## 最终建议

批准时应把需求拆成两个承诺：

1. **必须正确**：权威 voxel 的厚介质 entry/exit、Fresnel/Snell/TIR/Beer，以及存档 schema 安全；
2. **明确近似**：raster 内容先限于 unified opaque screen，离屏/被遮挡 raster 不保证；DDGI 和
   direct shadow 采用声明式阶段近似，不包含 caustics。

采用 `VoxelMaterial` + `SceneQuery` 两个深 Module、opaque-first + glass-resolve 的 pass seam，可以让
首版在 raster + 软件光追的既定架构内落地，并把 screen-space raster fallback 的限制保持显式。相反，
若直接在 `tracer.slang`、每个 shadow/DDGI shader 和现有 `composition.slang` 各加 `if glass`，编码
虽然一天内可见，之后会在存档、exit traversal、raster depth 与 lighting 中同时失真，不能称为
完成了“一体素厚、有反射和折射的玻璃材质”。

## 一手资料

- Pharr, Jakob, Humphreys, *Physically Based Rendering, 4th ed.*：
  [Specular Reflection and Transmission](https://www.pbr-book.org/4ed/Reflection_Models/Specular_Reflection_and_Transmission)、
  [Dielectric BSDF](https://pbr-book.org/4ed/Reflection_Models/Dielectric_BSDF)、
  [Transmittance](https://pbr-book.org/4ed/Volume_Scattering/Transmittance)、
  [Media](https://pbr-book.org/4ed/Volume_Scattering/Media)。
- Khronos glTF extensions：
  [KHR_materials_volume](https://raw.githubusercontent.com/KhronosGroup/glTF/main/extensions/2.0/Khronos/KHR_materials_volume/README.md)、
  [KHR_materials_transmission](https://raw.githubusercontent.com/KhronosGroup/glTF/main/extensions/2.0/Khronos/KHR_materials_transmission/README.md)。
- Walter et al., *Microfacet Models for Refraction through Rough Surfaces*, EGSR 2007：
  [原始论文](https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.pdf)。
- McGuire & Bavoil, *Weighted Blended Order-Independent Transparency*, JCGT 2013：
  [论文与作者材料](https://www.jcgt.org/published/0002/02/09/)。
- Everitt, *Interactive Order-Independent Transparency*, NVIDIA 2001：
  [原始论文](https://developer.download.nvidia.com/assets/gamedev/docs/order_independent_transparency.pdf)。
- Khronos Vulkan：
  [VK_KHR_ray_query](https://registry.khronos.org/vulkan/specs/latest/man/html/VK_KHR_ray_query.html)、
  [Acceleration Structures](https://docs.vulkan.org/spec/latest/chapters/accelstructures.html)、
  [Depth](https://docs.vulkan.org/guide/latest/depth.html)、
  [Framebuffer blending](https://docs.vulkan.org/spec/latest/chapters/framebuffer.html)、
  [OIT linked-list sample](https://docs.vulkan.org/samples/latest/samples/api/oit_linked_lists/README.html)。
