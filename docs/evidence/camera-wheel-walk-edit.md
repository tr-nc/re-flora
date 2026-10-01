# 滚轮连接步行与 OrbitEdit：可恢复试探版

本页更新 44d7d4a0 的一格自动切换实现。设计讨论／第一手参考见 [`../research/camera-zoom-intent-and-trajectory.md`](../research/camera-zoom-intent-and-trajectory.md)。

## 体验方法

- 编辑模式滚轮拉近到最近距离 .2，检查实际地形及玩家 capsule 安全落点后，平滑进入步行；落地后朝向水平。
- 步行中轻微向外滚动，镜头只小幅浮起。身份仍为 Walk、鼠标保持抓取、笔刷不执行；停止滚动后恢复原位，不切模式。
- 继续向外滚动，逐步确认意图；达到阈值才进入 OrbitEdit、释放鼠标，再沿协调曲线升空到保留 yaw、向下 45° 的编辑终点。
- 试探中反向滚动减少进度。鼠标观察仍有效，恢复保留用户最新的 yaw／pitch，不突然扭回旧视角。
- 试探期间角色位移暂时暂停，胶囊锚点不跟随渲染镜头浮起；持有的步行按键不被清掉，恢复后无需松开／重按 W。确认升空时才清除步行输入。
- Shift+滚轮半径和 GUI 指针归属保持原逻辑。已确认的短暂升空／落地接管期间不执行笔刷或接受新的镜头拖动。

## 当前候选参数（不是最终体验定值）

- 每 line 增加 .22 目标进度；单个大／合并事件最多增加 .35，不能一次确认；连续普通滚动约五格达到目标阈值。
- .65 秒没有输入后目标回到 0；用频率 18/s 的解析临界阻尼弹簧跟随目标，不播放不可撤回的固定预览动画。
- 实际弹簧进度达到 .8 且目标达到 1 才确认。连续慢滚也能进入，不按瞬时速度判断。
- 最大试探浮高 .025 世界单位；一格约 .0055。落地 .45 秒，确认升空 .7 秒，最终编辑半径 .6。
- 单事件限制不是可靠的触控板惯性识别器；长尾惯性仍需真实设备手感反馈，不宣称已经解决所有误触。

## 位置／朝向耦合

`camera_control/zoom.rs` 的升空使用 Hermite 安全曲线。起点后轴若指向地下，初段允许安全上升方向与后轴不完全一致；四元数平滑对齐，无 roll。曲线进度 .25 后后轴直接匹配轨迹切线，终点切线对应保留 yaw 的 -45° pitch。不是直线 lerp 位置并独立转 pitch，也不是声称 quaternion slerp 自动保证位置约束。

预览只作少量 world-up 平移，不扭头；恢复沿相同高度通道回去。严格后轴约束只适用于确认升空的对齐之后，不适用于恢复向前移动或仰视起步。

地形落点由 capsule 而非单点射线决定，宽度可能覆盖更高的邻近台阶。没有安全 grounded 结果则不落地。编辑镜头全段碰撞／狭窄房间轨迹仍不是本轮重做范围，需实际体验；不改变 FOV。

## 权威与保存

ZoomPreview／ZoomTransition 是 CameraControlRuntime 的临时状态，不保存进 GUI／snapshots。模式在 Preview 中仍为 Walk，只在确认时进入 OrbitEdit；cursor、旧笔刷 hold、输入接管在 App input 统一处理。没有 App-only Debug slider 或额外保存分支。

旧 G 三模式切换与 FreeFly 后备保留；编辑／步行滚轮往返不需要 G。Debug 临时借用 FreeFly 的 orbit 不自动落地。

## 验证

- `cargo fmt --check`、`cargo check`，Rust **1274 passed / 4 ignored**。
- 纯测试：单格／单个巨幅事件不确认而恢复、慢滚／快滚确认、反向撤回、恢复跟随最新 mouse look、不同 dt、跨 ±180° yaw、±89° 仰俯视、单调升高、对齐段后轴与路径切线 dot > .9999、无 roll、阈值前 Walk 权威、非法输入及 Debug／GUI 隔离。
- `env -u WAYLAND_DISPLAY RE_FLORA_CAMERA_ZOOM_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 40`：生产输入／Vulkan／物理路径实际驱动 80° 仰视单格试探恢复（cursor locked）、连续滚动确认（cursor released）、45° 编辑终点、正滚轮 capsule 落地、Walk；yaw 保留。输出 `single_notch=recovered` 和 `passed preview_recovery=true deliberate_scroll=true ...`。
- 常规 hidden muted Release smoke、同工作树 `--tail-latest-log 120`；无 ERROR/panic/VUID，shutdown failures=0。
- 日志：`target/camera-preview-{fmt,check,unit,tests,native,smoke,run-tail}.log`。用户私人 GUI 数值不提交；shader／生成字段无变化。

视觉手感待用户体验，不把测试通过等同于舒适度或性能验收。验证后从同工作树启动可见 Release，无 `--perf`。
