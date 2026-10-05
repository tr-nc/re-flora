# Re: Flora 有序 dither：研究与原生实验

基线：`main d9f5a66af1a89d517d8273bd4af812932df3dd41`；独立分支 `agent/dither`。这是候选实验，不代表用户视觉批准或性能验收。离线交互比较见 [ordered-dither.html](ordered-dither.html)，直接打开即可，保留相邻 `ordered-dither-assets/` 文件夹。渲染实现提交：`5ab43c4417f6e39b70056d12cf9896a9717a3e74`。

## 一手资料与证据边界

实际读取了以下正文/源代码，而不是把搜索摘要当成算法审计：

- Lucas Pope 作者开发日志，[2014-07 #181–194](https://dukope.com/devlogs/obra-dinn/tig-02/)、[2017-11 #767](https://dukope.com/devlogs/obra-dinn/tig-32/)。2014 文中区分重复阈值图案和顺序误差扩散，并讨论 Bayer/ordered blue noise 与缩放、视频压缩；2017 明确写出 Obra Dinn 的 8×8 Bayer / 128×128 blue-noise 以及运动时 swimming/flickering 的问题。审计范围是作者公开说明，**没有审计 Obra Dinn 私有 shader**；不能宣称复刻其稳定化算法，不能把 hash noise 称为 blue noise。
- [Aseprite 官方 ChangePixelFormat API](https://www.aseprite.org/api/command/ChangePixelFormat/) 与 [ordered_dither.cpp 实现](https://raw.githubusercontent.com/aseprite/aseprite/main/src/render/ordered_dither.cpp)。实际读取 `BayerMatrix::D2={0,2,3,1}`、`OrderedDither` 的两个 palette 索引选择和 `OrderedDither2` 的最佳混色搜索。其返回**整份 palette 颜色**，不逐通道拼接。我们的亮度阶不是完整调色板搜索，不宣称等同 Aseprite 算法。
- [ImageMagick thresholds.xml](https://raw.githubusercontent.com/ImageMagick/ImageMagick/main/config/thresholds.xml)：实际读取 `o4x4` dispersed 与 `h4x4o` orthogonal halftone 的 16 个阈值及注释来源。第二种不是不存在的 `c4x4`，也不是 blue noise。本实验借用 rank 顺序，阈值规范化为 `(rank+0.5)/16`，**不同于原文件的 level/17**，保证均匀 rank 的对称中点分布。
- [Joel Yliluoma 的作者算法文章](https://bisqwit.iki.fi/story/howto/dither/jy/)：实际读取文章开头、standard ordered 的非均匀 palette-spacing 限制与 algorithm 1 的整份颜色对/比例搜索。Chrono Cross 源图在文中是算法演示输入，**不是该游戏使用此算法的证据**；未审计后续全部变体。
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

## 已实现的设计决定

所有新控件归属英文 `Ordered Dithering`，声明在 config、统一 Save/search，默认全部 A/B 关闭；旧文件自动补缺省。共享 Bayer/halftone pattern、Brightness levels 与 Strength。强度为 0 严格返回旧路径（在 sanitize/转换前 early return）。

1. **Global scene**：所有 opaque/raster/terrain/glass/sky/camera effects 已 composition 后、tone-map 与 Contrast-aware 完整 RGB resolve 后的最终 display-linear 场景。HUD/Debug UI 后绘制，不经过场景量化。开启时局部候选跳过，global 只量化一次；legacy ±LSB 也跳过。关闭或零强度保留 legacy 输出。
2. **God Rays**：对最终混合 weight 做有序标量阶，保持 ray RGB tuple，不分别量化各通道。
3. **Camera Lens Flare**：temporal resolve 后的 HDR 贡献整份颜色做亮度阶，不写入 temporal history，避免 history 抹平 pattern。
4. **Sky background**：仅可见天空/太阳/星空的背景贡献，不是天光，单独命名；玻璃射线看到的环境不单独重着色，global 会覆盖其最终结果。
5. **Skylight / ambient: terrain**：仅正常 hybrid 地形接收的 diffuse environment（天空+DDGI 反弹混合）做亮度阶；保留 direct/local/emission、不改 authored radiance/DDGI atlas/capture、不影响 reference/diagnostics。没有可分离的“纯蓝天空”buffer，不按 RGB 蓝通道挑光，不污染照明权威状态。植被/模型/玻璃间接光不由此局部开关控制；要统一这些路径用 global。

颜色算法：对 encoded sRGB 的最大分量做相邻明暗阶选择，同一个比例缩放整份 encoded RGB，再回到 linear。亮度阶不是每通道阶，也不是全图仅 N 色；不会把不同来源的通道拼成假色，但转换/明暗缩放仍可能有感知色相变化。HDR 使用扩展 sRGB 转换，阶在 0–1 之外继续延伸，不在局部效果中 tone-map、不截断高光、不使用会在上端点变成无穷的 inverse-Reinhard。最终显示量化只收到已 tone-map 的 0–1 输入。

所有局部图案坐标也用当前渲染像素 / samples-per-axis 对应 final coarse scene cell；global 使用现有 post 的 final cell。零强度与取消勾选同路；非有限/负输入在候选内安全归零，设置在 Rust 边界规范化。图案是屏幕固定，运动时可能滑过物体，不宣称 world-lock 或 Obra Dinn 稳定化。

## 原生 Release 实验与可观察结果

原生实验在本工作区 RTX 3060 Ti / Vulkan / sRGB swapchain 上运行，全部 hidden + mute，未启动可见游戏。每次 GPU 运行都用 `flock --close /tmp/re-flora-summer-gpu.lock`；编译都用 `CARGO_BUILD_JOBS=2`。源码保留 runtime 英文 GUI checkbox A/B，脚本只是通过真实 saved fields 设置其内存值，不是另外一条实验渲染通道。

`src/app/core/ordered_dither_review.rs` 在同一 App 内依次运行原效果、global Bayer、global halftone、单独 rays、flare、terrain ambient、sky background、局部组合、global 覆盖局部、所有候选强度 0、切回原效果；每阶段稳定至少 60 帧后截图。共同设置：64:1 final ratio、4-to-1 density、Contrast-aware、8 levels / strength 1（零强度阶段除外）。SUN fixture 额外朝向当前太阳，避免从看不到 flare 的相机误判局部开关。新增 sun 相机 fixture 期间未改变共同渲染算法。

| 运行 | 原生 PNG | 检查结果 |
| --- | ---: | --- |
| smoke（以及最终再次 smoke） | 0 | hidden/muted 正常退出，`failures=0` |
| compare / 花园 | 11 | 同一进程真实 A/B 及返回原效果 |
| sun / 朝向太阳 | 11 | 可见 flare 贡献、相同 A/B 序列，强制 Khronos validation |
| glass | 11 | 透明路径、相同 A/B 序列，强制 Khronos validation |
| grid | 16 | 四档 ratio × 两档 density × 两种 resolve；中途 resize 至 1023×767，强制 Khronos validation |
| gui | 1 | 实际搜索 `ordered dithering`，native 窗口显示八个声明式控件与统一 Save |

共 50 张原生 PNG。本地 HTML 带 35 张原始游戏 PNG（完整三组 A/B、GUI、一个 partial-block 样例），不重新压缩；所有原始 grid 证据保留在 `target/ordered-dither-native/`。图片、命令、阶段日志、SHA-256 与 validation 模式汇总在 [evidence.json](ordered-dither-assets/evidence.json)。日志本地日期为 2026-10-06，UTC 为 2026-10-05。

实际读图观察：

- 原 ±LSB 噪声基本不显眼；8 阶、强度 1 的 global Bayer 在天空、叶片、树干、地面都明显呈交错棋盘/线条。Halftone 形成团簇点/斜块，细节有不同程度的纹理遮盖。两者不是视觉无损替换。
- God Rays weight 在该场景较强，离散混合档会出现深蓝块；地形 ambient 独立开关较温和，不应把未变化的植被说成 bug 或声称覆盖所有接收者。
- 默认花园视角 flare 信号不明显；追加太阳视角后，`lens-flare.png` 可见光晕周围成规律点的明暗贡献，局部开关确实有效。没有把全部天空量化伪装成 flare。
- Glass global 能处理透明合成的最终颜色；局部 ray/flare 开关不把有序点烘进 canonical voxel cache。任一局部相机 A/B 开启时，整份 ray/flare 组合改从最终 screen UV 采样（也包括未量化项的采样位置），不是逐项保持另一效果 canonical UV 的完全隔离实验；全部关闭时保留原 cache 路径。UI Backpack/toolbar 仍是原生像素，不跟着 scene 格缩放。
- 不同截图世界仍在模拟，文本计数/植被/temporal 收敛可能不同；不宣称全图 bit-exact A/B。图案是 screen-fixed，物体/相机移动时可能 swimming；尚未做长时移动视觉审查。

另外直接分析真实 PNG 的 RGBA（ImageMagick crop → raw rgba）：9 张花园图左上 `1024×256+0+0` 区域，每张 4096 个 8×8 blocks 的内部非均匀数量为 **0**；Backpack `500×650+1920+60` 的 hash 全部为 `55c234e1db51d98eae781a7da8e090b459b4c49f63b91bc441eec65c6a0bbd77`。原效果、强度 0、返回原效果的静态天空 ROI hash 同为 `0592e9f1069e54e3f7392840fc469d7e972c30207b22bc265b6675172de979cc`。这是限定 ROI 的实测，不外推动态全图。可用 `magick <PNG> -crop '<ROI>' +repage rgba:- | sha256sum` 重查 hash。

## 验证与浏览器闭环

- `cargo fmt --check`、`CARGO_BUILD_JOBS=2 cargo check`、`CARGO_BUILD_JOBS=2 cargo build --release`：通过，Release binary 位于本工作区 `target/release/re-flora`。
- 完整 `CARGO_BUILD_JOBS=2 cargo test`：binary 1346 passed / 0 failed / 3 ignored，library 4 passed。忽略项不是本任务新增长时 GPU 测试。
- `CARGO_BUILD_JOBS=2 python3 scripts/run_slang_tests.py`：35 passed。新增 CPU Slang 性质覆盖分布/均值、端点、HDR/非有限边界、整份颜色比例、开关/零强度/global 优先、scene-cell/partial-block 坐标。
- GUI 真正 egui pointer 输入测试：五个 A/B 可在搜索结果点击，统一 Save/reload 持久化；现有 generic 遍历覆盖全部 saved controls，附加迁移测试验证老文件/不完整 section 保留用户值并补缺省。
- Shader-derived Rust structs 与 GUI generated fields 由 check/build 再生，未手改。没有改原模型渲染文件或青边修复算法。
- Native scripts 检查自己的日志、`failures=0`、无 ERROR / panic / VUID，并逐字检查 `config/gui.toml` / `config/camera_snapshots.toml` 未被运行写回。一次 GUI 验证原先在释放 GPU lock 后查 latest，读到了随后 glass 的启动日志；这是 runner 的日志归属竞态，不是应用失败。已把 `--latest-log` 纳入同一锁并重跑 GUI 成功，失败证据保留在 `target/ordered-dither-native/gui-pre-lock-fix`，未把它计为有效验证。
- 用独立 `agent-browser` session `ordered-dither-76062dbdf8ea` 直接打开 file://，开启 offline 后复查。33 张 native A/B 图均加载为 2560×1440；GUI 与 partial-block 图分别加载为 2560×1440、1023×767。实际操作了场景/效果下拉框、A/B 交换、比较 slider、1:1 checkbox、模拟 source/range/motion/reset。六个模拟 Canvas 在强度 0 时逐字节相同；非零强度的六种渐变结果 checksum 各不相同。offline 重载、打开本地 evidence 链接与 390px 窄屏检查也通过，无页面横向溢出、page error / console error；会话已关闭。

Canvas 明示是 JavaScript encoded-RGB 合成图模拟：原图、无空间量化、Bayer 2×2（仅网页）、Bayer 4×4、halftone 4×4、静态 hash threshold（仅网页，不是 blue noise 或 baseline ±LSB）。所有量化项同源图、同亮度阶数、同强度，不能拿它当 HDR/DDGI/玻璃的 GPU 证据。页面无 CDN 或网络资源依赖。

**没有专项 Release 性能测量或性能验收**，截图 FPS 不是 benchmark。候选无需等待美术批准即可交付；视觉批准、长期运动/全场景/其他 GPU 和平台审查仍未完成。局部天光只覆盖正常 hybrid 地形，不构成跨所有接收路径的 lighting rewrite。

## 复现 / GUI 比较

```sh
CARGO_BUILD_JOBS=2 cargo build --release
node scripts/validate-ordered-dither.mjs smoke
node scripts/validate-ordered-dither.mjs compare
node scripts/validate-ordered-dither.mjs sun
node scripts/validate-ordered-dither.mjs glass
node scripts/validate-ordered-dither.mjs grid
node scripts/validate-ordered-dither.mjs gui
```

脚本拒绝已有输出目录，重跑前移到本工作区其他路径保留旧证据。强制 validation 需设置本机 `VK_LAYER_PATH` 和 `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`，不要在无层的机器上宣称执行了 layer 检查。脚本自行获取 GPU lock，不需要嵌套另一个 flock。

游戏内：Debug Panel 搜索 `ordered dithering`。保持同一 Scene Pixel Sampling 档位/density/resolve，先只勾 `Global scene dither (A/B)`，切换 Pattern 两种选项；再取消 global，分别勾 God Rays / Camera Lens Flare / terrain ambient，flare 应朝向可见太阳。Sky background 单独比较背景，不把它叫天光。把 levels 从 8 改为 4 / 16 看规律变化；Strength 0 或取消勾选回到原效果。统一 Save 保存，全部新开关默认 false。
