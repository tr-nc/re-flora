# 从 main 选择集成 Stone / Pavement 与行走修复

## 范围

- 起点：`main` 的 `77fb7eb9b23198298bb373caf80346f5670d5316`。
- 来源：`stone-dyn` 的 `5c409ff92f99eafcb348f3078f3b8c841db44eee`；按功能移植，不整支合并。
- 分支：`integrate/stone-useful`；独立 worktree `stone-useful`。
- 计划保留：步高/窄脊防爬修复、真正的 Stone 地形放置、Pavement 及其有效端点交互。
- 不带入：六种 collider-lab 实验及其运行时测量框架、6/6 加宽胶囊、前移相机/近面扫掠、低墙原型、会话地形重置。
- 保留 main 的 4/8 胶囊、相机、撒土/背包、树干碰撞、踢树及音频、Debug 顶层字母排序；不覆盖用户保存的其它设置。

## 第一步：普通行走的步高与落脚面

移植 `d08deedf`、`783526e3` 的物理代码和独立回归。没有导入低墙原生 fixture 或后来的碰撞表示实验。
普通行走上限按完整身高的一半计算：当前仍是半径4、圆柱半高8，总高24、步高12体素。
复合体素接触由胶囊局部 witness 重建世界坐标；上升须有可用落脚面。
不支持的 KCC 上升从原姿态重新求解，不能仅删除已解算的 Y。原有跳跃参数、合法宽楼梯和坡面保持。
`camera/controller.rs` 此步只有跳跃回归测试增加，运行时代码未改。

验证（本集成 worktree，记录在 `target/integration/step1-*`）：

- `cargo fmt --check`、`cargo check`、`cargo build --release` 通过；生成文件和 GUI 配置无改动。
- 物理 crate **62 passed**；相机 **10 passed**；App 物理 **37 passed**。
- hidden/muted 0.5s Release 启动通过；日志首帧就绪、正常退出，无 WARN/ERROR/VUID/panic。
- 首次 walk-entry 启动未显式开启内部诊断日志，未出现 review 标记，未据此判通过。第二步加入 `RUST_LOG=info` 复验，同一生产恢复检查在 frame=1/31 均 `whole_capsule=true`，未修改原生检查代码。
- 上述结果不是性能接受证据，也不是与所有主分支玩法的完整手动验收。

## 第二步：Stone 放置与旧预览退役

来源为 `030efa03` 至 `731e1ca0` 的 Stone 功能改动，按 main 的新接口适配：

- 保留主分支 `B = Soil`，新 `N = Stone`，独立工具槽13；撒土与 Stone 点击/保持状态互不混用。
- Stone 经 main 的异步 Edit pick 队列接收已捕获答案。miss 不回退成新射线，树干命中会遮挡放置，法线拟合围绕该答案的实际地形位置。新增回归覆盖捕获答案、miss、树干遮挡与距离限制。
- 普通 Rock 材质、接触平面姿态、统一0.2缩放、成功后 seed 递增，真实 atlas 发布/存档/Dig 保留。移除旧常驻预览及专用渲染管线；几何参考与 OBJ 导出保留为测试工具，不让退役 preview helper 留在运行时。
- 冲突中保留 main 的异步输入、树木种植、访花资源、土袋世界/阴影管线和统一排序。Stone 的几何/放置区域仍参与 main 顶层字母排序；保存字段继续由声明生成。
- 配置只替换退役的 Stone preview 字段、增加 Stone 放置字段；其它用户值不改。`cargo check` 正常再生 `src/app/generated/gui_adjustables_gen.rs`，未手改生成文件。

验证（`target/integration/step2-*`）：

- fmt/check、`cargo clippy -- -D warnings`、Release build 通过；21项 Stone相关测试（另1项显式导出 ignored）、16项工具状态、102项 GUI 测试通过。
- GUI 字母排序检查先因旧两组标题期望失败，更新为新的单一 Stone 根组后通过；未删除排序/绘制断言。
- `scripts/validate-stone-placement.sh` 通过：无启动预览、两次 seed 递增、真实材质、跨进程 Terrain 重载及 Dig；运行前后 GUI 字节不变。
- main 的 Soil world 检查通过：实际加入6656体素，库存守恒；60非按住帧均隐藏，sun depth 76→0。
- main 的 raster-tree smoke 通过：波前、GPU踢树、有限 rustle、风叠加、静态树干碰撞，以及生长/移除/替换；无 ERROR/VUID/panic。保留9.325/24.566ms fruit hitch与重复移除tree的 WARN，不将这些运行当性能验收。
- 严格 `cargo clippy --all-targets -- -D warnings` 另发现7项既有测试 lint（在6个与 main 逐字相同的文件）；新增 GUI needless-borrow 已修正。未进行无关 lint 清理，不能称 all-targets 严格检查通过。
