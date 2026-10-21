# 喷雾接入体积光：可切换的空气散射近似

2026-10-22。本轮是明确允许外观近似的渲染候选，不是原算法的无损优化，也不是正式发布。默认保留原模式；没有推送、可见窗口或可听播放。

## 使用与画面

**Debug → Mist & Scattering → `Sprinklers: volumetric-air mist (unchecked: original)`**。

- 不勾选：原来的逐核解析喷雾。
- 勾选：粗密度场 + 现有体积光摄像机射线、深度及太阳阴影采样。
- 仅适用于 directional mist；旧 soft-dome 对照保持原路径。
- 普通 Save 保存开关；旧配置只补默认 false，不自动写文件、不重置其他值。
- 强度、细水滴开关继续独立。关闭全局 god rays 不会把局部喷雾一起关掉。

已打开检查 [同一喷射状态的左右对照](../../target/sprinkler-air-review/comparison.png)：左为原模式，右为新模式。新模式更柔软、细喷束更弱，不是像素等价。
[生命周期截图](../../target/sprinkler-air-review/lifecycle.png) 按从左到右、从上到下为：恢复原模式、关闭全局光柱、关闭全局粒子、雾强度零、占空比零、恢复运行。

这些是候选画面，不代表玩家已经认可。图片仅缩放、拼接和加标题，没有调整曝光。

## 所有权与实现

1. 原 `sprinkler_mist_prepare` 仍生产每源 **256 个雾核、128 个独立水滴槽及原始灌溉接触**。水量、第一接触、时钟、音频和硬件不以新开关分叉。
2. `sprinkler_air_bounds` 归约当前活跃雾核的立方包围域及质量加权环境光；所有喷头共享一个 **96³** 场，不为每个喷头分配体积。
3. `sprinkler_air_splat` 将核密度累加进整数场。亚网格核适度展宽，解析质量在地形裁切和量化之前保持。每帧完整清零，未知地形或查询预算耗尽拒绝沉积；地形查询沿用现有真实体素占用，不用屏幕深度冒充碰撞。
4. `god_ray_air.slang` 是 `god_ray.slang` 的变体，共享原摄像机射线、场景深度及太阳阴影函数。局部雾只在密度场射线区间采样，最多 96 次、三线性重建；不是在每个光照采样点再遍历所有雾核。
5. 全局大气仍写原 R32F 输出、保留原时间滤波。局部雾写单独 RG32F 的总光学厚度/受光厚度，**没有雾的时间历史**，不把 5–20 Hz 脉冲拖成残影。
6. 原末端光学 pass 跳过雾核循环、保留精细水滴解析积分，将两者的厚度和光源项相加，**只作一次 Beer 合成**；不重复画原雾，不把两层独立 alpha 相叠。

主要源码：

- `shader/slang/sprinkler_air_{math,sample,bounds.comp,splat.comp}.slang`
- `shader/slang/god_ray{,_air}.slang`
- `shader/slang/sprinkler_mist.comp.slang`
- `src/tracer/{sprinkler_air,sprinkler_resources,pipeline_builder,extent_dependent_resources}.rs`
- `src/app/core/sprinkler_air_review.rs`

资源归 acquired frame slot 所有，首次启用时才分配密度缓存，每槽 3,538,944 bytes（3.375 MiB），最多三槽 10.125 MiB；停用保留已分配缓存以便实时切换。屏幕 RG32F 纹理随内部渲染分辨率重建（820×512 时约 3.20 MiB），无新增历史纹理。原模式不运行新场构建/积分，但保留这个纹理分配和新管线对象。

定点精度为密度的 1/64，单核单格贡献显式限于 65535；256×256 个核全叠加仍小于 uint32 上限。二维 dispatch 的两轴分别为 256 核和至多 256 源，避免平铺 65536 工作组超过 Vulkan 常见单轴 65535 限制。不存在截掉后几个喷头或水滴的溢出路径。

## 有意的近似与限制

- 不做真实折射、雾的自阴影或多次散射；采用一个质量加权环境光近似和现有体积光的简单太阳阴影。
- 场是粗网格：细喷束、近嘴形状、薄墙与台阶边缘会变软。沉积中心接受真实地形检查，但三线性重建**不等价于原逐像素地形裁切**，可在一个网格尺度内跨薄障碍混合。
- 多个远离的源会扩大共同包围域、降低局部空间分辨率；域随喷雾变化也可能产生重采样变化。没有声称任意布局/相机/256 源都有相同画质。
- 玻璃仍只积分第一玻璃界面之前的空气，不假装求出了折射后的雾。
- 未减少真实 256 个灌溉/喷射采样或 128 个水滴槽；颜色/形状近似不应被描述为无损。
- 本轮普通光学消融诊断仍强制原渲染器，不能与新 A/B fixture 混用，避免污染旧的雾/水滴成本归因。

