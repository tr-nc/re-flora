# Reddit：近半年开发回顾素材（持续补充）

## 口径与上次帖子

- 回顾窗口暂为 **2026-04-06 至 2026-10-06**，源码核对基线 `adde77ac`；本文是中文素材，不是最终英文帖子／正式发布说明。
- 以用户本次给出的技术变化为主线，后续继续补充；不强行收缩为泛化的花园功能清单。
- 找到疑似旧帖：[I'm officially open-sourcing my work-in-progress rust + Vulkan voxel game](https://www.reddit.com/r/VoxelGameDev/comments/1s9rwez/im_officially_opensourcing_my_workinprogress_rust/)（r/VoxelGameDev）。Reddit 阻止正文／JSON 读取，**尚未确认这是上次帖子**。请用户确认链接或提供正文，再逐项对比，避免把旧功能当新增。
- 以下区分 **代码确认**、**用户体验**、**待确认**。本轮没有重新测量性能，也没有证明当前源码功能都已进入下载版。
- 详细实现与来源见本文末尾的[附录：实现核对与来源](#附录实现核对与来源)。

## 1. 从逐像素路径追踪转向 DDGI

**主线素材：** 支持 DDGI，并让正在编辑的体素世界也能及时更新间接光照。光栅化绘制的草、叶子和树枝现在同样可以获取 Global Illumination，不再被挡在 GI 管线之外。

可面向读者写：

> 我把全局光照接到了 DDGI 上，也让光栅化植被共享这套照明。更重要的是，它不是只能照亮静态场景：在持续挖掘或添加地形时，完整光照结果仍会逐步更新并用于渲染，而不是等编辑结束才开始恢复。

**用户观察：** 比以前的 path tracing 更快、观感更好，也没有原先逐像素随机采样的颗粒噪声。

**边界：** 不写“动态编辑完全无闪烁／永远零延迟”。探针结果仍需收敛，当前文档明确记录未收敛时可能偏暗或闪动；性能百分比需要匹配 Release A/B。

演示：同一镜头持续挖开／填上洞口，展示地形与旁边草叶的间接照明变化。

## 2. 原生几何驱动的声音传播：reflection + reverb

**代码确认：两者都有。** 当前 PetalSonic 0.9.2 已不只是早期研究里的单反射 tap。

可以写成：

> 我也给声音加入了几何驱动的反射和混响。游戏发布体素场景与声源／听者的位置；后台 CPU 追踪反射路径和环境多次反弹，计算方向、延迟和不同频段的衰减。音频线程再用 HRTF 渲染早期反射，用三频段 FDN 合成混响尾音。

具体工作：

- 版本化、不可变的 Contree 声学快照，让地形变化能够进入声学求解。
- 独立传播 Worker 和有预算的查询，不在音频实时播放线程上追踪整场景。
- 对优先音源计算有限的早期反射；材质吸收／散射影响反射强度。
- 听者中心多跳追踪估计混响衰减、pre-delay 和湿声比例，驱动共享 8-line／三频段 FDN。
- 树冠作为有空间分布的音源接入，而非只有一个树中心发声点。

### 公开的原生 Rust 音频库

这一轮 Spatial Audio 已改为 **100% Rust 实现的原生空间化／声学算法**，不再依赖 Steam Audio 的 C++ 库。
当前依赖 manifest 与公开 README 均确认原生 HRTF、声学与 FDN 路径，没有 Steam Audio 依赖。
“100% Rust”指空间音频实现，不是说跨平台设备驱动／操作系统接口也没有 native API。

我们的库 **PetalSonic** 已公开，可在帖子里单独介绍并提供链接：

- GitHub：[tr-nc/petalsonic](https://github.com/tr-nc/petalsonic)
- crates.io：[petalsonic](https://crates.io/crates/petalsonic)
- API 文档：[PetalSonic 0.9.2](https://docs.rs/petalsonic/0.9.2/petalsonic/)

**用户体验：** 音频质量和整体性能相比以前有较大提升；本轮未重新做听感／性能对照，不填未经测量的提升比例。

**边界：** 是 ray/path-tracing + DSP 的混合声学，不是对所有声源、所有反弹逐样本精确求解；具体听感需带声音演示。

## 3. 可以实时随风运动的整树

**代码确认：** 整树光栅绘制，层级枝条姿态、GPU 求解／skinning、叶片绑定和树冠声音消费共享风场。

> 树不再只是静止的体素摆件：树干、枝条和叶片形成一套层级运动，父枝的变化会传到子枝。运动和沙沙声由同一个风场驱动。

**用户体验：** 在我的 **RTX 3060 Ti** 上，整棵树随风更新仍可以达到实时帧率。暂不附未经确认的 FPS／分辨率。

**用户补充的互动素材，保留：** 可以踹树，震动从根部传播到枝条与叶片，并发出沙沙声；踹树还可以把果子和叶子踹下来。
本轮在当前 main 的输入／树姿态／物理链路中没有找到 kick／树冲量入口，需要用户给操作方式、对应分支或演示后落实；不能把风响应弹簧当成已实现踹树。

## 4. 孤立体素检测与清理

**代码确认：** 地形六邻接连通性检测，编辑松开后移除脱离支撑的选定碎块，并转换为视觉粒子；载入场景也有检查入口。

> 挖掘后留下的悬空小碎块可以被检测并清掉，而不是永远悬在空中。

**术语待用户确认：** 此处暂将“孤立像素”理解为孤立地形体素，不是屏幕二维像素。存在分析／粒子容量预算，不承诺任意尺寸都即时清空。

## 5. 像撒沙子一样添加土

**用户提出的素材，保留：** 一种像撒沙子一样把土添加到地形上的新工具。

**当前部分确认：** 持续刷入背包中的体素材质、按真实添加量扣库存并同步地形；屋顶实验可在裸模型屋顶上添土。
用户进一步说明支持拖拽洒土，也可以拖拽形成石板道路、土墙。当前已核对连续 stroke capsule 的地形添加链路；这些具体形态／洒土效果先按用户提供的玩法素材保留，后续用演示对应代码。
未找到“粒子落地后沉积成地形”的完整模拟链路，需确认指的是哪个工具／效果。

暂用体验措辞“像撒土一样逐步添加地形”，不写成完整沙堆／颗粒土壤物理系统。

## 6. 在粗体素世界里表现更细的形状

**代码确认，应用于蝴蝶、花头和苹果：** 模型表面按有限视角采样为独立 N×N 像素 cell，保留位置、法线、UV／材质与表面深度；姿态和光照实时更新。当前三类可分别启用预缓存，也保留原生三角形绘制。

> 我在尝试一种局部精细、整体仍保持粗像素风格的表达：蝴蝶、花朵和苹果不必受地形体素的大小限制，但也不是贴上静态 sprite。它们保留模型表面的深度、姿态和实时光照，以独立的像素分辨率呈现细节。

**边界：** 花的缓存部分是花头，花茎有自己的绘制方式；不是把整个世界的体素密度提高，也不是单纯把屏幕缩小再放大。

演示：三类物体近景、转动视角、改变光照或风动。

## 7. 通用碰撞与刚体基础

**代码确认：** Rapier／Parry 的 32³ 精确体素碰撞砖，增量地形同步、固定步模拟、胶囊角色查询、掉落果实刚体、CCD、摩擦与休眠；变形树几何也有碰撞更新路径。

> 我加入了与可编辑体素世界对齐的碰撞基础，让角色移动、果实掉落和动态树几何的交互不再只依赖水模拟的 SDF。

**边界：** 不宣称任意规模果实堆都无穿透，也不因此推断踹树输入已经接通。

## 8. 玻璃反射与透射

**代码确认，实验场景：** 软件体素追踪支持 Fresnel 反射、Snell 折射、全内反射和吸收，并与光栅前景深度合成。

> 我还做了体素玻璃的反射和透射，让粗像素世界里的透明材质也能表现出空间层次。

**边界必须保留：** 当前是测试／小屋场景显式启用的实验，既有覆盖率性能门槛未通过；不宣传为所有正常场景都已正式可用的玻璃工具，也不宣称玻璃焦散。

## 9. 第一人称与外部编辑视角的缩放衔接

**代码确认：** 滚轮触发步行第一人称与 OrbitEdit 的平滑镜头衔接，位置曲线与姿态协调插值；不是只能硬切两种相机。

**用户的玩法表达：** 更好的第三人称／外部视角，让玩家能从第一人称通过缩放无缝拉远，开始更方便地编辑地形；拖拽洒土、铺石板道路、筑土墙。

**术语：** 当前核对到的是外部轨道编辑相机，不据此推断有可见角色模型的第三人称跟随系统。正文可以写“从第一人称漫步平滑拉远到编辑视角”，视频让体验自己说话。

演示：行走 → 缩放拉远 → 一笔拖出道路或墙 → 返回近景。

## 10. 更自然、会受风干扰的蝴蝶

**代码确认：** 蝴蝶自主飞行与风平流分开；有平滑、限速／限加速度的风漂移，强风反向与风停后的恢复也有守卫测试。

**用户补充的行为素材：** 蝴蝶会被花吸引；飞向花朵时可能被一阵风吹偏，随后改变目的地。动作不再只是重复的固定轨迹。

本轮在当前基线找到风响应，但尚未找到花目标选择／风后重新选目标的实现入口；这部分保留为用户提供的行为，待确认对应版本或演示，不改写成单纯随机游走。

## 11. 更细致、更有层次的叶片阴影

**用户确认的视觉变化：** 重做了叶子投在草地上的阴影；不仅动态闪烁观感更好，阴影本身也更细致，并有更丰富的层次感。在低分辨率风格化画面里保留这些细节并不容易。

> 草地上的树影现在不只是粗糙的一团暗色：细碎的叶影、明暗层次，以及随风变化的光斑，都有了更好的表现。

这项不只概括成“减少闪烁”，也不声称是逐叶精确光线模拟。本轮未做当前 shader 的专项视觉对照；旧阴影研究只用于背景，不能当成本次新效果的验收。

演示：固定近景拍草地，保留细碎投影与风动，再给一个远景说明低分辨率下的整体效果。

## 12. 性能与显存：用户当前实测素材

| 指标 | 用户提供的结果 | 发帖时需保留的条件 |
| --- | --- | --- |
| 风格化低分辨率渲染 | RTX 3060 Ti，**4K 屏幕下 200 FPS 以上**，仍保持可用游戏体验 | 是 4K 屏幕／输出下的低内部渲染分辨率，不写成原生 4K shading 200 FPS；补内部尺寸、场景和设置 |
| 实时地形编辑 | 支持每帧编辑，**编辑时可达 100 FPS** | 用户观察；补构建版本、场景、笔刷、是否持续更新 DDGI，不能由画面 FPS 推出所有后台任务每帧完成 |
| 显存 | 降低到 **约 2 GB** | 用户观察；补场景、统计工具及是进程占用／资源分配还是整卡用量，不扩展成所有场景的上限 |

可用第一人称口吻写：“在我的 RTX 3060 Ti 上，低分辨率风格化渲染输出到 4K 屏幕时可以超过 200 FPS；持续编辑地形时可以达到 100 FPS，当前观察到的显存占用约 2 GB。”

这些新数字不是从旧 GPU scope 的倒数推出来的；仓库里的 2560×1440 历史性能实验也不能代替这次 4K 输出的实际测量。

## 13. Linux、Windows、macOS

**已确认的支持范围：** 三平台源构建／打包路径，macOS 经 MoltenVK 使用 Vulkan-on-Metal；发布包实际范围仍以对应 tag 为准。

**用户提供的性能素材：** macOS 经 MoltenVK 有约 **30% 性能损耗**。
先保留为用户测量／估计，待补同场景、硬件、图形后端与比较基线；不能直接断言跨平台差异全部来自 MoltenVK，也不写成该库在所有 Mac 上固定损失 30%。

## 后续补充与发帖前检查

- [ ] 用户继续补充其他变化。
- [ ] 确认旧 Reddit 帖子的作者、日期和完整正文，建立真正的“上次以来”基线。
- [ ] 确认踹树、撒土工具与孤立像素所指的具体实现。
- [ ] 录制当前 Release 游戏视频：DDGI 动态编辑、带原声树冠／室内声学、局部像素模型和玻璃实验。
- [ ] 补充 3060 Ti 的内部渲染分辨率、4K 输出方式、200+／编辑 100 FPS 场景与设置、2 GB 显存统计口径，以及 Mac 30% 对比条件；区分用户观察与复测证据。
- [ ] 核对推荐下载版对应 tag 的功能范围。
- [ ] 根据目标 subreddit 选择技术深度，再写英文正文和标题；暂不发布。

其他候选补充：花园快照、白石小屋、屋顶空间探索、行走／果实稳定性、花朵美术与开发工具。
## 附录：实现核对与来源

### 证据范围

核对版本：游戏源码 `adde77ac`，当前依赖 PetalSonic **0.9.2**（`Cargo.toml`／`Cargo.lock`）。
本轮只做源码与历史文档核对，没有重新做画面、听感或匹配性能验收。
用户提供的 RTX 3060 Ti 体验单独记为用户观察，不转成未经测量的 FPS／百分比。

### 上次 Reddit 帖子

搜索 `site:reddit.com "Re: Flora" voxel` 找到疑似帖子：

[I'm officially open-sourcing my work-in-progress rust + Vulkan voxel game](https://www.reddit.com/r/VoxelGameDev/comments/1s9rwez/im_officially_opensourcing_my_workinprogress_rust/)，位于 r/VoxelGameDev。

搜索摘要提及 Rust／Vulkan、GPU compute 64-tree voxel path tracer、实时地形和 flora 编辑、GPU instancing、spatial audio 等。
这只是搜索摘要，不是已读取的完整原帖。正文提取失败，JSON 请求也返回 Reddit 的 “You've been blocked by network security” 页面；
未确认作者、日期或是否为用户最近一次发帖。需要用户确认链接或提供正文，才能严谨写“相较上一次新增”。
不能因为摘要中已经有 spatial audio 就说以前已有当前的几何反射／混响。

### 1. DDGI 与动态编辑

**已确认：** 当前不是只能给 ray-traced 地形使用 GI。草／植物层和树叶在 compute pass 中采样环境 irradiance，随后光栅绘制消费缓存；树枝光栅 shader 也采样 DDGI。

源码入口：

- [`DdgiRuntime`](../src/ddgi/runtime.rs)：完整 field／volume 的身份、调度、发布与消费者资源所有权；`publish_ready_volume` 不把半成品直接交给消费者。
- [`flora_vertex.slang`](../shader/slang/flora_vertex.slang)：`sampleFloraEnvironment`。
- [`flora_lighting_cache.comp.slang`](../shader/slang/flora_lighting_cache.comp.slang)：按实例／层计算 irradiance。
- [`tree_leaf_lighting_cache.comp.slang`](../shader/slang/tree_leaf_lighting_cache.comp.slang)：按叶体素计算 irradiance。
- [`raster_tree.frag.slang`](../shader/slang/raster_tree.frag.slang)：`sampleDdgiTerrainSmoothEnvironment`。

持续编辑时允许完整光照场渐进发布，而不是总等到松开编辑才更新；文档记载的 40 次编辑场景有 22 次编辑期间发布。
但发布可用的完整数据 **不等于即时完全收敛**。当前地形直接使用采样 irradiance × albedo，不再用编辑区域固定颜色兜底；未收敛估计仍可能闪动或偏暗。
详见 [`terrain_edit_lighting.md`](terrain_edit_lighting.md) 与 [`cave_edit_lighting.md`](cave_edit_lighting.md)。

**推荐宣传口径：** “实时更新的探针式全局光照，也覆盖光栅化植被；持续编辑期间仍能更新并使用完整光照结果。”
“没有旧逐像素路径追踪的颗粒噪声”可以作为观感描述，但不要写成“动态编辑永远无闪烁、零延迟、完美 GI”。
相比旧 path tracing 更快、质量更好暂归用户观察；本轮没有同场景 A/B 性能证据。

### 2. 声音：反射与混响均已实现

#### 游戏侧做了什么

- [`spatial_sound_manager.rs`](../src/audio/spatial_sound_manager.rs)：初始化原生 HRTF、48 kHz 音频 world，**固定开启 environmental acoustics**，传入声学场景与预算。
- [`ContreeBuilder`](../src/builder/contree/mod.rs)：从 CPU Contree 数据发布带版本的声学快照；体素材质提供三频段吸收、散射、透射等声学属性。
- [`App`](../src/app/core/mod.rs)：更新后调用 `publish_acoustic_scene`；不是把可变 GPU 场景交给音频实时线程。
- [`spatial_frame.rs`](../src/audio/spatial_frame.rs) 与 sound manager：发布完整 listener／emitter 状态。
- [`canopy_distributed_emitter_adapter.rs`](../src/audio/canopy_distributed_emitter_adapter.rs)：把树冠空间分布接到音源，而非只用树中心一个点。

#### PetalSonic 0.9.2 实际求解

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

旧 [`native_path_traced_audio_research.md`](native_path_traced_audio_research.md) 核对的是 **0.6.0**，其中“单 tap、没有 late reverb”是历史基线，不能用于描述当前版本。

### 3. 整树光栅绘制、风与声音

**已确认：** 当前整树采用光栅路径，树干／枝条的层级姿态跟随共享风场；子枝继承父枝变形端点，叶片绑定跟随，树冠声音也消费共享风场。
GPU 层级求解／表面 skinning 避免靠 CPU 每帧重建整树几何；交互查询和碰撞更新保留一致的姿态语义。

入口：[`pose.rs`](../src/tree_gen/pose.rs)、[`tree_pose.comp.slang`](../shader/slang/tree_pose.comp.slang)、
[`tree_skin.slang`](../shader/slang/tree_skin.slang)、[`whole_tree_rasterization_progress.md`](whole_tree_rasterization_progress.md)、
[`canopy_distributed_emitter_adapter.rs`](../src/audio/canopy_distributed_emitter_adapter.rs)。

- “我的 RTX 3060 Ti 上整树随风仍达到实时帧率”保留为用户体验，不附未经确认的帧率。
- “随风声更新”更准确写为 **运动与沙沙声由同一风场驱动**，不是声音信号驱动树的动画。
- **踹树待确认：** 本轮检索输入、树姿态、physics 和 shaders，未找到玩家 kick／冲量注入树层级的链路。当前弹簧风响应不能作为踹树证据。请确认操作方式／分支／提交或演示。

### 4. 孤立体素清理

发现的生产实现是 **地形连通性**，不是屏幕孤立像素滤镜：
[`terrain_connectivity.rs`](../src/app/core/terrain_connectivity.rs) 在地形编辑松开时分析六邻接连通分量，并可在载入世界时检查。
脱离支撑的选定体素通过 [`detachment.rs`](../src/app/core/terrain_connectivity/detachment.rs) 从 atlas 移除并生成视觉粒子，随后更新物理与可见地形。
跨分析边界会继续核对连接，避免把仍连接主地形的块误删；分析与粒子容量有预算，不能承诺所有尺寸即时全部清理。

建议写“移除编辑后脱离支撑的孤立体素／碎块”。如果用户实际指模型二维图像里的孤立像素，需另确认；不要混同两类连通性。

### 5. 像撒沙子一样添加土：部分确认

已确认 [`try_shovel_place`](../src/app/core/input.rs) 持续刷入背包材料，按真实写入量扣库存；
[`apply_surface_terrain_placement`](../src/app/core/vegetation.rs) 编译 stroke capsule，写入体素并发布地形／连通性变化。
屋顶实验的 [`apply_rooftop_soil`](../src/app/core/rooftop_scene.rs) 支持在原本没有土的模型屋顶上添土，限制高度／区域，不耗普通背包。

**未找到** 独立“撒土粒子 → 重力落地 → 沉积为地形”的完整链路。可以保留“像撒土一样添加”的体验比喻；
在确认用户指哪个工具前，不写成新增完整颗粒土壤／沙堆物理模拟。

### 6. 粗像素世界中的局部精细模型

**已确认：** 蝴蝶、花头、苹果可以独立于粗地形体素分辨率表现形状与颜色细节。

- [`model_surface_cache.rs`](../src/tracer/model_surface_cache.rs)：异步 CPU BVH 对有限规范视角、N×N 像素中心采样最近模型表面，缓存位置／法线／UV／材质；三类模型各自独立配置。
- [`model_surface_cache.slang`](../shader/slang/model_surface_cache.slang)：按当前姿态和观察方向选视角，把像素 cell 的投影覆盖与缓存表面深度结合；材质、照明与姿态仍实时计算，不是预烘焙最终颜色贴图。
- [`model-precache.md`](model-precache.md)：当前 8–32 分辨率，视角数量共用，三类可分别切换原生三角形／预缓存像素 cell；花朵只缓存花头，茎有独立绘制。

建议叫“模型空间、保留表面深度与实时光照的像素化细节”，不是全世界提升体素密度，也不是普通屏幕降分辨率。

### 7. 碰撞与刚体

[`re-flora-physics`](../crates/re-flora-physics/src/lib.rs) 以 Rapier／Parry 提供通用碰撞世界：
32³ 地形砖使用原生 `Voxels` collider，固定步默认 120 Hz，支持胶囊角色查询和动态 sphere／convex hull 果实、CCD、摩擦与休眠。
地形编辑增量同步碰撞砖及邻接边界；动态树几何有 [`DeformingGeometry`](../crates/re-flora-physics/src/deforming_geometry.rs) 更新路径。
与水模拟使用的 SDF 区分，不能写成水的 SDF 就是所有实体碰撞。

玩家表达：“角色和掉落果实开始与可编辑体素世界可靠地碰撞，移动树木的交互几何也随姿态更新。”
源码有此能力不等于任意规模果实堆无穿透或所有互动已完成；见 [`voxel_collision_architecture.md`](voxel_collision_architecture.md)。

### 8. 玻璃反射与透射

[`glass_voxel_software_rt_validation.md`](glass_voxel_software_rt_validation.md) 与相关 Slang transport 实现确认：
使用软件体素追踪而非硬件 RTX，包含 Fresnel 反射、Snell 折射、全内反射、Beer 吸收，混合合成中保留前景光栅深度。

**边界：** 当前显式玻璃测试／小屋场景启用；普通场景的 Sand 不自动变成玻璃。文档仍标记覆盖率性能门槛未通过、实验非 ship-ready；
不能写成所有正常地形都可直接放置的正式玻璃玩法，也不支持宣称完整玻璃焦散。用户展示时应标 prototype／experimental。

### 后续补充核对：相机、行为、阴影与性能

#### 相机与拖拽

[`camera_control.rs`](../src/app/core/camera_control.rs) 的 `begin_zoom_to_walk`／`begin_zoom_to_edit` 和
[`zoom.rs`](../src/app/core/camera_control/zoom.rs) 确认滚轮触发步行／外部轨道编辑镜头衔接。
当前 lift 轨迹使用带切线的曲线，综合平移和旋转的弧长调整进度；不是旧研究文档中描述的简单位置／pitch 分别 lerp。
[`brush_stroke.rs`](../src/app/brush_stroke.rs) 与地形 placement 的 stroke capsule 支持连续拖拽编辑。
用户描述的洒土、石板道路、土墙保留为玩法演示素材；未从已有 capsule 接口推断完整颗粒沉积模拟。
“第三人称”暂解释为外部编辑视角，不推断有跟随可见角色模型。

#### 蝴蝶与踹树

[`butterfly_flight.rs`](../src/particles/butterfly_flight.rs) 已确认独立自主运动／风漂移、平滑响应、速度和加速度上限，以及强风反向／衰减测试。
当前 particles／ambient ecology 检索没有找到花目标选择与风后重新选择目的地的链路。
用户明确补充“被花吸引、吹偏后改变目的地”“踹树能震落果实与叶片”；保留这两个行为，待用户给当前演示或对应版本再补代码入口。
这是证据版本尚未对应，不是断言功能不存在。

#### 叶影

用户进一步确认：重做后的草地叶影不仅动态闪烁观感更好，还 **更细致、更有层次感**。
保留该视觉目标，不擅自简化为“减少闪烁”。旧 `foliage_shadow_stability_research.md` 是历史研究，不能证明当前效果已验收；本轮未录制视觉对照。

#### PetalSonic 的公开入口与 Rust 范围

- [GitHub](https://github.com/tr-nc/petalsonic)
- [crates.io](https://crates.io/crates/petalsonic)
- [0.9.2 API](https://docs.rs/petalsonic/0.9.2/petalsonic/)
- [公开项目 README](https://github.com/tr-nc/petalsonic/blob/main/README.md)

已成功读取公开 main README：它声明 native HRTF、geometry acoustics、FDN late reverb、Rust audio／DSP 依赖与跨平台 CPAL。
安装的 0.9.2 manifest 和求解／DSP 源码确认没有 Steam Audio 库依赖；因此可以描述 **空间音频算法 100% Rust，不再使用 Steam Audio C++**。
设备 API／驱动仍是平台原生接口，不能扩展成整个音频栈不存在任何系统 FFI。
公开 README 的 quick-start 仍写 0.8，版本实现以本项目锁定的 0.9.2 为准，不照抄该例的版本号。
音质和整体性能“大幅提高”来自用户体验，本轮没有独立听感或性能 A/B。

#### 数值与平台证据边界

| 用户新提供的信息 | 本轮核对结论 |
| --- | --- |
| RTX 3060 Ti、4K 屏幕、200+ FPS | 作为用户当前观察保留；低内部分辨率输出到 4K 不等于原生 4K shading；需补内部尺寸、场景与设置 |
| 每帧地形编辑、编辑时 100 FPS | 用户观察保留；源码确认持续编辑／渐进照明机制，但本轮没有验证每帧全部后台工作完成或测量 FPS |
| 显存约 2 GB | 用户观察保留；需补工具、统计范围和场景，未建立所有场景的显存上限 |
| macOS 约 30% 损耗 | [`packaging.md`](packaging.md) 确认 MoltenVK 路径，但源码不能证明损耗百分比；需相同 workload 与明确硬件／后端基线才能归因 |
| Linux／Windows／macOS 支持 | 包装／源构建支持已确认；具体下载内容取决于 release tag |

旧 [`global-low-resolution-performance.md`](evidence/global-low-resolution-performance.md) 使用 2560×1440 窗口，
记录 GPU 时间与呈现等待，且明确不能把 GPU scope 倒数冒充游戏 FPS；不用于替代这次 4K／200+ FPS 的用户实测。
