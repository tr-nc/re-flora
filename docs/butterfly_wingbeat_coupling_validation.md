# 蝴蝶拍翼—飞行耦合 A/B

## 试用

在本 `butterfly-blend` 工作区运行游戏，按 **R** → **Butterflies** → **Flight**：

**Wingbeat-coupled flight (A/B experiment)**

- 未勾选：原飞行动力学与原翼动画/bob。
- 勾选：下拍支撑脉冲、上拍推进脉冲、惯性朝向与侧倾转弯；不重复叠加源 bob。
- 开关通过 `ButterflyFlightTuning.wingbeat_coupling` 接入统一 SavedControls，Debug Panel Save 可保存；旧配置缺字段默认为 false。不改用户当前配置。
- 适用于 Darting/guided 自然飞行；旧配置的 Original motion variant 与相机相对 mesh preview 不演示该耦合。
- 大约 0.25 秒渐变切换，保留位置、速度、个体相位 seed、颜色与生命周期；展示仍按已有 Butterfly Update FPS 统一冻结。

## 实现边界

依据 [调研](research/butterfly_wing_motion_coupling.md)，实现的是低阶风格化候选，不是 CFD、真实质量/重力/阻力模型，也没有模拟柔性合翼或逆偏航。

`src/particles/butterfly_wingbeat.rs` 读取批准源的同一组 `[wing, pitch, bob]` 曲线，并向 renderer 提供共享采样。右翼正角度在导出 Y-up 坐标中向上抬起，因此由实际曲线斜率区分上下拍，而不是另写一个独立正弦。

- 保留原本一秒动画循环；不把 25 Hz 源关键帧、120 Hz 物理步长、展示 FPS 或旧随机节奏频率混在一起。
- emitter 读取与原动画一致的时间轴和每只蝴蝶的稳定 phase offset，在物理子步末采样相位，随模拟姿态一起发布；renderer 不再单独推进已完全耦合的相位。
- 按各行程角位移归一化冲量，周期调制减去均值。通过累计曲线积分计算每个物理步的平均脉冲，避免 25 个源区间与 120 个物理步不整除导致的点采样偏差。
- 在现有平均巡航/栖息地恢复/避障控制上叠加有界拍翼调制；逐渐移除独立随机垂直机动。上拍期间有前向推进增强，但第一版没有独立拟合合翼末端的喷流脉冲。
- 导航 jerk 平滑与拍翼调制分开，最终仍经过加速度、速度、地形扫掠及世界边界限制。未勾选并完成过渡后保留原限制器语义。
- 平滑的空气相对 heading/pitch 和侧向机动驱动的 roll 决定姿态与支撑方向；不因风的地面漂移直接改变朝向。roll 限制为 ±0.45 rad。
- 关闭时恢复源 bob；完全开启时由世界轨迹负责平移起伏，源周期 pitch 保留。切换时用环形相位插值、姿态插值、bob 淡出避免突然替换。

当前幅度是保守的美术参数，主要随已有 speed/vertical strength 缩放；没有新增拍频滑杆、独立爬升用力状态机或左右翼差动。零均值测试只保证固定姿态/幅度下的周期冲量平衡，不代表所有机动下轨迹无偏；现有栖息地控制继续负责长期恢复。

## 验证

最终代码：

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：**1056 + 4 passed，2 ignored**。日志 `target/wingbeat-test.log`。
- 新回归覆盖实际曲线上下拍方向、周期积分平衡、左右转侧倾对称、A/B 渐变精确归零、零 dt 保持、个体身份保持、2/8/16/60 FPS 完整姿态冻结，以及 30/60/120/144 Hz 渲染分帧不改变物理结果。
- renderer 回归验证：完全耦合时，只改变外部动画时间/地面速度不会改变三角形字节；几何使用发布的姿态与相位，且没有再次加源 bob。
- 保存 round-trip 测试包含开启的新字段，通用的序列化叶子保存测试也通过。极端参数/实时改参测试包括新模式。
- shader、生成文件、源 mesh JSON、`config/gui.toml` 均无变化。

### Release 实机

使用 GPU 锁、hidden、mute，无可见游戏启动：

```bash
env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --auto-exit 0.5

mkdir -p target/wingbeat-flight
env -u WAYLAND_DISPLAY RE_FLORA_BUTTERFLY_REVIEW=wingbeat \
  flock --close /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute \
  --screenshot player-default target/wingbeat-flight/frame \
  --screenshot-delay 38 --screenshot-sequence 8 0.15 --auto-exit 41
```

`RE_FLORA_BUTTERFLY_REVIEW=wingbeat` 扩展已有诊断入口：先开启耦合，在自然主体出现后第 120/240 帧关闭/再次开启，同时记录发布的位置、速度、翼相位、blend 和姿态。通过正常保存字段切换，但诊断不主动保存配置。这不是代替 GUI 的唯一实验入口。

最终 smoke 日志：`target/re-flora-logs/re-flora-20260920-232705.307-1046557.log`。
最终自然飞行/截图日志：`target/re-flora-logs/re-flora-20260920-232810.644-1047000.log`。
两者正常退出、`failures=0`；最终截图运行无 ERROR、VUID 或 panic。

最终实机证据：

- 耦合开关在 frame 1/2019/2139 执行 true/false/true，已有自然出生个体参与切换。
- 47 个记录到的耦合姿态样本均有限，phase ∈ [0,1)，blend ∈ [0,1]，四元数平方范数最大误差约 `1.87e-7`。
- 成功生成 8 张 5120×2880 图，`target/wingbeat-flight/frame.000000.png` 至 `.000007.png`。受写盘串行限制，实际截图间隔约 0.19–0.22 秒，不声称严格 0.15 秒。
- 已检查第一张原图与同区域 8 帧裁剪拼图 `target/wingbeat-flight/contact.png`；可以看到翼形和位置变化，未见明显几何异常。蝴蝶在该取景中较小，这不等于近景或动态美感验收。
- 早期 15 秒运行没有自然蝴蝶出生，因此只算启动 smoke；之后 55 秒运行确认自然出生与往返切换。首次最终截图尝试因未创建输出目录失败，保留 `target/wingbeat-final-flight.log`，创建目录后重跑成功，不能把前次称为无错误截图验证。

未做前后性能对照，不以 debug 单元测试或截图采集期间 FPS 作为性能结论。尚待用户通过 GUI A/B 确认动态观感，再决定是否调整脉冲幅度/节奏或增加机动用力包络。
