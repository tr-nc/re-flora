# God rays 默认全主渲染分辨率

## 决定与改动

用户明确要求先采用直接、可靠的全分辨率方案，再比较性能；本次不引入深度感知上采样、边缘补算或新的 GUI 质量开关。

`src/tracer/extent_dependent_resources.rs` 将 god-ray raw/history/output 三张 R32_SFLOAT 纹理从主渲染宽高各 1/2 改为与主渲染深度纹理相同的尺寸。源 shader 和 dispatch 已按纹理实际尺寸工作，无须修改采样步数、shader ABI 或生成文件。保留原时域算法；lens flare 仍为原来的半分辨率。

这里“全分辨率”是**主场景内部渲染分辨率**，不是改变整个游戏的屏幕缩放：本机输出 5120×2880，场景内部 2560×1440；god rays 从 1280×720 提升到 2560×1440。新增资源日志确认 `scene=2560x1440 effect=2560x1440 textures=3 format=R32_SFLOAT`。

三张纹理的理论像素存储从 10.55 MiB 增至 42.19 MiB，增加约 31.64 MiB（不含驱动分配粒度）。

## 画质核对

使用用户未提交的 `glitch` 相机：position `[0.9965005, 0.6620432, 0.9027385]`，yaw `55.30538`，pitch `4.7783012`，FOV `60`。没有修改或提交 `config/camera_snapshots.toml`。

同一视角/设置，分别用旧、新 release 二进制在 3 秒后抓图：

- `target/god-ray-full/half.png`
- `target/god-ray-full/full.png`
- `target/god-ray-full/edge-comparison.png`：相同区域裁剪，最近邻放大 3 倍，左旧右新。

已检查对比图：原先树枝/天空边界附近的宽晕边在新图中明显消除，轮廓贴合改善；这不是对所有动态时域拖影的修复声明。

## Release 性能比较

RTX 3060 Ti；hidden + mute；GPU 锁串行运行；相同 `glitch` 相机、配置、主场景，保持自动选择的 FIFO present mode。不用截图读回期间的帧率作性能证据。

旧版本是 `bffaae22`，先构建并保存旧二进制，再仅修改 god-ray 资源尺寸并构建新二进制。两者源 shader 相同。

- 顺序：**A/B/B/A**，每轮渲染 20 秒。
- 忽略 frame < 300；GPU scope 日志每约 30 帧记录，最终旧版 58、新版 57 个样本。
- 初次探索沿用 min_samples=120 导致样本不足，未作为有效报告；根据实际日志频率将此次短测配置改为 min_samples=20 后完整重跑四轮。
- God-ray 设置：32 checks，max depth=1，weight=0.5，temporal blend=true，alpha=0.2。未改 GUI 文件。
- `target/god-ray-full/abba/comparison.json` 保存合并原始样本统计；四份独立 JSON/log 在同目录。没有剔除偶发高耗时样本。

| 指标 | 半分辨率中位数 | 全分辨率中位数 | 差异 |
| --- | ---: | ---: | ---: |
| God-ray 步进 GPU | 0.178 ms | 0.648 ms | +0.470 ms / +264.0% |
| God-ray 时域 GPU | 0.0775 ms | 0.205 ms | +0.1275 ms / +164.5% |
| 合成 GPU | 0.217 ms | 0.231 ms | +0.014 ms / +6.5% |
| **GPU 整帧** | **7.3845 ms** | **8.043 ms** | **+0.6585 ms / +8.9%** |
| CPU frame scope（含等待等影响） | 17.7535 ms | 17.337 ms | 不解读为 CPU 优化或 FPS 提升 |

GPU 整帧 p95：7.619 → 8.2886 ms，+0.6696 ms / +8.8%。两次旧版整帧中位数 7.448 / 7.379 ms，新版 8.075 / 8.033 ms，方向一致。

结论：此视角中，用约 **0.66 ms GPU 整帧成本**换取更干净的轮廓。不能把 +8.9% GPU 耗时直接当成实际 FPS 下降 8.9%，也不代表其他 GPU/分辨率/场景的成本。

临时 benchmark 配置沿用了 suite 示例的 2%/3% reporting budgets，并使用 `--allow-regression` 输出比较。多个 GPU 指标明确显示 REGRESSION；本次是用户要求的画质/成本比较，不宣称通过性能不回退门槛，也不因此撤销用户指定的全分辨率默认。

### 复现命令与证据

临时 scenario 文件：`target/god-ray-full/perf.toml`。核心 args 为 `--hidden --mute --perf --camera-snapshot glitch --auto-exit 20`，warmup_frame=300，跟踪上表五项指标。

```bash
env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock \
  python3 scripts/perf_suite.py --config target/god-ray-full/perf.toml \
  run-ab god-ray-glitch \
  --baseline-binary target/god-ray-full/re-flora-half \
  --candidate-binary target/god-ray-full/re-flora-full \
  --order A,B,B,A --output-dir target/god-ray-full/abba --allow-regression
```

SHA256：

- 旧二进制：`5a314c72e4ac19866f1886d99cddd435ae7a806b3b167eb66c89bdd2723a5f7e`
- 新二进制：`d927a77310b5cb7e69dffe57823d1c825ee1eb191b1cbd4c05fc335445cc26d9`
- GUI 配置：`0d6168a831b71b67a620c6135f3fad05686e6d51acee116e2ae9888b4c5ec1f4`
- 相机配置：`0c61e329027a8a401625ce668896b7c9453376add275550f0bb053c2c0e1536c`

## 正确性验证

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：1057 + 4 passed，2 ignored；新增尺寸测试覆盖普通、奇数和退化输入。
- `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过；日志 `target/re-flora-logs/re-flora-20260921-011628.773-1066711.log`。
- 全分辨率截图运行日志 `target/re-flora-logs/re-flora-20260921-011638.428-1066839.log`；已用同工作区 `--latest-log` / `--tail-latest-log` 检查。
- 四轮性能运行、最终 smoke 与截图运行均无 ERROR、VUID 或 panic，正常退出且 failures=0。
- 用户 GUI 与 snapshot 内容均保留；没有生成文件变化，没有启动可见游戏。
