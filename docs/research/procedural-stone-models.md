# 随机 low-poly 花园石材：一手审计与原生实验

## 目标与证据界限

权威生成物是 **Stone Model**（常规 indexed triangle solid），不是 voxel 集合。**Paving Slab** 是上下平整、可作铺地/踩踏实体的切割石板；**Landscape Rock** 是轮廓不对称、适度分面的景观岩石。本实验不实现铺设玩法、背包或导航；预览不修改花园地形。视觉候选不等于用户认可，也不等于性能通过。

本次不派研究 agent（Worker 的授权明确禁止再委派）。以下是实际读过的作者/官方文档及源文件，不把搜索摘要或二手教程当作算法证据。访问于 2026-10-06；浮动 URL 只描述本次读取版本，后续实现没有复制其源码。

## 一手资料审计

| 来源（实际审计 URL） | 实际看到的实现/契约 | 许可与限制 |
|---|---|---|
| Blender 官方 [`rockgen.py`](https://raw.githubusercontent.com/blender/blender-addons/main/add_mesh_extra_objects/add_mesh_rocks/rockgen.py)、[`__init__.py`](https://raw.githubusercontent.com/blender/blender-addons/main/add_mesh_extra_objects/add_mesh_rocks/__init__.py) | 随机选择 12 种预制多面体，按轴随机缩放/顶点参数；`generateRocks` 添加两层 Subsurf、四层 Displace，可选 Smooth；固定 seed；不是保证任意输入都低面数/凸/不自交的方案。注册文件署名 Paul “BrikBot” Marshall。 | 源头 SPDX **GPL-2.0-or-later**。只研究方法，不复制/移植代码；不能把 GPL 源码静默并入不同许可项目。生成内容与程序许可不能混为一谈。依赖 Blender 的 modifier/texture 实现。 |
| 作者 Denton Woods [`README`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/README.md)、[`generate.py`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/rockgen/generate.py)、[`clip.py`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/rockgen/clip.py)、[`LICENSE`](https://raw.githubusercontent.com/DentonW/Rock-Generator/master/LICENSE) | icosphere 或 jittered point-cloud convex hull → 比例 → 法线方向噪声 → 平滑/侵蚀 → 真平面切割并封口 → 基底切平 → normals/颜色。审计到 clip 的边交点、环、封口与 winding 处理；作者明确声明激进位移可自交，封闭不能替代自交检查。 | **MIT**, copyright 2026 Denton Woods；复制实质代码须保留通知。作者披露 AI 协作来源；不将它视为工业验证。这里只采纳“真切面而非仅噪声”的方法，不导入 numpy/scipy/生成器。其文档声称输出归使用者，未把该声称推广到其它工具。 |
| Three.js 官方 [`ConvexGeometry`](https://threejs.org/docs/pages/ConvexGeometry.html)、[`LICENSE`](https://raw.githubusercontent.com/mrdoob/three.js/dev/LICENSE) | 3D points → 包围它们的 convex hull；官方说明平均复杂度 O(n log n)，是 addon，不是基础引擎。适合自然轮廓，但铺地石板仍需独立平顶设计。 | **MIT**，复制/分发须保留 copyright/permission notice。本实现不引入 Three.js，也不复制 QuickHull；离线展示只用 Canvas。 |
| SideFX 官方 [`Clip SOP`](https://www.sidefx.com/docs/houdini/nodes/sop/clip.html) | 按平面裁剪 geometry，可选保留一侧/两侧并生成截面；可作为“切面”工作流的官方参照，不等于存在官方 Labs Rock Generator。 | 文档说明不是源代码许可证。Houdini 为专有产品，不能据此声称可直接移植节点实现。本项目不需要 Houdini 运行时。 |

检索到的 `acfaruk/proc-rock` raw README/LICENSE 为 404，没有取得可靠许可文本，因此**未选用、未复制，也未给它下许可结论**。Unity 社区凸包石块帖子只能是作者方法描述，没取得可审计代码/许可，因此不作为实现依赖。

## 方法比较与选型

- **随机点云凸包**：低面数、轮廓自然、凸实体适合 voxelization；需要稳健 hull/共面处理，无法靠压扁同时表达“切割平顶铺地石板”。
- **扰动球/多面体 + noise**：有圆润/凹凸和侵蚀感；大位移可能翻面、自交，顶面不适合平稳踩踏；需要控制 LOD 和 normals。
- **凸 blank + 半空间裁剪**（本次）：轮廓和真正的平面切口均由明确 planes 控制。每个 plane 保留内核，实体始终凸；封口和 fan triangulation 简单、依赖为零。代价是没有洞穴、层理凹槽或脱落碎片；这些不在本次真实 variation 范围内。

石板从 prism 出发，只变更 XZ 角切/窄 chamfer，主顶面与底面始终水平；厚度与 footprint 有独立约束。岩石采用 8–24 个有限非对称 support planes，法线适度倾斜，不叠加无界噪声。两类共享裁剪实现而不共享不合适的形状配方。

## 权威模型 interface

`src/stone_models.rs` 的 `generate(StoneSpec) -> Result<StoneMesh>` 是唯一 seed→geometry seam：

- SplitMix64 固定算法与 24-bit 抽样，不依赖库 RNG 改版；seed 为 u32。
- 右手系、+Y up；256 terrain voxels = 1 garden world unit；底部局部 pivot `(0,0,0)`。
- 顶点为共享 topological positions；u32 indices、每三角 outward flat normal 与 sRGB face color。渲染与 OBJ 才拆 shading corners，不影响实体闭合性。
- f64 归一化裁剪后转 f32；有限参数 clamp、非有限回默认，size 与 facet budget 都有界；Slab 高度≤短边的 30%。
- `validate` 检查正面积、有限属性/索引/包围盒、CCW/向外、所有点在面内、每条边恰两次且反向、Euler=2、正体积。凸性 + 定向封闭拓扑给出本方案的无自交实体约束，不宣称是任意外部网格的自交修复器。
- `obj()` 输出标准 v/vn/f triangles，导出的是模型，不是 voxel 冒充模型；OBJ 不携带本实验 face palette，geometry/flat normals 保留。

## 原生双路径与生命周期

`Stone Model` 只有一份 source positions/indices/flat normals/colors；`src/tracer/stone_preview.rs` 按 `StoneSpec` 保留不可变 `Arc<Source>`，路径或 pose 切换不重新定义 geometry：

1. **A/B 未勾选 — voxel/Contree 对照**：同一 source triangle mesh 转成生产 `ModelTriangleGpu` → 真实 `model_voxelize.comp` → `make_surface.comp` → 生产 `ContreeBuilder` → 真实 `marchContree` 可见性/深度。独立 R8 atlas 与 node/leaf buffers，不写入花园 atlas、world save 或 collider。`ContreeBuilder::new` 仅把 surface provider 接口从具体类型拓宽为既有 `ResourceContainer`，构建算法未重写。
2. **A/B 勾选 — direct triangle 实验**：同一 source mesh 派生顺序 indexed draw 与 triangle attribute buffer，独立 `stone_preview.vert/frag.slang` 硬件 rasterization → 既有 scene composite/pixel post-processing。不是离线模拟、mesh 名称占位或自动走 voxel fallback。

两路径均使用现有颜色/深度与全局 pixel grid，仍由最后的原生 egui pass 画 UI。Slab 使用现有 Limestone 材质，Rock 使用 live Rock palette；共同 `stone_lighting.slang` 复用环境光与 garden shadow 采样。**source sRGB face colors 保留在模型、direct upload 和离线展示中，但原生材质着色当前取 garden palette，不应用随机 face tint**；OBJ 没有 MTL/face colors。没有改 `model_mesh.slang`、全局 Contrast-aware 青边修复，也没有新增 dithering。

私有 atlas 为 64³、**128 cells/world unit**，不是改变全局 256 terrain voxels/world unit。GPU voxelizer 固定 256 的单位契约由输入 positions ×0.5 适配；Contree chunk size 与 origin 恢复真实 world scale。pivot 在 atlas `(32,2,32)`，留足边界；0.75-cell surface shell 属于生产 voxelization 表达，因此体素轮廓会比直接三角略厚/粗糙。每次真正 GPU atlas readback 都检查材质、填实中心、空边界与 4096 个独立 f64 winding/triangle-distance reference samples，CPU reference 不作为渲染替身。

`stone_transform.slang` 定义 bottom-local→world quaternion rigid pose、逆变换与 `stonePhysicalFrame`。**方向 snap 已在 main 集成**：direct vertex 调用共享 `modelMeshViewFrame`，使用同一 immutable bank、全局 checkbox/count，几何与法线共同量化；关闭时保留原运算。Voxel geometry 与模拟 pose 不改变。GUI yaw 仍是普通刚体旋转，不是视角设置的另一个 owner。见[集成证据](../evidence/stone-style-integration.md)；离线展示已打包真实 native 双路径和连续/128/256/global 图片，不依赖即将删除的 Worker target。

每个 acquired frame slot 持有 source/volume generation；seed、type、A/B、enable 或 extent descriptor 变化只回收已通过 fence 的 slot。graphics 与 compute 的 transient descriptor pools 都按 acquired slot 重置。私有 Contree allocation 自动提交的 managed CPU-cache GPU readback 在退出时先消费完成，再关闭/join decoder；不会把它发布为 garden CPU terrain。初始化 atlas 清除明确转到 GENERAL，vertex ID 使用 Vulkan 原生语义，避免额外 DrawParameters 能力。

## 保存、搜索与运行

`config/gui.toml` 追加 14 个声明式 saved fields；只有该配置和生成的 `src/app/generated/gui_adjustables_gen.rs` 拥有 saved values。旧 GUI 文件缺字段时在 loader 边界补齐，不要求用户丢弃旧配置。面板按调整 concern 分组，不按对象拆散：

- **Stone Geometry**：类型、seed（GUI 0–65535；模型 API 为 u32）、宽/深、Slab 厚度与角切、Rock 高度与 cutting-plane budget、轮廓 variation。类型条件使无关 controls disabled，但保留 ID 与 saved values。
- **Stone Rendering**：master enable、direct triangle A/B、仅在 enable 时 focus、预览 lift、rigid yaw。

**默认 master=false 保持原游戏。** Master=true 后，A/B unchecked 才是 stone 的既有 voxel 环境链路；checked 是 direct 候选。预览采用一次只读 downward terrain query 放在固定 inspection site，默认抬高避开草，没有碰撞/铺设玩法；切换和关闭均不改变地形。关闭不会强制重置用户相机。面板与说明为英文，说明走现有 `ui_text`，诊断计数只写日志。

正常试用（仅供用户/集成者主动启动，本 Worker 没有自动打开可见游戏）：

```bash
CARGO_BUILD_JOBS=2 cargo run --release
# Debug Panel → search “stone rendering” → enable stone preview.
# A/B unchecked: voxel/Contree; checked: direct triangles.
# Search “stone geometry” adjusts model; use the normal Save button.
```

原生验证脚本只启动 hidden/muted Release，用 `/tmp/re-flora-summer-gpu.lock` 串行 GPU。所有路径、日志查询、PNG 都属于调用脚本的 worktree。它复制并在 EXIT 恢复自己的 `config/gui.toml`；native GUI Save 子场景会短暂通过真实 Save 改写该文件，然后恢复原 hash，勿手动启用 Save replay 而不备份。

```bash
scripts/validate-stone-preview.sh
# Artifacts: target/stone-native/{voxel,direct}-{rock,slab}.{png,log}
# target/stone-native/{smoke,cycle,gui-save,gui-reload}.log
```

`RE_FLORA_STONE_REVIEW` 是 opt-in native fixture，支持 `voxel-rock/direct-rock/voxel-slab/direct-slab/cycle`。它驱动上述真实 saved-bound live fields，普通 fixture 不保存文件。`cycle` 覆盖 seed/type/size/yaw/A-B、关闭再开、16:1↔64:1 final grid 与 4/16 source samples。`RE_FLORA_STONE_GUI_SAVE_REVIEW` 仅为脚本的明确 Save 接受测试：记录**实际 native Save button rect**，通过现有 egui-winit adapter 回放规范化 WindowEvents，使既有 `Response.clicked → DebugSettings::save` 处理器执行。它不是 per-setting save hook，也没有直接写配置来模拟点击。输入是自动回放，**不是人工或 OS 鼠标操作**。隐藏 X11 窗口上的 core XSendEvent 尝试未被 winit XI2 接收；该失败没有算作 GUI 通过。

GUI 保存后以**同一 SHA256 的 Release binary**、无 stone fixture 重启，验证 saved master/A-B 与 source fingerprint 真正从磁盘 reload；查询日志也只用这个 binary 的 `--latest-log/--tail-latest-log`，不让 Cargo 把刚保存的值重编译成 fallback defaults、造成假阳性。

## 已取得的验证证据

在本 Worker worktree、NVIDIA GeForce RTX 3060 Ti、Slang API 2025.23.2 上执行：

- `cargo fmt --check`、`CARGO_BUILD_JOBS=2 cargo check`、完整 `cargo test`（main binary：**1354 passed / 4 ignored / 0 failed**，见 `target/stone-tests.log`）；快速 geometry/adapter/UI regressions 不包含 GPU 或随机 perf benchmark。
- `python3 scripts/run_slang_tests.py`：**35 Slang CPU tests 通过**，包括 bottom-pivot、刚体逆变换、向量长度/法线测试。
- `CARGO_BUILD_JOBS=2 cargo build --release` 与默认 `--hidden --mute --auto-exit 0.5` smoke；默认不出现 enabled stone preview。
- 四个真实 native fixture、10 阶段 lifecycle/extent cycle，以及 native GUI Search→Save→same-binary restart 全部成功，无 Vulkan validation error、panic 或 device-lost。
- 真实 native 搜索截图：`target/stone-native/gui-rendering.png`（5 controls）、`gui-geometry.png`（9 controls）；`gui-save.png/gui-reload.png` 是 native Save/reload 证据。Rust egui regression 还实际点击搜索结果中的 A/B checkbox，统一 Save 到临时 TOML 后重载，检查值及 unrelated scene setting 不变。

| 默认 seed 42 | 两路径 source fingerprint | triangles | 真 GPU occupied cells | Contree nodes / leaves |
|---|---|---:|---:|---:|
| Rock | `0ced2b67a555fe68` | 84 | 16498 | 224 / 2523 |
| Slab | `cbb8b4a48b37c91f` | 60 | 5489 | 133 / 2189 |

日志记录 `kernel=model_voxelize surface=make_surface visibility=Contree`、`cpu_reference_samples=4096` 与 `garden_atlas_writes=0`，direct 日志明确 `path=direct-triangle`；截图中能看到两类不同轮廓及未像素化 UI。曾抓到并修正新增 DrawParameters、清除后 UNDEFINED layout 和未回收 managed readback 问题；最终脚本不放行这些错误。默认关闭与部分 fixture 有启动 fruit-physics hitch warning，不把它当性能通过/失败结论，未对未修改的基线 commit 做独立 perf 归因。

## 离线展示与可导出的模型

打开 [`procedural-stone-gallery.html`](procedural-stone-gallery.html)。无网络依赖，8 个模型（2 类型 × seeds 42/7/122/65535），类型/seed/view rotation/wire controls 与普通 OBJ download。`procedural-stone-gallery-data.js` 和 `stone-models/*.obj` 都从权威 Rust generator 导出，不在 JavaScript 另造算法；生成文件勿手改：

```bash
CARGO_BUILD_JOBS=2 cargo test export_stone_gallery -- --ignored
```

**Canvas 是模拟展示，不是 native 渲染证明**，其页面明确标注。独立 native PNG 区引用本 worktree `target/stone-native`，清理 target 后会缺图，须重跑原生脚本；PNG 不作为巨型二进制资产提交。agent-browser 在 GPU lock 下验证离线 data/source hashes、全部四张 native 图片、类型/seed/wire 输入与截图；证据为 `target/stone-browser/validation.log`、`gallery-rock.png`、`gallery-slab-65535.png`。原生截图与模拟 Canvas 不混称。

## 未验收事项与集成风险

- **未作视觉/美术批准、未作性能接受。** 当前 native garden lighting 下分面明暗较弱；离线展示的 source face tint 不能代表 native palette。不能以截图中的 FPS 或一次 build_ms 当 release benchmark/性能预算通过。
- Voxel 表达有 shell/128-cell 分辨率导致的 silhouette 与法线离散误差；direct 普通 raster 精确取 source facets，预期不逐像素相同。
- 预览不参与 garden collider、shadow caster 或 DDGI occluder，不增加铺设、踩踏玩法或持久世界对象；只采样现有 lighting/shadows。
- Direction/view-quantization helper 接入留给 controller 的对应 Worker；本分支只有清晰 local pivot/rigid seam，没有重写该数学或其 `model_mesh.slang`。
- 唯一 tracked generated diff 是 GUI adjustables；集成时应合并 `config/gui.toml` source，再由 `cargo check` 再生，不能手改 generated 输出。pipeline builder、Tracer、App/GUI 注册是与其它分支的潜在交汇点。
- 参数、seed 与模型单位已测；未宣称任意平台/driver 下 bitwise 浮点相同、任意外部 mesh 可修复或全 seed 空间穷尽验证。
