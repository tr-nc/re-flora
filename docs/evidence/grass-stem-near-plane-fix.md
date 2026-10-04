# 草茎代理近裁切修复与 Release 验证

## 结果

修复了[定位记录](grass-stem-fullscreen-performance-diagnosis.md)中的异常全屏代理，
没有关闭解析求交、降低采样质量、减少草数量或丢弃跨 near 的草。

- 实现：`42ed85f4`；长期保留的回归镜头／测量脚本：`a6ced8d6`。
- 低位俯视的 B 草绘制 GPU p50：原 **66.787 ms → 0.994 ms**，约 **67 倍**改善。
- 贴近斜视：原 **38.611 ms → 1.944 ms**。
- `top`、`low`、`near` 和强弯曲 `near-curved` 的固定帧 B 截图，与修复前
  的 PNG **逐字节相同**；不是靠删草、改变遮挡或降低质量换来的改善。
- 运行时 Debug A/B 保留，默认值不变。**B 仍慢于原体素路径 A**，不宣称
  新渲染机制已经胜过 A，也不把这次修复当成完整性能／视觉验收。

## 正确修复

改动唯一的投影所有者 `shader/slang/stem_proxy_projection.slang`，草与花茎共用：

1. 将膨胀后的模型空间盒子八个角点变换到齐次裁切空间。
2. 对六个视锥平面计算最大有符号距离；只有某一平面外包含全部角点时，
   才拒绝整个盒子。包括完全位于 near 前、eye 后、far 后和画面侧面的盒子。
3. 保留 `z >= 0` 的角点；对于跨 near 的盒子，额外计算十二条盒边与 `z=0`
   平面的交点。只对正 `w` 的保留顶点进行透视除法。
4. 由这些顶点的投影极值构造保守矩形，裁到屏幕范围，拒绝空矩形。

near 裁切后的盒子是凸多面体；在正 `w` 区域，透视投影极值发生在它的顶点。
这些顶点正是保留角点和边／near 交点。因此无需全屏回退，也不能仅删除
near 后方角点来代替边交点。真正包住相机可见体的盒子仍允许生成全屏代理。
侧面平面拒绝是保守测试，不是宣称完成 OBB／视锥的精确相交剔除。

解析求交、采样网格及其 bounds margin、风姿态、颜色／光照缓存、片元的
真实深度写入均未改动。没有新增 GPU ABI 或手工修改生成文件。

## 回归测试

新增 `shader/tests/stem_proxy_projection_test.slang`，直接调用生产投影函数。
修复前运行返回 **exit 1**；原定位最小复现重新运行返回 **exit 9**。
修复后两者均返回 **exit 0**。

覆盖四侧屏幕外的 near crossing、完全位于 near 前／eye 后／far 后、真正
跨 near／eye、普通可见盒子、合法全屏代理，以及 64 组确定性旋转／缩放／
位移盒子的网格采样（pixelization bounds inflation 开／关，共 128 组）。
每个可见采样点都必须落在生产代理内；不依赖复制一份待测投影算法。

代理面积在最小测试层面的可核算结果（2560×1440，不是硬件调用计数）：

| 测试 | 原异常全屏代理数 → 修复后 | 矩形覆盖面积 → 修复后 |
|---|---:|---:|
| 定位中的右侧屏幕外盒子 | 1 → 0 | 3,686,400 → 0 pixel² |
| `[-.001,-.002,-.02]..[.003,.004,-.005]` 跨 near 盒子 | 1 → 0 | 3,686,400 → 221,184 pixel²（屏幕的 6%） |
| `[-1,-1,-1]..[1,1,1]` 真正包住相机的盒子 | 1 → 1 | 3,686,400 → 3,686,400 pixel² |

第二行仍然绘制部分屏幕矩形，而不是剔除整个盒子。
这些面积是投影矩形几何面积；不能当成 GPU fragment invocations 或实际命中率。
本次未加入 GPU 原子计数／pipeline statistics，完整草场景的全屏代理数量、
片元调用数和求交命中率仍未直接测量；不从 GPU 时间反推出这些数值。

## Release GPU 测量

RTX 3060 Ti，2560×1440，X11，Release 真实应用，隐藏静音，GPU 运行串行。
相同九块草丛：高草 2,267、矮草 1,831。每个 A/B 对人口相同。
固定 1/60 时间、成熟生长、惯性风响应、采样分辨率 45；普通弯曲 0～2 voxel，
`curved` 为 4 voxel。沿用定位记录的 near/top/low 镜头。
新增 `inside`：相对 focus `[0,0.008,0.0001]`，进入草丛；`low-curved` 为低位强弯曲。

10 个镜头／物种组合，各 A/B 两轮，第二轮倒序，共 **40 次运行**。
每次 120 帧预热、300 帧采样，排除开始四帧；表格各模式汇总 **592 个 GPU 样本**。
测量 scope 为 `graphics.flora` 和 `frame.render`，不以 CPU／present 等待冒充 GPU。

