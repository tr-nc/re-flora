# 草茎实验：俯视满屏草时的性能骤降（历史定位记录）

> 后续已修复异常全屏代理；实现、回归测试与 Release 重测见
> [草茎代理近裁切修复](grass-stem-near-plane-fix.md)。下文保留修复前的调查证据，
> 「未修复／不实施」描述的是当时的调查范围，不是当前代码状态。

## 状态与范围

- 用户反馈：正对地面、画面几乎全是草时，性能特别差。
- 调查基线：`3a34072f3ceed7d18e5e12302a3e198a571c9193`。
- A：原来的高草／矮草体素网格及远距离 billboard；B：实验性的像素化解析茎代理。
- **本次只调查和记录，不修复、不调整默认值。** 所有临时 Rust／shader 实验已撤回；仅提交本文档。
- 之前的完整 A/B 测量见 [grass-stem-rendering-ab.md](grass-stem-rendering-ab.md)。

## 核心结论

已确认一个会把小规模像素开销放大为灾难性开销的路径：

> 逐株代理的包围盒只要触碰／跨过近裁切面或眼平面，当前投影函数就把代理
> 变成全屏矩形；这个处理发生在屏幕横向／纵向范围裁切之前。
> 因此，**完全在画面侧面之外的草也可能变成全屏代理**。
> 每个代理的片元仍执行草茎求交，未命中时才返回透明色和远深度。

低位俯视场景中，临时取消全屏回退，将草绘制 GPU 中位数从 **65.840 ms**
降到 **1.029 ms**。同时，保留回退、仅绕过求交，仍需 **15.997 ms**。
这支持「代理覆盖异常膨胀 × 片元求交成本」是此次骤降的主要组合原因，
而不是单纯顶点数量或光照缓存更新成本。

**取消回退不是正确修复**：它没有正确处理近裁切面交点，可能漏画。
绕过求交也会改变覆盖和遮挡。两者都是已经撤回的诊断实验，不能作为
性能验收、未来可达到的帧时间或保持画质的优化结果。

没有扩成全屏的较高俯视场景，B 仍明显慢于 A；因此即使未来正确解决回退
问题，也不能据此宣布新机制已优于旧机制。

## 复现与测量条件

- Release 真实应用运行，NVIDIA GeForce RTX 3060 Ti，2560×1440。
- `--hidden --mute --windowed --perf --authored-flora-bench`，X11；GPU 测试串行。
- 使用现有 `GrassStemReview`，通过生产画草路径生成九块固定草丛。
- 正常草数量：高草 2,267、矮草 1,831；每个 A/B 对数量相同。
- 固定帧时间 1/60；生长 potential=1、启用惯性风响应、自然弯曲 0～2 voxel，
  茎采样分辨率 45。无花头。UI 不参与截图。
- focus：`[0.85, 0.42906237, 0.85]`。下表高度是相对于 focus，
  focus 自身在当地地面上方 0.015 world unit。
- 俯视时 z 偏移设为 0.0001，避免相机方向与 up 完全平行。
- 相机默认近裁切距离 0.01：`src/gameplay/camera/desc.rs`。
- 测量 `graphics.flora`（草绘制）、`frame.render`（全帧 GPU）；
  不把 CPU frame／present 等待时间当作 GPU 时间，也不直接相加嵌套 GPU scope。

### 原路径 A/B：120 帧预热，296 个有效采样帧

单位：ms。丢弃预热后最初四帧，避免 timestamp 回读跨边界。

| 镜头 | 相机相对 focus 偏移 | 草 A p50 / p95 | 草 B p50 / p95 | 全帧 GPU A / B p50 |
|---|---|---:|---:|---:|
| 较高俯视 `top` | `[0, 0.12, 0.0001]` | 0.098 / 0.109 | 1.229 / 1.273 | 5.925 / 6.095 |
| 较低俯视 `low` | `[0, 0.04, 0.0001]` | 0.102 / 0.111 | **66.787 / 67.674** | 5.978 / **71.705** |
| 原近景斜视 `near` | `[0.025, 0.025, 0.07]` | 0.102 / 0.111 | 38.611 / 38.804 | 6.061 / 43.554 |

低位俯视的草 pass 约为 A 的 655 倍。不是只看总帧时间猜测，瓶颈明确出现在
草绘制 GPU scope；其光照缓存 scope 仅约 0.11～0.15 ms。

### 缩小场景与临时消融

为了缩短反馈周期，后续运行保留 120 帧预热，将结束帧由 420 改为 180，
取得 56 个有效采样帧。恢复原 shader 后又重测同一短窗口作为对照。

