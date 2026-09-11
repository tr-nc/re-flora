# 轴对齐树叶的局部抖动

Debug → Flora → Leaves → `Local Flutter (0 = original)`，默认 1，范围 0–2。设为 0 可对比原效果。保持原有整体风响应。

每片叶子增加一个有惯性、有阻尼的内部铰链角。局部风驱动力与风速平方相关，叠加按叶子种子区分的连续小尺度激励；无风时激励归零，运动衰减。同一个角度同时驱动中心的小幅平移和内部光学法线，并沿用已有的离散姿态发布节奏。

**几何顶点偏移不旋转，立方体始终轴对齐。每片叶子在每帧只计算一个 RGB，各个顶点及各个面共用它。** 内部法线只决定观察到的正反面混合、太阳入射角和透射贡献，不用于几何变换、阴影射线偏置或 DDGI 缓存方向。

这是物理启发的薄叶代理模型：角度与颜色有共同状态，避免独立的颜色闪烁器。未解析的湍动采用连续双频激励，风速单位、正反面色差和透射比例仍是美术标定，不代表完整气动或叶片光谱模型。环境光继续使用现有稳定的树冠缓存方向。研究依据见 [叶片朝向研究](research/leaf_orientation_flutter.md)。

## 验证

- `cargo fmt --check`、`cargo check`；Rust 测试 925 通过、2 忽略。
- `python3 scripts/run_slang_tests.py`：9 项通过，直接执行生产着色器函数，检查轻风持续响应、个体差异、无风衰减、法线及位移边界、光学变化与全黑输入。
- `RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1` 隐藏静音 release 实机：叶片峰值角约 0.355 弧度，个体角差约 0.625 弧度，静风末态约 4e-8 弧度；保持姿态边界及原有响应验证通过。
- 同一 blacky 近景分别捕获关闭和默认模式，各 120 个连续帧，每 3 帧保存 RGB。检查实际截图中的局部位移与颜色变化；本地回放为 `target/leaf-flutter/review.html`。风场 B 在这个场景启动约 20 秒后才有明显来流，捕获前预热 3600 帧。

复现截图：

```sh
RE_FLORA_LEAF_REVIEW=1 RE_FLORA_WIND_AB_SMOKE=1 flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --windowed --denoiser-bench blacky target/leaf-flutter/settled-new.toml --denoiser-bench-warmup-frames 3600 --denoiser-bench-frames 120
```

`RE_FLORA_LEAF_REVIEW` 只覆盖本次运行的强度，并启用密集关键帧，不写入 GUI 默认值。两次捕获未锁定模拟相位；回放用平均采样间隔，不能当作性能、精确运动时间或主观自然度的验收证明。

状态从 80 增至 112 字节；当前 32768 容量的两个输出缓冲合计增加 2 MiB。尚未完成叶片性能专项基准。视觉候选与性能验收分开。
