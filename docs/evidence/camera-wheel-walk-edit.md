# 滚轮连接步行与 OrbitEdit

## 操作

- **OrbitEdit 编辑模式**继续用原来的旋转／平移和自由鼠标笔刷。向上滚轮拉近，达到最近距离 .2 后，在 orbit 焦点 XZ 下查询地形，并用生产玩家 capsule 扫落地；安全落点存在才进入步行。
- **步行模式**向下滚轮，围绕当前位置的 eye anchor 拉起编辑镜头。只保留 yaw，不沿当前视线后退：无论原来仰视／平视／俯视，终点均为向下 **45°**、距离 **.6**。鼠标释放出来，完成过渡后可继续笔刷编辑、旋转或缩放。
- 两方向都有 **.45 秒、时间驱动 smoothstep** 的位置和 pitch 过渡；yaw 使用最短角度路径。落地后朝向水平，随后恢复原有步行碰撞／重力／脚步。
- **Shift+滚轮调笔刷半径**保持优先；GUI 接管鼠标或阻挡面板时不触发相机缩放。过渡期间不接受新缩放／orbit 拖动，也不执行笔刷，防止连滚抖动或误涂；完成后继续操作。
- 没有地形／capsule 无安全 grounded 结果则留在编辑模式；不会因射线没命中而直接掉进空世界。

不再需要 G 完成编辑↔步行往返。此次保留旧 G 三模式切换和 WASD FreeFly 作为后备；移除飞行模式留待用户确认本轮交互。Debug 临时借用 FreeFly 的 orbit 不自动落地，关闭 Debug 仍能恢复原飞行模式。

## 实现边界

`camera_control/zoom.rs` 负责纯镜头目标与插值；CameraControlRuntime 负责过渡／模式权威；App input 负责滚轮归属、实际地形和 capsule 查询、停止旧笔刷 hold／运动惯性、同步 cursor grab。参数是固定交互常量，没有添加不可保存的 Debug slider。过渡是临时运行状态，不保存进 GUI 或 camera snapshots。

落点由 capsule 碰撞决定，可能高于中心地形射线命中的高度（角色宽度覆盖邻近台阶），而非强制使用一个点的高度。插值阶段是编辑镜头运动，不是沿全段执行物理角色行走；编辑镜头的原有穿墙／穿地限制没有在本轮重做。此轮不更改 FOV，也不自动找鼠标下的另一个目标点。

## 参考

- [Planet Coaster 官方建造说明](https://www.planetcoaster.com/en-US/planet-coaster-1/news/console/crea-il-tuo-parco-tema) 提到用第一人称视角从游客角度检查建造结果，参考的是“编辑与亲身体验互补”，不是声称它有相同滚轮绑定。
- [Google Earth 社区的 ground-level zoom 导航说明](https://support.google.com/earth/thread/84855703/can-you-temporarily-disable-street-level-view?hl=en) 提到自动倾斜／进入地面层级。它不是游戏；本项目借鉴距离跨层级的导航概念，45° 和 yaw-only 出场规则来自本次用户要求。

## 验证

- `cargo fmt --check`、`cargo check`、Rust **1269 passed / 4 ignored**。
- 纯测试覆盖仰视／水平／俯视（±89°）、yaw／FOV 保留、45° 终点、短 yaw 路径、不同帧步长、非法 dt、完成前不进入 Walk、GUI scroll 不落地、Debug 临时 orbit 不丢失 FreeFly 返回状态。
- 真实 Release/Vulkan：`env -u WAYLAND_DISPLAY RE_FLORA_CAMERA_ZOOM_REVIEW=1 cargo run --release -- --hidden --mute --auto-exit 30`。诊断等待初始化，模拟 80° 仰视步行 → 负滚轮 → 45° OrbitEdit → 正滚轮 → 生产 capsule 落地 → Walk；断言 yaw=36°、cursor 释放／抓取和实际模式，输出 `[CAMERA_ZOOM_REVIEW] passed`。
- `target/camera-zoom-native.log` 中两次过渡均完成，capsule 落地成功，无 ERROR/panic/VUID，shutdown failures=0。该 opt-in 诊断不保存参数，正常启动不执行它。
- 常规 hidden muted Release smoke 与同工作树 latest-log 检查通过。没有自动启动可见游戏；镜头舒适度、用户编辑体验和狭窄地形的视觉表现仍需手动反馈。

产物：`target/camera-zoom-{fmt,check,unit,tests,native,smoke,run-tail}.log`。用户现有 `config/gui.toml` 私人值不修改／提交。
