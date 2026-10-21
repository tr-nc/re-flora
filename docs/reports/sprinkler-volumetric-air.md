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

## 16 源初步 Release 计时

实现提交 **`7cbb2ec3`**。使用同一个 `--features no_validation_layer` Release 二进制，16 源 / spread / 30% duty / 各模式三次，共 **6 次 22 秒运行、5135 个暖机后 GPU 样本**；每次 709–983 个样本、16.258–16.822 秒计时窗口。排除最初五秒及 frame-180 回读之后一秒。交错顺序 original→air、air→original、original→air；没有删掉较慢或有警告的重复。计时期间世界与风正常推进，各 run 不是同一瞬间的粒子帧重放；相同的是配置、源布局、场景、相机和时钟规则。暂停的同状态截图/不变量验证与这组性能运行分开。

本次设备 RTX 3060 Ti，实际 **4096×2560 native / 820×512 internal / stride 5 / 自动 FIFO**。加载同一 130190 flora / 4 trees 花园，使用 `garden-arrival`、保存的 30° / 5 RPM / 5 Hz / mist 6，保留完整水滴、灌溉与实际 16 槽音频处理，输出静音。

这是本次日志读取的分辨率；上一轮消融测量是 5120×2880 / 1024×576。没有修改桌面或画质来复现旧环境，**不能用上次的 16.841 ms 与本次作跨分辨率加速比**。

每项为三个 run 的相应统计值的算术平均；p50/p90 不是跨 run 池化分位数：

| GPU 项目 | 原模式 mean ms | 空气近似 mean ms |
|---|---:|---:|
| 密度场清零、bounds、沉积 | — | **0.331** |
| 体积光（原大气；新模式还包含局部雾） | 0.095 | **0.574** |
| 原大气时域 pass | 0.031 | 0.031 |
| 最终光学 pass（原：雾+水滴；新：水滴+合成） | 11.812 | **3.272** |
| **上述全部光学 + 大气，逐帧求和再统计** | **11.938** | **4.208** |
| 完整 GPU 帧 mean | **21.531** | **13.485** |
| 完整 GPU 帧 p50 / p90 | 21.067 / 26.066 | 13.497 / 15.041 |
| **实际 CPU 帧序列 cadence FPS** | **43.86** | **58.31** |

光学+大气总范围平均减少 **7.730 ms（64.8%）**；整个 GPU 帧平均减少 **8.046 ms（37.4%）**。完整帧差包含其他场景阶段的波动，不能全部归到雾；FPS 来自实际帧号/时间，不是 GPU 时间倒数。这是外观近似的候选收益，不是等画质无损加速、普遍 60 FPS 或正式性能验收。

水/源阶段仍在运行：prepare **0.232→0.232 ms**、wetting **0.699→0.699 ms**、硬件 **0.671→0.672 ms**。共享 source tiles **0.009→0.009 ms**。剩余约 3.27 ms 的末端 pass 仍包含完整精细水滴，不应把它误报成新雾的独立成本。

每轮光学+大气 mean：
- 原模式：11.890、11.883、12.043 ms；
- 空气近似：4.223、4.213、4.188 ms。

**计时警告未隐藏：**最后一次 air 运行记录了 12 条 PetalSonic underrun 警告（累计 17 次 underrun）；前五次没有此警告。6 次均正常退出，无 ERROR/VUID/SYNC-HAZARD/panic。保留该重复的全部计时和警告，不宣称声音无回归或整体验收通过；根因与是否由更高帧率间接触发尚未确定。这里只作 GPU 成本/实际 cadence 的有限测量，没有可听音频验证。

证据及复现：
- [原始统计、输入/二进制哈希](../../target/sprinkler-air-review/measurement/summary.json)；同目录保留 6 份完整日志。
- 二进制 SHA256：`1ea8cba10946ee5096d71f23fee44d86ba1ef3ce4c0f6542d48c62e2cad72ca1`，仅指计时用无验证层版本，不是交付的默认 Release。
- `node scripts/benchmark-sprinkler-air.mjs --help` 给出完整参数与失败恢复。常用复现：

```sh
flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -c '
  cargo build --release --features no_validation_layer &&
  node scripts/benchmark-sprinkler-air.mjs --counts 16 \
    --output target/sprinkler-air-review/NEW-measurement
  cargo build --release
'
```

统计器复用原消融日志解析，新增可命名的**逐帧**聚合范围；9 项 Node 测试通过，包括反相关两个 pass 的 p90 不能相加、CLI 恢复、实际分辨率、暖机/回读窗口及缺失 scope。计时后已恢复默认验证层 Release。单源/其他数量、100% duty、重叠、其他相机/显卡和完整动态美术验收尚未覆盖；下一步应先由玩家比较实际观感，再决定是否采用或继续优化。
