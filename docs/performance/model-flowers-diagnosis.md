# 密集花草卡顿：共享表面缓存已被退役，花头回到实时生成

## 结论与状态

用户反馈同一种花大量种植后卡顿，转开镜头仍卡。当前 `vegi` 的渲染基线为
`d39dd0f9`。隐藏 Release 复现确认：**每实例、每完整花头、每帧重新采样三角形**是
主要额外 GPU 成本；只有区块级可见性筛选，使背后的花继续支付同样成本。

用户记得的共享预渲染缓存**确实实现过**，不是把缓冲区复用误认为结果缓存：

- `1abaa973`：实现共享可重打光的模型表面启动缓存。
- `f44c9340`：缓存结果／失效验证，保留蝴蝶连续根运动。
- `f7b7e036`：使用通用分页 GPU 存储，生产路径取消实时生成 fallback。
- **`6bae0d62`**：花草工作开始前的合并，提交说明明确移除 model surface cache
  和 generic GPU paging experiments。删除了 `src/tracer/model_pixel_cache.rs`、
  `shader/slang/model_pixel_cache.slang`、`model_pixel_bake*.slang` 及缓存文档，
  恢复实时 tile 生成。该提交同时是当前 `main` 的基线，不是仅花头漏绑一个现存缓存。
- `643bd6a7`：新花头接入当时的 `ModelPixelFrame`，因而继承了实时生成路径。

**本次仅提交诊断工具和证据，未恢复缓存、修改采样算法或降低画质。卡顿尚未修复。**
仅补可见性会改善背对场景，不能解决看着一大片花时的重复几何工作；降低像素数也不是根治。

## 实测

RTX 3060 Ti，真实 Release/Vulkan，X11，2880×1620 输出；隐藏、静音、普通自动选择
present mode。相同地形／设置，生产种植路径放置野老鹳草，每株两个完整花头，B 模式，
16 离散视向，成熟状态与固定太阳时刻。每组 8 秒，剔除最初 2.5 秒 GPU 样本；
不是随机单位测试、浏览器测试或微型采样器基准。

| 株数 | 视角 | 每花头像素 | 花头生成 GPU p50 | 整帧 GPU p50 | 实测帧间隔折算 FPS |
|---:|---|---:|---:|---:|---:|
| 0 | 面向种植区 | 32 | 0 ms | 9.526 ms | 58.3 |
| 32 | 面向种植区 | 32 | 4.681 ms | 14.055 ms | 49.8 |
| 128 | 面向种植区 | 32 | 17.886 ms | 27.533 ms | 29.6 |
| 0 | 原地背对 | 32 | 0 ms | 8.066 ms | 58.5 |
| 128 | 原地背对 | 32 | 19.340 ms | 27.591 ms | 31.1 |
| 128 | 原地背对 | 8 | 1.341 ms | 9.415 ms | 58.3 |
| 0 | 区块完整位于视锥后方 | 32 | 0 ms | 7.592 ms | 58.3 |
| 128 | 区块完整位于视锥后方 | 32 | 0 ms | 7.758 ms | 58.4 |

原地背对仍提交 **128 株、256 tiles、262,144 texels/帧**；区块完全离开视锥后提交为零。
128 株正面／背面的实际花草显示 pass 仅约 0.161／0.013 ms，而植被响应 pass 约
0.026 ms（空场景 0.024 ms）。因此本场景主要瓶颈不是茎叶绘制或风模拟。
32→128 株的几何生成成本约增为 3.8 倍；32→8px 后大幅下降，符合 N² 的 tile 工作量。

FPS 来自连续 GPU 日志时间戳和帧号差，不是 `1000 / GPU耗时`，也不采用只记录慢帧的
`PERF/FRAME`。这些是短时诊断结果，不是最大规模验收、启动烘焙成本或新缓存性能承诺。
独立的两组复现也得到背对 128 株额外 19.269 ms；不是单次截图采样误差。

[机器可读测量](../evidence/stylized-flowers/native-performance-diagnosis.json)。
完整原始日志位于本工作树 `target/flower-perf-diagnosis/`。

## 当前调用链与缓存区别

1. `src/tracer/mod.rs` 对扩张后的 **chunk AABB** 做视锥／距离筛选；通过后取整块的
   `species_len`。相机仍在该大区块内时，转身并不会让整个 AABB 出视锥。
2. `ModelPixelFrame::flowers` 计算 `count × parts`，每帧调度完整 `resolution² × tiles`
   compute。没有逐株可见性筛选，也没有表面缓存命中判断。
3. `flower_pixel.comp.slang` 调用 `sampleOrthographicModelPixel` →
   `sampleModelPixelGeometry`，重复三角形求交与保守覆盖；默认正交 bake 使用独立规范相机，
   不会自动因真实花头在相机后方而省掉工作。
4. `ModelPixelStorage` 复用的是帧槽内的**存储分配**；`ModelPixelBatch::publish` 每次
   都先执行 compute 再准备 draw。它不是跨实例／跨帧的几何结果缓存。
5. 当前粒子模型路径同样经 `ModelPixelFrame::particles` 每帧生成 tiles。
   `model_pixel_object.slang` 明确注明 “No atlas ... or persistent image cache”。
   普通预制精灵贴图与这条模型像素路径不能混为一谈。

历史缓存存储的是模型局部位置、规范深度、法线与材质 ID；最终实例颜色、照明、变换不烘死。
它仍有每实例 lookup + relighting 和显示成本，但不再逐帧遍历模型三角形、生成覆盖。
因此用户“先算好、每帧只取已有结果”的方向正确；当前缺的是该共享结果层，
**不是缓存因风或光照每帧失效，而是生产代码里已经没有它**。

恢复时应适配当前 `ModelPixelFrame`／资源代际所有者和最新共享采样算法，让完整花头及
整株 A/B 成为共享表面来源。不要整体撤销 `6bae0d62`、顺带恢复已退役树木实验，也不要
另建与现有所有者竞争的缓存／退休机制。可见性精化是另一项补充，不能代替恢复共享缓存。

## 复现与验证

```sh
cargo build --release
node scripts/diagnose-flower-performance.mjs --suite matrix
# 当前会按预期报 OFFSCREEN REGRESSION、返回 1：
node scripts/diagnose-flower-performance.mjs --suite offscreen --check-offscreen
```

第二条检查使用“128 株不可见花额外 tile GPU p50 > 1 ms”的宽松诊断报警线，
只用于抓住本次回归，**不是新增通用性能预算或发布门槛**。矩阵命令完成测量返回 0
也不表示卡顿已修复。`--help` 提供前提、输出路径、采样长度及错误恢复方法。

`RE_FLORA_FLOWER_BENCH=count,front|away|outside,pixels,heads|whole` 是显式诊断夹具，
上限 1024 株，不在普通游戏中启动；脚本只测试 0/32/128 株。真实实例通过生产种植接口
创建，参数只在内存中覆盖；没有新 GUI 控件，不读取／写入玩家存档。脚本检查每组
`config/gui.toml` 字节不变。不要同时启动另一游戏实例。性能计时不触发截图或 GPU oracle。

已验证：`cargo fmt --check`、`cargo check`、`cargo test`、普通隐藏静音 Release 启停及日志、
完整八组矩阵、独立失败复现和 CLI 帮助／错误恢复。主测试集 1,234 通过、4 忽略，另 4 项通过。
无 Vulkan validation／应用错误，无生成文件变化；普通启动不激活诊断夹具。
