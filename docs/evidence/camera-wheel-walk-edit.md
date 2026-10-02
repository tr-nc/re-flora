# 滚轮连接步行与 OrbitEdit：直接后退版

用户体验 df8405ba 后要求移除试探／浮起／恢复，先迭代整体镜头的自然运动。本页取代上一轮预览版使用说明；先前研究中的 preview 建议不是当前行为。

## 体验

- 步行中向外滚轮直接开始连续后退→编辑镜头过渡，没有垂直浮起预览、意图积累、等待或回弹阶段。
- 水平或仰视起步时，初始位置切线是沿 yaw 的平面后退，不再强加至少 .25 的向上方向；随后沿曲线自然接入保留 yaw 的向下 25° 编辑终点（后续按用户反馈由 45°→35°→25° 调低，半径 .6 不变，位置与朝向同步调整）。
- 确认后的 Hermite 曲线和 quaternion 朝向协调保留：短初始对齐区后，后轴匹配运动路径切线，无 roll。不退回到直线位置／独立 pitch lerp。
- 编辑模式滚轮拉近到 .2，真实 capsule 安全落地检查通过后回步行。空地形／不安全落点不切换。
- Shift+滚轮笔刷半径、GUI 归属、过渡期间不误涂、cursor 释放／抓取保持原逻辑。G／FreeFly 后备暂留。

## 删除与限制

整条 ZoomPreview 路径及其临时状态、弹簧、进度／确认阈值、恢复代码和对应测试删除，不只是隐藏预览。升空 .7 秒、落地 .45 秒、最终编辑距离 .6 不变；无新增 Debug 设置或保存字段。

### 后续手感迭代：整段姿态缓入缓出

用户反馈起止仍突兀。原位置虽用 cubic smoothstep，初段转向却集中在曲线前 .25，导致整段姿态的速度峰值偏早。新增按“位移距离＋旋转角度×.15 世界单位／弧度”计算的 128 段姿态长度表（只在过渡创建时建立）；quintic smootherstep 再驱动该长度进度，起止速度及加速度为零。原后退曲线、quaternion 朝向／切线约束和最终姿态不改，也不靠延长动画掩盖速度分布。

确定性采样记录：旧 -89° 起步的整段姿态速度峰值在 27% 时间，新 -89°／0°／80° 分别在 50%／51%／50%；起止采样步长均低于峰值的 3%。这是镜头运动学指标，不是 app 性能 benchmark，也不等于舒适度已经获得用户批准。

后退最终仍须升高才能进入俯视编辑；本次删除的是独立的“先向上漂一下再恢复”，不是取消编辑视角高度。任意仰视时严格沿后轴会指向地下，所以起步短对齐区仍允许安全的平面退镜头。狭窄场景全轨迹碰撞、舒适度仍待用户反馈，不宣称已经最终通过视觉验收。

## 验证

- `cargo fmt --check`、`cargo check`，Rust **1271 passed / 4 ignored**。
- 新纯测试覆盖水平／45°／89° 仰视的初始切线 y=0、平面后退方向；保留 ±89° 的单调升高、后轴／切线 dot > .9999、无 roll、yaw/FOV 和帧率／非法 dt 测试。
- `env -u WAYLAND_DISPLAY RE_FLORA_CAMERA_ZOOM_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 25`：实际 80° 仰视单格直接开始，无 preview；编辑终点（原 45°，现 25°）、yaw 保留、鼠标释放、正滚轮生产 capsule 落地、Walk 抓取鼠标。日志 `[CAMERA_ZOOM_REVIEW] passed preview=false wheel_roundtrip=true ...`。
- 普通 hidden muted Release smoke 与同工作树 latest-log tail；无 ERROR/panic/VUID，shutdown failures=0。日志 `target/camera-withdraw-{fmt,check,unit,tests,native,smoke,run-tail}.log`。
- smoothing 迭代：`cargo fmt --check`、`cargo check`，Rust **1272 passed / 4 ignored**；新增整段速度峰值／起止速度回归测试，先在旧算法失败（peak=27），修改后通过。Release 25 秒实际往返和 smoke／latest-log 检查通过，无错误，shutdown failures=0。日志 `target/camera-smoothing-{before,after,check,unit,tests,native,smoke,run-tail}.log`。
- 35° 调整：23 项 camera_control 测试及 Release 实际往返／smoke／latest-log 检查通过，日志确认 `edit_pitch=-35`；半径和 smoothing 参数不变。产物 `target/camera-35-{check,unit,native,smoke,run-tail}.log`。
- 25° 调整：23 项 camera_control 测试及 Release 实际往返／smoke／latest-log 检查通过，日志确认 `edit_pitch=-25`；半径和 smoothing 参数不变。产物 `target/camera-25-{check,unit,native,smoke,run-tail}.log`。
- 验证／提交后启动同工作树可见 Release，无 `--perf`，让用户继续反馈。私人 `config/gui.toml` 不提交；无 shader／生成字段变更。