## 验证

- `cargo fmt --check`、`cargo check`；完整 `cargo test`：**1678 app + 4 library + 1 integration** 通过，既有 11 app/4 integration ignored 保留。后续二维 dispatch 修正又跑了 sprinkler 定向测试和 Release 构建。
- **51 个 Slang CPU 测试**通过，包括新场的空域、边界、相机在域内、平行射线、场景截断、展宽质量、定点误差及单次合成。
- 配置迁移/Save round-trip 测试覆盖缺字段、false、true，不写入旧文件、不改变其他偏好；真实 render-input mapper 验证仅空气模式字段变化。反射布局测试覆盖三个新 pass 的字段偏移及 padding。
- 默认验证层 Release + 隐藏静音、同步验证的最终 16 源花园运行：原→新→原、暂停、全局 god rays 关闭、粒子关闭、强度零、阀门零、恢复运行，完整退出，无 ERROR/WARN/VUID/SYNC-HAZARD/panic。
  - 暂停 A/B 的全部水接触以及水滴几何字节完全相同。
  - 暂停新场在两次回读中 `live=11726`、`sum=85988`、hash `75e96313ff0f1e25` 相同。
  - 关闭阀门后 `live=0`、`sum=0`、header invalid；恢复后重新出雾。
- 另外同步验证运行通过：单体素墙、相机在雾中、resize（含不同分辨率）、源增删生命周期。日志含真实 `sprinkler_air.field` / `god_ray.air` scopes，不能仅靠设置为 true 算覆盖。
- GUI 仅新增一个 false 的声明；相机库及输入 garden snapshot 的 SHA256 未变化。

证据：`target/sprinkler-air-review/`，包括 `garden-final.log`、`garden-final/*.png`、`native/rotary-wall.log`、`native-rest/*.log`、`all-tests-final.log`、`slang-tests.log`、`tests-sprinkler.log`。所有截图均已打开检查。

### 保留的失败和验证边界

1. 第一轮暂停比较失败：只冻结世界时钟不足以冻结独立视觉时间驱动的风场。现在仅在 opt-in 对照 fixture 中持有真实上传风场快照；原生产风/时间行为未改，严格的接触、水滴及场哈希断言随后通过。失败日志为 `garden-switch-first.log`。
2. 初版布局测试误写 24 bytes；反射布局含 8 bytes padding，实际为 32。修正测试后完整重跑通过，没有手改生成文件。
3. 首轮同步验证使用已弃用的 layer `enables` 设置，触发设置警告；改用仓库现有 `validate_sync=true`，保留 `native-deprecated-settings/`，未把那轮算作通过。
4. 既有 `mist-edit` fixture 的“移除墙后必须有核越过 X 平面”断言失败；在**同一构建的原模式**也复现（`mist-edit-original.log`）。因此本轮不把该编辑 fixture 算作通过，也不声称已定位其历史引入点或完成该测试的修复。其他原/新对照不能替代这项受阻的地形编辑端到端验证。
5. 中间一次 22 秒切换运行在断言全部通过之后记录了 PetalSonic underrun 警告（`garden-switch.log`）；最终 16 秒同步验证切换运行没有这些警告。没有隐藏那次警告，也未凭重跑就认定音频问题已解决。

## 复现可视对照

先 `cargo build --release`，然后在同一工作树、串行 heavy lock 内运行：

```sh
RUST_LOG=info \
RE_FLORA_GARDEN_PERF=1 \
RE_FLORA_SPRINKLER_REVIEW=rotary-perf \
RE_FLORA_SPRINKLER_PERF=16,spread,30 \
RE_FLORA_SPRINKLER_AIR_REVIEW=switch \
RE_FLORA_SPRINKLER_AIR_CAPTURE=target/sprinkler-air-review/NEW-directory \
target/release/re-flora --hidden --mute \
  --terrain-load target/garden-delivery/world/implementation/garden-final.rflterrain \
  --camera-snapshot garden-arrival --auto-exit 22
```

捕获目录必须不存在。存在 DISPLAY 时只在子进程取消 Wayland 变量。`original` / `air` review 固定一种模式，不暂停世界，也不自动截图。`switch` 才冻结喷射时钟和上传风、切换控制并做回读，不适合性能计时。

性能比较应把**新场清零/构建、共享体积光、新末端水滴合成、原全局体积光和时域 pass**纳入一致的总范围，再报告整个 GPU 帧及真实帧率；不能只展示变小的 `sprinkler_mist.pass`。视觉验收和进一步性能优化仍是后续独立阶段。
