# 蝴蝶配色与动画可见尺寸迭代

2026-09-13，`agent/butterfly-block-flight`。

## 动画尺寸补偿

用户反馈从方块切换到动画后明显变小。实际快照路径给两者相同边长；GPU 上传直接使用该尺寸，顶点 shader 对两者使用相同的 1.25 缩放。实际使用的图集第 1、3 行（零起始）共十帧，平均每帧只有 58/256 = 22.65625% 像素不透明。这解释了标称尺寸相同但视觉面积大幅减少的原因，不是切换重置飞行状态。

- 在 guided-flight 动画的输出快照中使用固定 2.1 倍边长，即约 `sqrt(256/58)`；平均不透明面积约为原方块的 99.9%。这是面积近似，不代表每一帧轮廓或主观视觉重量完全相等。
- 不按动画帧动态缩放，不消除翅膀开合。方块尺寸、模拟尺寸、运动、透明度、颜色索引、原始 legacy 飞行模式和共享 shader 均不改。
- 回归测试使用实际图集和真实外观切换快照，同时检查运动、节奏、颜色、动画推进不变；旧代码明确失败，输出 `sprite/block mean visible area ratio 0.2265625`。补偿后通过。

验证：`cargo fmt --check`、`cargo check`、`cargo test` 通过（961 主测试通过、2 ignored，另 4 个库测试通过）。持锁 `cargo run --release -- --hidden --mute --auto-exit 0.5` 正常退出，日志 `target/re-flora-logs/re-flora-20260913-175912.904-26209.log`。此次短运行使用用户保存的方块模式，只是运行正确性证据；动画补偿的 GPU 观感尚需后续试玩／捕获，不能由面积测试替代主观验收。

无生成文件变化。`config/gui.toml` 和 `config/camera_snapshots.toml` 的用户改动不提交，隐藏运行前后 SHA-256 一致。

## 自然初始配色候选

依据 `docs/research/butterfly_initial_colors.md` 的自然史边界作美术调整：奶白 35%、淡黄 25%、橙褐 20%、土褐 15%、蓝 5%。紫／红预设及纹理编号保留，但不进入初始抽样。上述百分比是设计权重，不是野外丰度；没有新增物种图案或声称精确复刻物种。

调色同时调整现有四个明暗角色，保留透明像素和原动画资产。方块继续读取同一预设的中间色，动画继续读取同一预设的四角色 LUT；没有新生成器、数量或栖息地改动。改变随机抽样范围后，不承诺跨代码版本的同 seed 随机序列逐值不变；运行中外观切换的个体和随机状态仍由回归测试保护。

验证：fmt/check 通过；完整测试 963 主测试通过、2 ignored，另 4 库测试通过。新增穷举权重／稳定编号、透明与明暗角色顺序测试。持锁 release hidden muted 连续帧捕获：

```sh
RE_FLORA_BUTTERFLY_REVIEW=switch flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --windowed --denoiser-bench player-default target/butterfly-review/natural-palette-size-long.toml --denoiser-bench-warmup-frames 1000 --denoiser-bench-frames 1200
```

日志 `target/re-flora-logs/re-flora-20260913-180220.190-28869.log`：自然主体 frame 1312，脚本设置方块 frame 1432、动画 frame 1552；正常退出，无 ERROR，保留既有多图集选择 WARN。此前较短捕获未覆盖切换，因此不算切换证据。

已检查 `target/butterfly-review/natural-palette-size-long.artifacts-xqjBSo/frame-0450.png`、`0570`、`0573`。全景中主体过小且场景/UI 干扰明显，不能凭这些图确认配色优劣或主观尺寸匹配。实机运行和切换日志通过，最终视觉效果仍待人工试玩；捕获运行不是性能验收。用户保存的方块模式、6Hz、速度 0.4、上下强度 3.2 等设置原样保留，未将这些无关改动提交。

面板说明同步改为共享飞行／风／节奏，另明确动画有透明留白补偿、方块不变。最终文案版本再次通过 fmt/check 和持锁隐藏静音短运行，日志 `target/re-flora-logs/re-flora-20260913-180424.300-29590.log` 正常退出、无 ERROR、shutdown failures=0。最终用户配置 SHA-256 与运行前相同，无生成文件变更。