单位：ms。

| 场景 | 草 A p50 / p95 | 修复后草 B p50 / p95 | 全帧 GPU A / B p50 |
|---|---:|---:|---:|
| near-both | 0.096 / 0.109 | 1.944 / 1.964 | 5.779 / 6.426 |
| mid-both | 0.089 / 0.095 | 0.944 / 0.961 | 6.177 / 6.339 |
| far-both | 0.035 / 0.039 | 0.271 / 0.285 | 6.146 / 6.164 |
| mid-tall | 0.067 / 0.071 | 0.759 / 0.784 | 6.155 / 6.286 |
| mid-short | 0.028 / 0.035 | 0.261 / 0.272 | 5.834 / 6.164 |
| near-curved | 0.089 / 0.105 | 2.282 / 2.295 | 5.680 / 6.838 |
| top-both | 0.092 / 0.105 | 1.094 / 1.280 | 5.630 / 5.985 |
| low-both | 0.083 / 0.111 | 0.994 / 1.181 | 5.591 / 5.912 |
| inside-both | 0.083 / 0.103 | 0.787 / 0.956 | 5.549 / 5.755 |
| low-curved | 0.089 / 0.105 | 1.246 / 1.424 | 5.601 / 5.975 |

原 shader 的 top/low/near 长窗口结果来自定位记录；修复后是两轮汇总，
不是声称对环境噪声做了完美隔离。普通中远景剩余成本基本维持原量级。
未优化逐顶点 pose、求交循环、SV_Depth 或 LOD；这些应独立评估，不能把
修复前的错误覆盖当作它们的基线。5 ms 仅沿用低位俯视的诊断报警线，
不是新增项目性能预算。

## 视觉、共享花茎与限制

- `top-both`、`low-both`、`near-both` 对定位基线截图逐字节相同；
  `near-curved` 对原草 A/B 基线截图逐字节相同。
- 检查了 `low-both`、`inside-both`、`low-curved` 的 B 截图：仍有密集草覆盖，
  草之间可见真实地面。片元深度实现保持不变，前三组精确截图对比覆盖了
  原有 terrain／草遮挡结果；不是独立深度 buffer 读回测试。
- 共享花茎额外完成六次 native 隐藏静音运行，五张固定镜头截图与一轮
  16 阶段 live wind／相机 sweep（固定、绕转、dolly、near），并成功发布 resize。
  各日志无 ERROR／panic／VUID，关机 `failures=0`。
- **原有 `validate-stem-sampling.mjs` 不是全绿**：默认日志过滤会隐藏其期望
  的诊断行；加 `RUST_LOG=info` 后，六个应用均成功，但脚本仍断言旧的四组合
  pixelization 开关，而当前 fixture 的 `policy()` 已固定 combined 模式。
  未顺带改动这份旧脚本；对实际日志另行验证 16 阶段、当前 combined 配置、
  live 相机变化、resize 和干净关机，通过。不能宣称原脚本通过。
- 未做用户视觉确认、跨 GPU、硬件 overdraw 统计或动态帧序列差分；静态截图
  相同和相机 sweep 无错误不等于已证明所有近裁切运动都无闪烁。

## 验证与复跑

通过：`cargo fmt --check`、`cargo check`、`cargo test`（1,356 app 测试及 4 个辅助
测试通过，4 ignored）、全部 **32 Slang CPU tests**、`cargo build --release`、
`cargo run --release -- --hidden --mute --auto-exit 0.5` 及其日志检查。
40 个 GPU 测量运行全部退出成功、`failures=0`，无 ERROR／panic／VUID。
用户 GUI 配置／camera snapshots 未纳入提交；花茎验证前后 SHA-256 未变。

```sh
cargo check
cargo build --release
python scripts/run_slang_tests.py
node scripts/validate-grass-stem-ab.mjs
```

仅重跑此次症状：

```sh
env -u WAYLAND_DISPLAY RE_FLORA_GRASS_STEM_REVIEW=low-both-b \
  target/release/re-flora --hidden --mute --windowed --perf --authored-flora-bench
```

无需再临时改 fixture。将 low 换成 top／inside／near，或 both 换成 curved；
末尾 a/b 始终只切换渲染模式。

本地证据：

- `target/grass-stem-ab/summary.json`、`<case>-{1,2}.log`、`<case>.png`。
- 原六组合基线附件保存在 `target/grass-stem-diagnosis/pre-fix-ab/`；定位基线
  仍在 `target/grass-stem-diagnosis/baseline/`。
- `target/grass-stem-diagnosis/fix-visual-comparison.json`：精确 PNG hash 对比。
- `target/grass-stem-diagnosis/fix-*.log`：构建、单元测试、Slang、smoke、GPU 测量。
- `target/stem-sampling-review/`：共享花茎 native 日志与截图。