| 临时实验（B） | 较高俯视草 p50 | 较低俯视草 p50 | 原近景斜视草 p50 |
|---|---:|---:|---:|
| 原 shader，短窗口对照 | 1.159 | **65.840** | 38.177 |
| 仅取消代理全屏回退 | 1.117 | **1.029** | **1.939** |
| 保留回退，仅用顶端点替代真实求交 | 0.319 | **15.997** | **9.884** |

实验只改变一个对应路径；后两行会改变覆盖／遮挡，不能进行严格的成本相减，
不能把差值视为某条指令或单个组件的精确耗时。编译器可能同时消除部分
不再使用的计算，驱动的深度处理和其他 GPU 工作重叠也可能变化。

额外的场景缩小：只在镜头中心保留一块草丛（高草 373、矮草 290），使用
原 shader 和同一个低位俯视镜头，草 pass 为 **1.681 ms**。截图中的中心
可见草覆盖与九块草丛场景接近，但不保证逐像素相同。增加周围草丛后，
可见覆盖变化不大，成本却达到约 66 ms；结合回退消融与下面的最小投影
复现，说明不能把此次骤降仅归结为「可见草像素很多」。

## 代码定位：已确认的事实

### 1. 代理投影把侧面不可见的植株变成全屏工作

`shader/slang/stem_proxy_projection.slang` → `projectStemProxy()`：

```slang
anyInFront = anyInFront || clip.w > 0.0;
crossesNear = crossesNear || clip.z <= 0.0 || clip.w <= 1e-7;
...
if (!anyInFront) return p;
if (crossesNear) { ndcLo = float2(-1); ndcHi = float2(1); }
ndcLo = clamp(ndcLo, float2(-1), float2(1));
ndcHi = clamp(ndcHi, float2(-1), float2(1));
```

- `anyInFront` 只检查眼平面前方，没有证明盒子与相机可见体相交。
- 任一角点进入 near／eye 一侧，就先覆盖掉实际 XY 投影范围。
- 侧面完全不可见的盒子，本来可以在屏幕范围裁切后得到空范围；
  现在却先被覆盖为 `[-1,1] × [-1,1]`，失去了这个拒绝机会。
- 全部位于眼平面与近裁切面之间、但 `w>0` 的盒子，同样不能被这个
  `anyInFront` 检查排除。这是代码允许的另一放大条件，本次未单独量化。
- 茎 pixelization 还会将 local bounds 膨胀 `2 * height / resolution`；
  这可能扩大触发范围，但本次没有单独测量它的占比。

直接调用**生产投影函数**的最小 Slang CPU 复现：perspective near=0.01、far=10，
相机位于原点朝 -Z；盒子为 `[100,0,-0.015]..[100.01,0.01,-0.005]`。
它完全在右侧视锥之外，但跨 near。调用 proxy 顶点 0 和 2，实际得到
`[-1,-1,0,1]` 与 `[1,1,0,1]`：即一个全屏矩形。

该复现甚至关闭 pixelization，说明全屏回退自身就足够触发，不需要采样
网格膨胀。它证明函数行为，不是 GPU 性能证据。

诊断命令实际返回 **exit 9**（定义为「屏幕外盒子生成全屏代理」）：

```sh
slangc target/grass-stem-diagnosis/offscreen-proxy.slang \
  -std 2025 -I shader/slang -target executable \
  -o target/grass-stem-diagnosis/offscreen-proxy
target/grass-stem-diagnosis/offscreen-proxy
```

这些 target 文件是本地诊断附件，不属于提交代码或常规测试集。

### 2. 包围代理内的未命中像素也执行求交

- `shader/slang/grass_stem.frag.slang`：先生成 sample ray，再调用
  `traceGrassStem()`，之后才判断 miss、返回透明色和 `depth=1`。
- `shader/slang/grass_stem_geometry.slang`：最多八个中心点，逐段测试
  rounded-cone／端点球帽；高度不为一时最多七段。
- 一个真实小草茎不占满全屏，但它的全屏代理会把非常多无命中射线
  送进这个循环。多个代理重叠时，相同屏幕像素被重复处理。
- 帧计划的可见性筛选主要在 chunk 层级；这个路径没有在投影前用
  正确的逐株可见体相交结果拒绝这些草。相关入口：
  `src/tracer/flora_frame_plan.rs`、`src/tracer/mod.rs` 的草绘制阶段。

### 3. 显式片元深度不能假定拥有旧网格的提前深度拒绝效率

