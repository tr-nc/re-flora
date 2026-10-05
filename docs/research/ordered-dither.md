# Re: Flora 有序 dither：研究与原生实验

基线：`main d9f5a66af1a89d517d8273bd4af812932df3dd41`；独立分支 `agent/dither`。这是候选实验，不代表用户视觉批准或性能验收。离线交互比较见相邻 `ordered-dither.html`（交付阶段补充）。

## 一手资料与证据边界

实际读取了以下正文/源代码，而不是把搜索摘要当成算法审计：

- Lucas Pope 作者开发日志，[2014-07 #181–194](https://dukope.com/devlogs/obra-dinn/tig-02/)、[2017-11 #767](https://dukope.com/devlogs/obra-dinn/tig-32/)。2014 文中区分重复阈值图案和顺序误差扩散，并讨论 Bayer/ordered blue noise 与缩放、视频压缩；2017 明确写出 Obra Dinn 的 8×8 Bayer / 128×128 blue-noise 以及运动时 swimming/flickering 的问题。审计范围是作者公开说明，**没有审计 Obra Dinn 私有 shader**；不能宣称复刻其稳定化算法，不能把 hash noise 称为 blue noise。
- [Aseprite 官方 ChangePixelFormat API](https://www.aseprite.org/api/command/ChangePixelFormat/) 与 [ordered_dither.cpp 实现](https://raw.githubusercontent.com/aseprite/aseprite/main/src/render/ordered_dither.cpp)。实际读取 `BayerMatrix::D2={0,2,3,1}`、`OrderedDither` 的两个 palette 索引选择和 `OrderedDither2` 的最佳混色搜索。其返回**整份 palette 颜色**，不逐通道拼接。我们的亮度阶不是完整调色板搜索，不宣称等同 Aseprite 算法。
- [ImageMagick thresholds.xml](https://raw.githubusercontent.com/ImageMagick/ImageMagick/main/config/thresholds.xml)：实际读取 `o4x4` dispersed 与 `h4x4o` orthogonal halftone 的 16 个阈值及注释来源。第二种不是不存在的 `c4x4`，也不是 blue noise。本实验借用 rank 顺序，阈值规范化为 `(rank+0.5)/16`，**不同于原文件的 level/17**，保证均匀 rank 的对称中点分布。
- 本仓库 [scene_pixel_style.slang](../../shader/slang/scene_pixel_style.slang) 的 Contrast-aware 明确从源样本中选整份 RGB，修复独立 Lab 通道中位数造成的 cyan splice。这个性质只保证 resolve 不制造新 tuple；后续 dither 会创建同色系明暗值，不能说输出仍一定属于源样本集合。

上述 URL 本次正文读取成功；没有观察到 403。网页是可变 main 分支，报告记录审计内容，不把它说成固定版本的完整仓库审计。只以 Obra Dinn 为已查证游戏实例，不推广为所有像素游戏共识。

## 基线路径审计

- `shader/slang/dither.slang`：固定 seed 的整数 hash，±1 encoded sRGB LSB，静态噪声；没有低色阶量化。
- `post_processing.slang`：HDR average 先平均后 tone-map；Contrast-aware 逐样本 tone-map 后按 L* 选择整份颜色。最终 coarse cell 由 `scenePixelCoordinate` 给出，显示 block 最近邻放大；边缘 cropped，完整组才进入 barrier。之后 legacy 防色带 round-trip sRGB，输出浮点 screen target。
- `composition.slang`、`composition_camera_effects.slang`：lens flare temporal HDR 结果经手写 bilinear 采样相加，god ray visibility temporal 结果经采样后乘 weight/sun visibility，再 lerp ray color。`glass_resolve.slang` 的透明路径也消费这两个相机效果，必须同步接线。
- `skylight.slang`、`composition_sky.slang`、`composition_scene.slang`：可见天空背景与太阳/星空属于 composition；`getAuthoredSkyRadiance` 是另一条真实环境光来源，经 DDGI capture/global sky filter 与 local probe 查询到物体。
- `tracer.slang`：地形 normal hybrid 路径读取 `sampleDdgiTerrainSmoothEnvironment`，薄表面多面积分后得到接收 irradiance，与 direct/local/emission 分开。path tracing reference 有独立 indirect 估计。
- `environment_lighting.slang`、`flora_vertex.slang`、`flora_shadow.slang`、`particle_*`、`model_mesh.slang`：植被/模型/粒子有 vertex/cached receiver 环境光，不能仅改 `getSkyColor` 就声称全部蓝色天光被控制。本任务不改模型几何或缓存光照所有权。
- `src/tracer/scene_resolution.rs`、post uniform updater：四档 final pixel ratio、两档 density、两种 resolve、partial cell 与 native UI 策略原样保留。

## 设计决定（实现阶段验证）

所有新控件归属英文 `Ordered Dithering`，声明在 config、统一 Save/search，默认全部 A/B 关闭；旧文件自动补缺省。共享 Bayer/halftone pattern、Brightness levels 与 Strength。强度为 0 严格返回旧路径（在 sanitize/转换前 early return）。

1. **Global scene**：所有 opaque/raster/terrain/glass/sky/camera effects 已 composition 后、tone-map 与 Contrast-aware 完整 RGB resolve 后的最终 display-linear 场景。HUD/Debug UI 后绘制，不经过场景量化。开启时局部候选跳过，global 只量化一次；legacy ±LSB 也跳过。关闭或零强度保留 legacy 输出。
2. **God Rays**：对最终混合 weight 做有序标量阶，保持 ray RGB tuple，不分别量化各通道。
3. **Camera Lens Flare**：temporal resolve 后的 HDR 贡献整份颜色做亮度阶，不写入 temporal history，避免 history 抹平 pattern。
4. **Sky background**：仅可见天空/太阳/星空的背景贡献，不是天光，单独命名；玻璃射线看到的环境不单独重着色，global 会覆盖其最终结果。
5. **Skylight / ambient: terrain**：仅正常 hybrid 地形接收的 diffuse environment（天空+DDGI 反弹混合）做亮度阶；保留 direct/local/emission、不改 authored radiance/DDGI atlas/capture、不影响 reference/diagnostics。没有可分离的“纯蓝天空”buffer，不按 RGB 蓝通道挑光，不污染照明权威状态。植被/模型/玻璃间接光不由此局部开关控制；要统一这些路径用 global。

颜色算法：对 encoded sRGB 的最大分量做相邻明暗阶选择，同一个比例缩放整份 encoded RGB，再回到 linear。亮度阶不是每通道阶，也不是全图仅 N 色；不会把不同来源的通道拼成假色，但转换/明暗缩放仍可能有感知色相变化。HDR 使用扩展 sRGB 转换，阶在 0–1 之外继续延伸，不在局部效果中 tone-map、不截断高光、不使用会在上端点变成无穷的 inverse-Reinhard。最终显示量化只收到已 tone-map 的 0–1 输入。

所有局部图案坐标也用当前渲染像素 / samples-per-axis 对应 final coarse scene cell；global 使用现有 post 的 final cell。零强度与取消勾选同路；非有限/负输入在候选内安全归零，设置在 Rust 边界规范化。图案是屏幕固定，运动时可能滑过物体，不宣称 world-lock 或 Obra Dinn 稳定化。

## 实验/验证结果

待后续提交记录真实 native Release 截图、smoke、CPU Slang 性质测试、GUI 保存搜索、性能测量与 browser 检查。Canvas 内容只会标为算法模拟，不作为 GPU/游戏截图或性能证据。
