# 游戏内实时像素蝴蝶：可体验版本

## 操作

在本工作区 `cargo run`。按 **R** 打开 Debug，找到 **Butterflies**：

- **3D Pixel Wings (off = Original Rendering)**：运行时 A/B。勾选新翼面渲染，取消恢复此前的渲染；不改变飞行方案。当前默认开启。
- **Pixels per Butterfly (N x N, All Distances)**：全局8–64，默认 **22**。每只使用相同规格，远近不会切换成更低规格。
- **Wing Animation FPS**：2–60，默认60；只量化拍翼，不量化相机或改变飞行物理。
- **Wing Self Shadows (Game Sun)**：翼面之间的太阳自阴影，默认开启。
- **Preview 7 Palettes Near / Mid / Far (Camera-relative)**：默认关闭。显式渲染调试样本，三排分别距相机0.25、0.5、1世界单位；原有7种配色，不必等待生态生成。它们使用真实世界光照/深度，会被地形或树遮挡。样本跟随镜头，不是新增生态个体；取消即可移除。

这些设置全部由 `config/gui.toml` 声明，复用统一 Save/加载机制。没有 App-only 设置或单独保存钩子。预览开关不会修改自然生成率；正常游戏中的蝴蝶出现仍由既有生态逻辑决定。

## 实现

源为已批准的 `blender-v5/butterfly-prototype.blend`：两块翼面，无身体、无边框、无表面异色色块。`experiments/butterfly-method-comparison/export-runtime-mesh.py` 从保存的 Blender 源导出 `assets/butterfly/wing-mesh.json`，包含156个三角形、26个关节动画关键帧、源SHA256。不是方向图集或渲染图片。JSON嵌入二进制；不要手改生成的模型数据。

`src/tracer/butterfly_mesh.rs` 负责动画、朝向、配色和实例数据：

- 保留每个实例的位置、朝向和独立生命周期相位种子，两面/侧缘使用同一个原有 preset 的 mid-shade 基色。
- 使用同一秒循环和源25Hz关节关键帧插值，再按2–60FPS量化取样。相位使用独立渲染时间，不被世界更新节拍限制。
- 156个三角形变换到世界空间。相机空间中的固定包络不随拍翼重新贴合；投影前裁至实际近裁剪面，避免眼平面除零/突变。屏幕边缘不重新拟合像素格。

`butterfly_tile.comp.slang` 每个实例、每个原生纹素发出一条当前游戏相机射线，取最近的真实翼面交点。所有实例批量 dispatch，一次生成颜色与实际投影深度：

- 全局N×N与距离无关。不是把整张屏幕降分辨率，也不是一只一个 renderer/draw call。
- 接游戏太阳方向、颜色与亮度，真实面法线的 Lambert 明暗；读取世界/树叶/云阴影，并接入当前 DDGI / 原有环境光模式。
- 翼面自阴影沿游戏太阳方向检测本实例三角形。不使用网页固定方向光，不把暗色后画在像素边缘。
- 颜色/深度放在共享结构化存储缓冲的小格中，再通过 `butterfly_tile.vert/frag.slang` 批量合成；整数寻址，无 mip 或线性过滤。每个有覆盖纹素写回几何交点深度，接入既有 terrain-depth-prefill / raster / hybrid-composition 链。
- 透明区域遵守既有粒子管线的预乘零色＋深度1合同。显式顶点/实例流沿用当前 Vulkan 能力集，不增加 shaderDrawParameters / shaderDemoteToHelperInvocation 要求。
- 生命周期淡出按远到近提交；不是透明翼材质。相交透明物体的通用排序并非本次解决范围。
- 计算结果→片段读取的资源准备在 render pass 外；加入 extent descriptor 更新以及 DDGI publication consumer registry。

每格预留64²地址，实际只计算N²。池为256只（约19 MiB GPU实例/三角形/纹素资源；当前正常花园蝴蝶上限16）。超出池会明确报错而不是静默降低分辨率；普通粒子仍更新，不留上一帧数据。若以后扩大生态上限，需要同步做池增长/分批设计及性能验证。

## 验证方法

```sh
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
python3 scripts/validate_butterfly_mesh.py --seconds 8
cargo run --release -- --latest-log
cargo run --release -- --tail-latest-log 200
```

显式 GPU fixture 通过 `RE_FLORA_BUTTERFLY_MESH_REVIEW=sweep` 激活，固定视觉时间，依次切换22→8→64→旧方式→22无自阴影→22有自阴影；不保存改过的参数。它不是正常单元测试。会回读生产 GPU 纹素，检查全部有效覆盖点的深度与 CPU 最近三角形射线结果，导出每只原生尺寸的诊断 PNG。诊断PNG为夹取后的线性RGB，不是游戏最终色调映射截图。

`cargo fmt --check`、`cargo check` 通过；`cargo test` 为 **1031 + 4 passed，2 ignored**，包含全部声明设置的保存/重载回归、57种全局分辨率与59种相位采样的纯逻辑检查。

本次 RTX 3060 Ti、release、隐藏静音验证：

- 21只真实提交/dispatch，不依赖自然生成。
- 22×22的736个命中点、8×8的63个、64×64的6234个，与CPU深度计算吻合。最大深度误差约 `4.1e-6`，全部浮点有限、深度合法。
- 运行时开关/分辨率/动画FPS/自阴影变更无 Vulkan validation error、panic 或资源访问错误，正常 `failures=0` 退出。
- 原生PNG按各实例检查尺寸；截图确认新翼面像素格确实进入游戏画面，且地形遮住部分远排样本。
- 默认22×22、21只样本的 `butterfly.tiles` GPU scope 在一次短跑中中位数约 **0.104ms**。这是小格生成一项的初测，不是整帧增量、完整A/B基准或最终性能验收；诊断回读本身也不属于正常运行。
- 原有自然场景10秒检查未生成蝴蝶，因此明确不把该空场景当作新渲染路径的验收。

日志/截图：`target/butterfly-resume/`；原生回读：`target/butterfly-resume/game-tiles/`。生成 Rust 变更仅来自 GUI 构建生成器。

## 边界与后续

- 固定22×22是内部画面规格，不能阻止远到只占几个屏幕像素时丢失可见细节；仍保持正常近大远小。
- 低至8px、接近侧对镜头的薄翼可能一个纹素都采不到。这是原生低分辨率几何采样的边界，不通过加身体或描边掩饰。
- 场景/太阳阴影**落在蝴蝶上**与翼面**自阴影**已接入；蝴蝶向地面、植物、其他蝴蝶投影尚未接入，不声称已实现。
- 逐显示像素进行场景深度测试，因此遮挡可以切开放大的像素块；优先不穿地形。
- 可体验不等于已完成所有视觉/性能验收。先在真实游玩中确认轮廓、相位、尺寸、配色与太阳效果，再独立开展性能预算验收。