B 的 fragment 输出 `SV_Depth`，代理顶点的 raster depth 为 0。
检查实际优化后的 SPIR-V，看到 `DepthReplacing`，没有 `EarlyFragmentTests`。
因此不能假设当前地面或已画草的深度会在求交之前低成本挡掉所有无效片元。

这不是「完全没有 early-Z」的硬件结论：驱动可能进行其他保守优化；本次
没有 pipeline-statistics、fragment invocation／early-Z counters 或硬件抓帧。
也没有单独隔离深度替换的精确耗时。

### 4. 顶点／光照成本存在，但不是这个数量级骤降的充分解释

B 仍在每个代理顶点中循环生成各层 pose、读取颜色和光照缓存；六个顶点
并不等于每株只计算一次。它还保留了原来的光照 compute cache。
这些可能影响较高俯视、普通中远景的剩余开销，但大草丛／相机不变时，
只改变代理回退就消掉了绝大部分极近景开销，不能优先将此次几十毫秒
骤降归因于顶点阶段。

## 如何再次复现

现有已提交的可交互草场景可直接复现用户操作（启用 B、靠近并俯视）：

```sh
RE_FLORA_GRASS_STEM_TRYOUT=1 \
cargo run --release -- --authored-flora-bench --perf
```

定量复现本次 `top`／`low` 镜头，需要在临时 checkout 中给
`src/app/core/grass_stem_review.rs` 增加两个 distance 解析分支，并在 offset match
中增加上述两组偏移；其他逻辑不变。保留结束帧 420 得到长窗口，或仅将它
改为 180 得到短窗口。此次这些 fixture 改动没有保留到生产源码中。

```sh
cargo check
cargo build --release
env -u WAYLAND_DISPLAY RE_FLORA_GRASS_STEM_REVIEW=low-both-b \
  target/release/re-flora --hidden --mute --windowed --perf --authored-flora-bench
```

将末尾 `b` 换成 `a` 对比。草数量和 sample phase 的相机必须一致；过滤
sample→complete 中的 `[PERF][GPU_FRAME_SCOPE]`，排除最初四帧。
本地 `target/grass-stem-diagnosis/measure.mjs` 对低位俯视设置了 5 ms 的
**诊断报警线**；原 shader 实测触发 `RED`。这只用于识别此次骤降，
不是新增项目性能预算。

## 留给后续修复的验证要求（本次不实施）

1. 正确区分完全不可见、全部处于 near 后方、真正跨 near／eye 的代理；
   不用直接取消回退或丢掉跨平面草来掩盖问题。
2. 最小投影复现、低位俯视、斜视贴近、进入草丛、强弯曲都需要覆盖；
   同时检查漏画、近裁切面闪烁及 terrain／草的真实深度关系。
3. 分别量化全屏代理数量、代理覆盖像素／fragment invocations、命中率和
   near-plane 裁切后的绘制成本。不要只报告顶点数量下降。
4. 解决异常代理后，再独立评估求交、pose 复用、逐株剔除和 LOD 的剩余成本；
   不能用当前错误覆盖的消融数字承诺正确实现的性能。
5. 保留运行时 A/B，使用相同草数量、镜头、Release 和 GPU timestamps
   重跑原矩阵与新增俯视用例；性能验收与用户视觉确认仍是两件事。

## 附件、恢复与限制

本地附件目录：`target/grass-stem-diagnosis/`。

- `baseline.json`／`baseline-*-both-*.log`：原 shader 的长窗口 A/B。
- `baseline-short.json`：原 shader 的短窗口复测。
- `minimal.json`：单块中心草丛的缩小场景。
- `no-fullscreen.json`、`no-trace.json`：两种已撤回的诊断消融。
- 各实验子目录中的 PNG：固定 frame 120 的截图，低位俯视及缩小场景已查看。
- `offscreen-proxy.slang`：上述生产函数最小复现；`grass-stem-fragment.spvasm`：
  实际 optimized fragment 的 SPIR-V 反汇编。

实际用户的镜头 pose 未保存下来；这里是同类俯视症状的固定重现，不声称
逐帧复刻用户的操作。没有 cross-GPU 验证或硬件 overdraw 计数。
长窗口每配置一次，短窗口的恢复原 shader 再测提供一致性检查；
这是定位证据，不是精确分解各组件的完整性能研究。

诊断应用运行均正常退出，检查日志无 ERROR／panic／VUID，`failures=0`。
临时 shader 和 Rust fixture 改动均撤回；用户 GUI 配置／camera snapshots
不改动、不纳入提交。恢复后重新 `cargo check`、`cargo build --release`，
并做原路径隐藏静音 smoke，确保交付的 Release 不是任何诊断变体。
