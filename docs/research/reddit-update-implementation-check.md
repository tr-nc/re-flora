# Reddit 开发回顾：实现核对

## 证据范围

核对版本：游戏源码 `adde77ac`，当前依赖 PetalSonic **0.9.2**（`Cargo.toml`／`Cargo.lock`）。
本轮只做源码与历史文档核对，没有重新做画面、听感或匹配性能验收。
用户提供的 RTX 3060 Ti 体验单独记为用户观察，不转成未经测量的 FPS／百分比。

## 上次 Reddit 帖子

搜索 `site:reddit.com "Re: Flora" voxel` 找到疑似帖子：

[I'm officially open-sourcing my work-in-progress rust + Vulkan voxel game](https://www.reddit.com/r/VoxelGameDev/comments/1s9rwez/im_officially_opensourcing_my_workinprogress_rust/)，位于 r/VoxelGameDev。

搜索摘要提及 Rust／Vulkan、GPU compute 64-tree voxel path tracer、实时地形和 flora 编辑、GPU instancing、spatial audio 等。
这只是搜索摘要，不是已读取的完整原帖。正文提取失败，JSON 请求也返回 Reddit 的 “You've been blocked by network security” 页面；
未确认作者、日期或是否为用户最近一次发帖。需要用户确认链接或提供正文，才能严谨写“相较上一次新增”。
不能因为摘要中已经有 spatial audio 就说以前已有当前的几何反射／混响。

## 1. DDGI 与动态编辑

**已确认：** 当前不是只能给 ray-traced 地形使用 GI。草／植物层和树叶在 compute pass 中采样环境 irradiance，随后光栅绘制消费缓存；树枝光栅 shader 也采样 DDGI。

源码入口：

- [`DdgiRuntime`](../../src/ddgi/runtime.rs)：完整 field／volume 的身份、调度、发布与消费者资源所有权；`publish_ready_volume` 不把半成品直接交给消费者。
- [`flora_vertex.slang`](../../shader/slang/flora_vertex.slang)：`sampleFloraEnvironment`。
- [`flora_lighting_cache.comp.slang`](../../shader/slang/flora_lighting_cache.comp.slang)：按实例／层计算 irradiance。
- [`tree_leaf_lighting_cache.comp.slang`](../../shader/slang/tree_leaf_lighting_cache.comp.slang)：按叶体素计算 irradiance。
- [`raster_tree.frag.slang`](../../shader/slang/raster_tree.frag.slang)：`sampleDdgiTerrainSmoothEnvironment`。

持续编辑时允许完整光照场渐进发布，而不是总等到松开编辑才更新；文档记载的 40 次编辑场景有 22 次编辑期间发布。
但发布可用的完整数据 **不等于即时完全收敛**。当前地形直接使用采样 irradiance × albedo，不再用编辑区域固定颜色兜底；未收敛估计仍可能闪动或偏暗。
详见 [`terrain_edit_lighting.md`](../terrain_edit_lighting.md) 与 [`cave_edit_lighting.md`](../cave_edit_lighting.md)。

**推荐宣传口径：** “实时更新的探针式全局光照，也覆盖光栅化植被；持续编辑期间仍能更新并使用完整光照结果。”
“没有旧逐像素路径追踪的颗粒噪声”可以作为观感描述，但不要写成“动态编辑永远无闪烁、零延迟、完美 GI”。
相比旧 path tracing 更快、质量更好暂归用户观察；本轮没有同场景 A/B 性能证据。

## 2. 声音：反射与混响均已实现

### 游戏侧做了什么

- [`spatial_sound_manager.rs`](../../src/audio/spatial_sound_manager.rs)：初始化原生 HRTF、48 kHz 音频 world，**固定开启 environmental acoustics**，传入声学场景与预算。
- [`ContreeBuilder`](../../src/builder/contree/mod.rs)：从 CPU Contree 数据发布带版本的声学快照；体素材质提供三频段吸收、散射、透射等声学属性。
- [`App`](../../src/app/core/mod.rs)：更新后调用 `publish_acoustic_scene`；不是把可变 GPU 场景交给音频实时线程。
- [`spatial_frame.rs`](../../src/audio/spatial_frame.rs) 与 sound manager：发布完整 listener／emitter 状态。
- [`canopy_distributed_emitter_adapter.rs`](../../src/audio/canopy_distributed_emitter_adapter.rs)：把树冠空间分布接到音源，而非只用树中心一个点。

### PetalSonic 0.9.2 实际求解

本轮阅读的是 Cargo registry 安装的 0.9.2 源码，可通过 [版本源码入口](https://docs.rs/crate/petalsonic/0.9.2/source/src/) 按以下文件／函数追溯：

| 层次 | 当前实现 | 不应夸大的部分 |
| --- | --- | --- |
| 异步传播 | `acoustic_propagation.rs` 的 `AcousticWorker` 启动 `petalsonic-acoustics`；捕获场景／空间输入，发布带身份的 response，并丢弃被取代的求解 | 不是每个音频采样都重新追踪整个世界 |
| 直接声 | 在有预算的几何查询中估计遮挡／透射与频段增益 | 不同声音可配置直接声策略；蝉鸣有绕过直接透射的诊断策略，不能说所有声音完全相同 |
| **早期反射** | `solve_early_reflections` 从听者发出 Fibonacci 分布探测射线，连接命中表面与优先音源代表点，做可见性检查；按路径长度、方向、材质吸收／散射计算 tap，保留有限个强反射 | 当前早期路径是单次表面反射的近似筛选，不是任意多跳、全音源精确反射 |
| **后期混响** | `solve_late_reverb` 从听者追踪多次反弹，累计三频段能量与路径时间，估计 RT60、pre-delay 与 wet gain | 追踪用于估计环境参数，不是构造完整精确声学脉冲响应 |
| 实时播放 | `spatial/processor.rs` 渲染带延迟、方向与频段增益的早期 taps；`spatial/late_reverb.rs` 的 `ThreeBandFdn` 用共享、听者中心的 **8 条 delay-line／三频段 FDN** 合成混响尾音 | FDN 是混响合成器；不能称为所有反弹声波的逐样本物理模拟 |

早期音源、反射 taps、ray 数和 bounce 数均有预算；质量设置改变工作量，不是每棵树／每个声源都无上限求解。

**推荐表达：** “原生几何驱动的空间声学：异步 ray/path tracing 计算早期反射与环境混响参数，再由 HRTF 和三频段 FDN 实时播放。”
玩家版本可以说：“墙面和空间形状开始真正影响声音的反射与混响。”

旧 [`native_path_traced_audio_research.md`](../native_path_traced_audio_research.md) 核对的是 **0.6.0**，其中“单 tap、没有 late reverb”是历史基线，不能用于描述当前版本。

## 3. 整树光栅绘制、风与声音

**已确认：** 当前整树采用光栅路径，树干／枝条的层级姿态跟随共享风场；子枝继承父枝变形端点，叶片绑定跟随，树冠声音也消费共享风场。
GPU 层级求解／表面 skinning 避免靠 CPU 每帧重建整树几何；交互查询和碰撞更新保留一致的姿态语义。

入口：[`pose.rs`](../../src/tree_gen/pose.rs)、[`tree_pose.comp.slang`](../../shader/slang/tree_pose.comp.slang)、
[`tree_skin.slang`](../../shader/slang/tree_skin.slang)、[`whole_tree_rasterization_progress.md`](../whole_tree_rasterization_progress.md)、
[`canopy_distributed_emitter_adapter.rs`](../../src/audio/canopy_distributed_emitter_adapter.rs)。

- “我的 RTX 3060 Ti 上整树随风仍达到实时帧率”保留为用户体验，不附未经确认的帧率。
- “随风声更新”更准确写为 **运动与沙沙声由同一风场驱动**，不是声音信号驱动树的动画。
- **踹树待确认：** 本轮检索输入、树姿态、physics 和 shaders，未找到玩家 kick／冲量注入树层级的链路。当前弹簧风响应不能作为踹树证据。请确认操作方式／分支／提交或演示。

## 4. 孤立体素清理

发现的生产实现是 **地形连通性**，不是屏幕孤立像素滤镜：
[`terrain_connectivity.rs`](../../src/app/core/terrain_connectivity.rs) 在地形编辑松开时分析六邻接连通分量，并可在载入世界时检查。
脱离支撑的选定体素通过 [`detachment.rs`](../../src/app/core/terrain_connectivity/detachment.rs) 从 atlas 移除并生成视觉粒子，随后更新物理与可见地形。
跨分析边界会继续核对连接，避免把仍连接主地形的块误删；分析与粒子容量有预算，不能承诺所有尺寸即时全部清理。

建议写“移除编辑后脱离支撑的孤立体素／碎块”。如果用户实际指模型二维图像里的孤立像素，需另确认；不要混同两类连通性。

## 5. 像撒沙子一样添加土：部分确认

已确认 [`try_shovel_place`](../../src/app/core/input.rs) 持续刷入背包材料，按真实写入量扣库存；
[`apply_surface_terrain_placement`](../../src/app/core/vegetation.rs) 编译 stroke capsule，写入体素并发布地形／连通性变化。
屋顶实验的 [`apply_rooftop_soil`](../../src/app/core/rooftop_scene.rs) 支持在原本没有土的模型屋顶上添土，限制高度／区域，不耗普通背包。

**未找到** 独立“撒土粒子 → 重力落地 → 沉积为地形”的完整链路。可以保留“像撒土一样添加”的体验比喻；
在确认用户指哪个工具前，不写成新增完整颗粒土壤／沙堆物理模拟。

## 6. 粗像素世界中的局部精细模型

**已确认：** 蝴蝶、花头、苹果可以独立于粗地形体素分辨率表现形状与颜色细节。

- [`model_surface_cache.rs`](../../src/tracer/model_surface_cache.rs)：异步 CPU BVH 对有限规范视角、N×N 像素中心采样最近模型表面，缓存位置／法线／UV／材质；三类模型各自独立配置。
- [`model_surface_cache.slang`](../../shader/slang/model_surface_cache.slang)：按当前姿态和观察方向选视角，把像素 cell 的投影覆盖与缓存表面深度结合；材质、照明与姿态仍实时计算，不是预烘焙最终颜色贴图。
- [`model-precache.md`](../model-precache.md)：当前 8–32 分辨率，视角数量共用，三类可分别切换原生三角形／预缓存像素 cell；花朵只缓存花头，茎有独立绘制。

建议叫“模型空间、保留表面深度与实时光照的像素化细节”，不是全世界提升体素密度，也不是普通屏幕降分辨率。

## 7. 碰撞与刚体

[`re-flora-physics`](../../crates/re-flora-physics/src/lib.rs) 以 Rapier／Parry 提供通用碰撞世界：
32³ 地形砖使用原生 `Voxels` collider，固定步默认 120 Hz，支持胶囊角色查询和动态 sphere／convex hull 果实、CCD、摩擦与休眠。
地形编辑增量同步碰撞砖及邻接边界；动态树几何有 [`DeformingGeometry`](../../crates/re-flora-physics/src/deforming_geometry.rs) 更新路径。
与水模拟使用的 SDF 区分，不能写成水的 SDF 就是所有实体碰撞。

玩家表达：“角色和掉落果实开始与可编辑体素世界可靠地碰撞，移动树木的交互几何也随姿态更新。”
源码有此能力不等于任意规模果实堆无穿透或所有互动已完成；见 [`voxel_collision_architecture.md`](../voxel_collision_architecture.md)。

## 8. 玻璃反射与透射

[`glass_voxel_software_rt_validation.md`](../glass_voxel_software_rt_validation.md) 与相关 Slang transport 实现确认：
使用软件体素追踪而非硬件 RTX，包含 Fresnel 反射、Snell 折射、全内反射、Beer 吸收，混合合成中保留前景光栅深度。

**边界：** 当前显式玻璃测试／小屋场景启用；普通场景的 Sand 不自动变成玻璃。文档仍标记覆盖率性能门槛未通过、实验非 ship-ready；
不能写成所有正常地形都可直接放置的正式玻璃玩法，也不支持宣称完整玻璃焦散。用户展示时应标 prototype／experimental。

## 后续补充核对：相机、行为、阴影与性能

### 相机与拖拽

[`camera_control.rs`](../../src/app/core/camera_control.rs) 的 `begin_zoom_to_walk`／`begin_zoom_to_edit` 和
[`zoom.rs`](../../src/app/core/camera_control/zoom.rs) 确认滚轮触发步行／外部轨道编辑镜头衔接。
当前 lift 轨迹使用带切线的曲线，综合平移和旋转的弧长调整进度；不是旧研究文档中描述的简单位置／pitch 分别 lerp。
[`brush_stroke.rs`](../../src/app/brush_stroke.rs) 与地形 placement 的 stroke capsule 支持连续拖拽编辑。
用户描述的洒土、石板道路、土墙保留为玩法演示素材；未从已有 capsule 接口推断完整颗粒沉积模拟。
“第三人称”暂解释为外部编辑视角，不推断有跟随可见角色模型。

### 蝴蝶与踹树

[`butterfly_flight.rs`](../../src/particles/butterfly_flight.rs) 已确认独立自主运动／风漂移、平滑响应、速度和加速度上限，以及强风反向／衰减测试。
当前 particles／ambient ecology 检索没有找到花目标选择与风后重新选择目的地的链路。
用户明确补充“被花吸引、吹偏后改变目的地”“踹树能震落果实与叶片”；保留这两个行为，待用户给当前演示或对应版本再补代码入口。
这是证据版本尚未对应，不是断言功能不存在。

### 叶影

用户进一步确认：重做后的草地叶影不仅动态闪烁观感更好，还 **更细致、更有层次感**。
保留该视觉目标，不擅自简化为“减少闪烁”。旧 `foliage_shadow_stability_research.md` 是历史研究，不能证明当前效果已验收；本轮未录制视觉对照。

### PetalSonic 的公开入口与 Rust 范围

- [GitHub](https://github.com/tr-nc/petalsonic)
- [crates.io](https://crates.io/crates/petalsonic)
- [0.9.2 API](https://docs.rs/petalsonic/0.9.2/petalsonic/)
- [公开项目 README](https://github.com/tr-nc/petalsonic/blob/main/README.md)

已成功读取公开 main README：它声明 native HRTF、geometry acoustics、FDN late reverb、Rust audio／DSP 依赖与跨平台 CPAL。
安装的 0.9.2 manifest 和求解／DSP 源码确认没有 Steam Audio 库依赖；因此可以描述 **空间音频算法 100% Rust，不再使用 Steam Audio C++**。
设备 API／驱动仍是平台原生接口，不能扩展成整个音频栈不存在任何系统 FFI。
公开 README 的 quick-start 仍写 0.8，版本实现以本项目锁定的 0.9.2 为准，不照抄该例的版本号。
音质和整体性能“大幅提高”来自用户体验，本轮没有独立听感或性能 A/B。

### 数值与平台证据边界

| 用户新提供的信息 | 本轮核对结论 |
| --- | --- |
| RTX 3060 Ti、4K 屏幕、200+ FPS | 作为用户当前观察保留；低内部分辨率输出到 4K 不等于原生 4K shading；需补内部尺寸、场景与设置 |
| 每帧地形编辑、编辑时 100 FPS | 用户观察保留；源码确认持续编辑／渐进照明机制，但本轮没有验证每帧全部后台工作完成或测量 FPS |
| 显存约 2 GB | 用户观察保留；需补工具、统计范围和场景，未建立所有场景的显存上限 |
| macOS 约 30% 损耗 | [`packaging.md`](../packaging.md) 确认 MoltenVK 路径，但源码不能证明损耗百分比；需相同 workload 与明确硬件／后端基线才能归因 |
| Linux／Windows／macOS 支持 | 包装／源构建支持已确认；具体下载内容取决于 release tag |

旧 [`global-low-resolution-performance.md`](../evidence/global-low-resolution-performance.md) 使用 2560×1440 窗口，
记录 GPU 时间与呈现等待，且明确不能把 GPU scope 倒数冒充游戏 FPS；不用于替代这次 4K／200+ FPS 的用户实测。
