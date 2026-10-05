# 场景深度描边

分支：`experiment/global-low-resolution`。模块为场景后处理，不按模型或绘制后端另建描边路径。

## 唯一场景深度入口

composition 同时合成颜色和 `scene_depth_tex`：tracer 地形、光栅模型以及可见部分透明度的表面按同一最近可见深度规则收口。Glass resolve 在实际绘制前景玻璃时更新该深度。描边只读取此目标，不再自行拼接 raster/tracer 深度。

`shader/slang/scene_depth.slang` 是可见深度选择的唯一实现。composition 的颜色与深度共享有效深度规则；天空记为 1。新目标为内部尺寸 R32F，320×180 时约 0.22 MiB；与窗口 resize 一起创建并通过已有拓扑协调 descriptor generation retirement。

处理顺序：composition / glass resolve → depth_outline.pass → tone mapping / 最近邻放大 → 原生 UI。

## 深模块

- `src/tracer/depth_outline.rs`：pipeline、保存设置快照、输入归一化和 dispatch；不暴露逐模型操作。
- `shader/slang/depth_outline.slang`：读取唯一场景深度、邻域采样、就地混合场景颜色。
- `shader/slang/depth_outline_filter.slang`：投影重建、遮挡跳变/连续折角区分、斜面/天空/细节保护。

每个 invocation 只读写自己的颜色；邻居只读深度，无跨线程颜色反馈。中心及四组相向邻居判定边缘，外圈用于判定局部连续性，不把外圈边缘膨胀到仍为平面的中心。最多 17 个深度采样，没有模型图集或历史缓冲。

## 本次修复的两个根因

1. **遗漏一类折角**：原三个样本的有符号残差只取正值。正负曲率并不等于前景/背景，连续地形折角也可能是负值。增加相向平面外推及视空间切向转角判定；正负折角和镜像邻域都有回归。平面上的像素仍不描边，遮挡远侧不生成第二圈外晕。
2. **GPU 实际丢失 tracer 深度**：Slang 2025.23.2 的 `-O3` 将两个独立 guarded-ternary 的 min 错误折叠为 raster 选择值与自身的 min。原 shader 重编译后的 SPIR-V 和新入口最初的错误产物均确认存在 `FMin %x %x`，tracer 依赖消失。CPU / `-O0` 正确，CPU 测试无法抓住该 GPU 错误。可见深度改为显式逐路 nearest accumulation；优化后产物保留两路输入，原生深度诊断及 tracer 墙体截图已确认。

新增 `scripts/check-scene-depth-artifact.mjs` 检查优化后 composition 输出对两个深度 producer 的 SSA 数据依赖。错误产物先失败、修复产物再通过；同时检查普通和 Glass composition。它不是完整 SPIR-V 语义证明，仍配合真实 Vulkan 运行及截图。没有升级/声称修复整个编译器，也没有对所有其它 shader 做同类全局审计。

## Debug Panel：Scene Depth Outlines

全部声明在 `config/gui.toml`，绑定统一生成字段、搜索和 Save。开关、强度、颜色、相对跳变阈值、最小 world-space 深度间隙、柔化、天空强度、细薄细节强度均保留。

新增 **Crease angle threshold (degrees)**，默认 45，范围 5–180；控制连续几何折角，180 关闭这一分量，不关闭遮挡边缘。这与深度跳变阈值分开，避免用户把跳变阈值调高后意外抹掉地形折角。

已有用户调参和 camera snapshots 未重置。强度/细节强度为 1 时植物会出现密集深色细节；这是保留的用户设置，不声称最终外观已获批准。

## 安全与限制

- 由 inverse projection 重建视空间位置和 reciprocal eye depth，不直接阈值化非线性 device depth。
- 投影平面的 reciprocal depth 仿射，视空间采样点共线；斜面不因坡度而产生描边。
- 天空中心不描边；图像外样本不伪装天空、不重复 clamp 样本。
- 细薄结构可衰减，NaN/infinity 与越界设置在 shader/参数入口处理。
- 部分透明表面参与当前可见层深度，不恢复多层透明后隐藏的所有几何。屏幕特效/阴影颜色边界没有独立几何深度，UI 不参与。
- 不能识别同深度材质边界或恢复子像素细节，没有 temporal history，摄像机运动固有的像素跳变仍可能出现。

## 验证与成本

`target/depth-outline-fix/` 保存证据：

- 原滤波回归先以 exit 22 失败；修复后全套 35 Slang CPU 测试通过。
- GPU 产物检查：`artifact-red.log` 失败，`artifact-green.log` 和 `final-artifact.log` 通过。
- `raw-tracer-depth.png`、`merged-depth-diagnostic.png`、`merged-depth-fixed.png` 分别显示原始地形深度正常、错误汇聚丢失地形、修复汇聚保留地形；诊断 shader 修改已全部移除。
- `terrain-walls-fixed.png` 已查看：tracer 墙体与地面交界、门洞、前景植物进入同一描边路径。
- 完整 Rust：1331 主程序 + 4 library 测试通过，3 ignored。保存快照回归不再依赖用户配置中的描边默认值。
- 原生 Release 隐藏静音启动、同步验证 resize、tracer walls 和 Glass fixture：无检索到的 ERROR/VUID/hazard，退出 failures=0。
- 同一 320×180、82,454 株宽景，Release / IMMEDIATE，两次 300 帧，剔除边界四帧后 592 样本：当前 `depth_outline.pass` p50/p95 **13/14 µs**。旧版 9/10 µs 数据来自遗漏 tracer 的实现，不再作为完整场景描边成本。

无自动可见游戏启动，无 push；Release 二进制从此分支构建。
