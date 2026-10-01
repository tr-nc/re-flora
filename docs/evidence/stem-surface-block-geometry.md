# Surface-attached 茎：几何格大小 A/B

当前统一 selector／动态控件见 [`stem-selector-controls.md`](stem-selector-controls.md)：experimental checkbox 和 A material slider 已退休；下文记录原实现和验证。

## 使用

R → Debug → **Pixel Sampling — Flower Stems**，启用实验，选择 **Surface-attached cells (material A / block geometry B)**。

- **Surface-attached stems: block geometry B (off = continuous A)**：默认关闭。关闭保留此前连续轮廓＋材质量化；开启才改变真实几何。
- **Surface-attached stems B: geometry cell size (larger = blockier)**：独立、可保存的几何格大小，默认 1，范围 0.25–4，单位为 authored stem voxel edge 的倍数。小格更细，大格更块状。
- 旧 surface-cell 滑杆明确标为 **A: material cell size (continuous silhouette)**，不再误以为它改变几何。既有设置 ID／数值、方法索引和其他采样模式保留。

两项新字段声明在 `config/gui.toml`，自动生成 typed GUI／SavedControls 绑定，统一保存。所有茎采样控件集中在同一 Debug concern；不添加 App-only 滑杆或保存分支。用户工作区的 experimental=true、direction=550、material cell=1.33、radius=1.43 保留且不提交。

## 实现

`shader/slang/flower_stem_blocks.slang` 从共用 `stemCenter` / `stemRadius` 骨架生成解析 voxel slabs：

- authored local Y 网格，中心线位置在 X/Z 格上量化；主／侧枝的每行是填满整数格的矩形截面。不是减少圆锥段数，不是屏幕网格或只改绿色采样。
- 每个 Y slab 使用共享的 piecewise-affine wind warp。相邻行共用同一 Y 边界／风位移，分枝也共用该变形；格子身份不随风或相机重新 fit。法线用 inverse-transpose，实际斜面、棱角、轮廓和场景深度一起变化。
- 射线按 Y 行顺序求 voxel box intervals，行内排序合并主／侧枝，追踪实体内部相机的真正 union exit，而不是显示内部网格接缝。水平射线位于共享行边界时联合处理两行。没有不受控的 raymarch／距离自适应细分。
- 每个 box 与同一个 live socket half-space 求交，生成真实截断面／法线；继续检查最终实际显示点。没有把绿色深度压到花瓣后面或做 per-species 修补。
- fragment 直接查询这些实体，写实际世界命中的深度；没有贴 RGBA、新增 per-plant atlas、重建花头 cache，也没有新增碰撞／真实阴影投射行为。
- Vertex proxy 和 CPU chunk culling 都包含格子量化的空间 fringe（最大 authored edge × scale × geometry scale × 2），不依赖相机距离。

live 参数沿 `RenderFrameInput` → `StemExperiment` → GPU uniforms。保留已有 float4 ABI：`flower_stem_shape.w` 的正值仍为 world-direction B 的 object resolution，负值为 surface geometry B 的 cell scale，0 为原契约。两种 B 按 sampling method 互斥，即使两项保存的复选框都为 true，也不会混用。

## 验证

- `cargo fmt --check`、`cargo check`，全量 Rust **1264 passed / 4 ignored**。新增 geometry mode gating、NaN／上下界、空间 padding、review slider 端点和返回 A；frame-input sentinel 覆盖两个新字段。
- 新标签生成后另跑 `every_declared_generic_setting_saves_its_live_value`：通过。生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，由 cargo check 更新。
- Slang **30** 项通过。新 `flower_stem_block_geometry.slang` 以穷举 interval-union oracle 对照生产有序求交：0.25/1/4 格、两种风、540 个外部／内部／水平／斜向射线，检查距离、法线、socket；另外检查内部相机上下出口不落在内接缝、极短粗茎和倾斜 cap。细／粗网格实际命中数量不同，不只是设置变量改变。
- `env -u WAYLAND_DISPLAY node scripts/validate-stem-blocks.mjs --seconds 90`：真实 hidden/muted Release 四 capture（A、B、fine、coarse），八个 live phase（A、fine、normal、coarse、wind、orbit、near clipping、返回 A），六种花实际 draw；resize 成功、head bank 只构建一次、GUI hash 不变、无 ERROR/panic/VUID，shutdown failures=0。
- `scripts/validate-stem-contract.mjs --seconds 70`：之前的 World-direction A/B 四近／远 capture、12 phase 回归通过。
- 默认 hidden/muted Release smoke 通过，检查了退出／错误日志。
- 已阅读真实 fine／coarse 截图：粗格显著呈方块台阶、侧枝变为块状；细格保留纤细主茎／分枝。用户视觉批准和大花园性能验收仍是后续阶段，没有自动启动可见游戏。

产物（图片不提交）：`target/stem-blocks-review/{a,b,fine,coarse}.png`、`sweep.log`、`summary.json`。命令日志：`target/stem-blocks-{fmt,check,tests,slang,saved-controls,native,smoke,contract-regression}.log`。

## 明确的限制

- 这是**方截面、填充 voxel slabs 的风格化几何**，并非精确圆管体素化。小格使曲线台阶更细，但截面不会收敛成原来的圆形；大格也会改变粗细、填充间隙、合并很近的分枝，极粗例子明显类似块状小仙人掌。
- 每个截面明确至少占一个几何格；这是用户主动选择的几何量化，**不是按屏幕距离偷偷加粗的抗锯齿**。远处小于屏幕像素的部分仍可能 alias。
- 风采用每行仿射近似，不是逐点完全复现连续曲线。粗格改变花头附近的轮廓／遮挡；socket 半空间约束不等于完整 flower mesh 的碰撞 oracle。
- 没有新增网格缓存，但按需 box interval tracing 随行数增加，细格／高茎／大花园可能更贵。截图 FPS 不作为 benchmark，尚未做同场景 Release timing 对照或极端花高性能验收。
