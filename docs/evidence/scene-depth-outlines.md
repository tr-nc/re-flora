# 场景深度描边

实验分支 `experiment/global-low-resolution` 的独立深模块：

- Rust：`src/tracer/depth_outline.rs`，拥有 pipeline、参数归一化和 dispatch；调用方只提供场景尺寸与保存设置快照。
- Shader adapter：`shader/slang/depth_outline.slang`，读取场景不透明深度并就地混合场景颜色。
- Filter：`shader/slang/depth_outline_filter.slang`，拥有深度选择、投影重建、梯度判定与细节保护；纯逻辑 Slang CPU 测试与 GPU 使用同一实现。
- 拓扑只负责资源绑定及协调 resize descriptor generation retirement；没有独立的 resize/资源回收策略。

## 处理顺序

场景 composition / glass resolve → **depth_outline.pass** → tone mapping / 最近邻放大 → 原生 UI。描边固定一内部像素宽，不按窗口像素扩张，不为每个模型引入特例。没有新图集、历史缓冲或额外图像。

每个 invocation 只读写自己的颜色像素；邻居只读取深度，避免就地处理时的跨线程颜色反馈。反射资源跟踪提供 composition、描边和后处理之间的依赖。

## 避免常见问题

- **非线性设备深度**：用真实相机 inverse projection 重建 reciprocal eye depth；这等价于线性 eye depth 的倒数，不直接阈值化 device-depth 差。
- **斜面误判**：投影平面的 reciprocal depth 在屏幕上是仿射量，相向梯度的差为零。四组相向邻居（含对角）检测前景侧的梯度残差，而非直接使用梯度幅值。曲面或小折角仍可能响应，可通过阈值和柔化调整；不宣称能消除所有几何误判。
- **天空和外晕**：天空/无效深度使用零 reciprocal depth 哨兵；天空中心不描边。只响应近侧，sky silhouette 单独可减弱。
- **图像边界**：缺少相向样本时跳过该对；不以 clamp 重复样本制造假梯度。
- **植被黑点**：相对阈值、最小 world-space 深度间隙、低默认强度，加上细薄特征衰减。不会改变几何或模拟。
- **透明物体**：部分 alpha 的 raster 不作为不透明深度边缘；使用可用的 terrain 深度。当前深度缓冲没有保存被透明层遮盖的所有不透明 raster 层，故不承诺恢复全部隐藏轮廓，也不描透明玻璃边缘。
- **无效设置**：进入 shader 前拒绝 NaN/infinity、夹取合法范围，保证阈值柔化区间非零。

深度不能发现同深度材质边界或所有折角；没有增加法线目标来伪造这些信息。没有 temporal history，无法消除低分辨率摄像机运动固有的像素跳变；本轮不声称完成动态视觉验收。

## Debug Panel

`Scene Depth Outlines`（英文 UI）集中提供：

1. Scene: depth outlines：开关，默认开启。
2. Outline strength：混合强度，默认 0.25。
3. Outline color：sRGB 色板，默认 `#141E24`。
4. Depth discontinuity threshold (relative)：默认 0.04。
5. Minimum depth gap (world units)：默认 0.004。
6. Outline threshold softness：默认 0.5。
7. Sky silhouette strength：默认 0.6。
8. Thin detail strength (grass and leaves)：默认 0.25。

全部声明在 `config/gui.toml`，绑定统一生成字段、搜索和 Save；没有 App-only 控件或新增单项 save hook。用户原先调整的字段与 camera snapshots 保留。默认克制，最终外观仍需用户调参确认。

## 验证与测量

- `cargo fmt --check`、`cargo check`、完整 `cargo test`：1330 主程序测试 + 4 library 测试通过，3 ignored。
- GUI 保存/搜索/布局测试 75 项通过；新增 mapper 回归确认八项设置进入渲染快照。原生 Debug 搜索截图显示八项匹配集中在一个分组，UI 清晰且无裁切。
- Vulkan 库测试 57 项通过。
- Slang CPU 测试覆盖平面、近远斜面、遮挡前景/背景、天空、微小间隙、薄特征、inverse projection 及不透明/透明深度选择。全套 35 项。
- 隐藏静音 Release 启动、原生截图、开启 Vulkan synchronization validation 的连续窗口 resize 正常，无检索到的 ERROR/VUID/hazard，退出 `failures=0`。
- 同一 Release 二进制、2560×1440 窗口 / 320×180 场景，固定宽景 82,454 株，IMMEDIATE；保存开关分别启用/禁用。每档两次 300 帧，剔除边界四帧，592 样本。开关验证期间仅临时修改新增开关，随后恢复；已有用户参数未改。
- 初轮 filter（添加显式无效深度选择守卫前）：描边区间 p50/p95 **9/10 µs**，禁用后的空 profiling scope **1/1 µs**。GPU 整帧启用 **5427/5986 µs**，禁用 **5418/5987 µs**。整帧差异很小，不作为显著加速/回归的证据，也不推广为其他场景或分辨率的固定成本。
- 固定相机的开关截图相差 82,304 个输出像素，正好为内部像素块的整数倍；顶部 50 行天空差异为零。实际截图已查看。

日志/截图/原始测量在 `target/depth-outline/`；没有自动启动可见游戏，没有 push。
