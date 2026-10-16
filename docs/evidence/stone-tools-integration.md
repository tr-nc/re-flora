# 从 main 选择集成 Stone / Pavement 与行走修复

> 本文记录 2026-10-11 的集成范围。低墙后来按用户要求单独恢复，见
> [Pavement 低墙恢复](pavement-low-walls-restoration.md)；碰撞实验仍未带入。

## 范围

- 起点：`main` 的 `77fb7eb9b23198298bb373caf80346f5670d5316`。
- 来源：`stone-dyn` 的 `5c409ff92f99eafcb348f3078f3b8c841db44eee`；按功能移植，不整支合并。
- 分支：`integrate/stone-useful`；独立 worktree `stone-useful`。
- 已集成：步高/窄脊防爬修复、真正的 Stone 地形放置、Pavement 及其有效端点交互。
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

## 第三步：Pavement 与有效端点交互

按 `731e1ca0..a512c18b` 的功能差分移植并拆除低墙分支，不整批导入这段历史中的原型或会话重置：

- `V = Pave`，独立工具槽14；`N = Stone`、`B = Soil` 不变。Slabs / Brick / Dirt 共用距离采样路径；保留完整横向铺设、弯道接缝、地面姿态、保护已有地形以及真实 atlas 发布。Dirt 清理植被而不抬高地形。
- 保留“已通过的固定端点 + 可替换当前候选”的交互。拒绝点不进入导引历史；红色预览不发布断开的后半段，合法新目标可在同一手势继续，无需先退回旧端点。发布与候选预检共用同一保护逻辑；手势开始时冻结配方和地面，最新 atlas 仍约束每次发布。
- 使用明确的 `PlayerToolPointerInteraction` 区分拥有冻结地面、逐光标采样的铺路手势和 main 原有异步单次 pick。旧 `PlayerToolPointerAction` / 队列不接收铺路手势；Soil、Stone、Dig、树木/洒水器的捕获答案流程保持。增加工具路由/持续按住状态回归，保留 main 原有诊断入口。
- 相机/工具切换、阻挡面板、右/中键和 Terrain Load 会取消未建尾部，已发布体素保留。Pavement 提示与 Grow 不被状态背包遮住；Debug 顶层排序和 Soil 的预览半径保持。
- 删除低墙 recipe、状态、调节字段、说明与原生 fixture 依赖；没有 `low_walls` 模块、碰撞候选代码/开关、会话启动备份或 Reset 控件。`config/gui.toml` 此步只增加4个铺路保存字段；`cargo check` 再生 GUI 绑定，此步生成文件仅 `src/app/generated/gui_adjustables_gen.rs`。
- 只保存最终体素/植被，不承诺可编辑的持久路径、自动交叉路口、闭环或整条路径 Undo。

### 原生 fixture 的选择性适配

原来源的指针/恢复检查同时测试低墙和铺路。这里保留两种实际铺路材质，不把已删除的墙案例计入通过数：

- 普通指针检查保留 Slabs / Brick 原来的自然台阶地面坐标。
- 急转弯和恢复检查使用显式 opt-in 平坦土壤；0.12宽铺路需要更长可变尾部，因此直线前导段设为0.5，之后验证急转弯拒绝/合法目标恢复。不是普通游戏地形改平，也不是对旧低墙样本的性能比较。
- 冻结配方检查改为冻结铺路宽度。原“第一块墙居中”断言改为“第一整排铺路居中”，同时断言测试确有多条铺路车道；没有要求第一块偏置石板位于整排中心。
- atlas guard 检查在地面快照冻结后发布 Rock 障碍，验证最新的禁止叠铺保护拒绝整条候选、atlas 字节不变，并能换清楚的目标继续；不再声称验证低墙全高保护。

### 最终验证

日志在本 worktree 的 `target/integration/step3-*`；较大的原生数据在 `target/stone-placement`、`target/pavement`、`target/pavement-pointer`、`target/pavement-continuity`、`target/pavement-recovery`。

- fmt/check、生产目标 `cargo clippy --bin re-flora -- -D warnings`、`cargo build --release` 通过。
- 全套 `cargo test -- --test-threads=2`：**1574 passed / 7 ignored**；collision tool **4 passed**；物理 crate **62 passed**。随后去掉测试内冗余 identity conversion、收窄 GUI 说明后，铺路 **39**、工具 **18**、GUI **104** 项定向复验通过。
- 首次默认并行的全套测试有1项失败：未修改的 `sparse_worker_not_ready_waits_without_busy_duplicates_then_publishes_once` 在1000次 yield 内未收到 worker 的 not-ready 结果。原日志 `step3-tests.log` 保留；单独重跑及上述2线程整套复验通过。未改这个模块，也不宣称已经修复其测试稳定性。
- 最终严格 all-targets Clippy 仍只有第二步记录的7项既有测试 lint，6个所属文件与 main 无差分；新铺路的 lint 已修正。此项仍不是全绿。
- 最终 Release 重跑 `validate-stone-placement.sh` 及4个 `validate-pavement*.sh` 全部通过：真实材质、左右弯道完整排、重复铺设 atlas 不叠层、Dirt、跨进程重载、真实 Winit 路由、受阻红色预览、重复按下不重启、直接合法目标续铺、已建前缀字节不变、松开/取消静音。所有这些原生日志无 WARN/ERROR/VUID/panic；检查了弯道和受阻预览截图。
- 同一最终二进制复验 main 的 Soil world：`added=6656`、库存守恒、60个未按住帧均无土袋；walking entry 在 frame=1/31 均 `whole_capsule=true`；raster-tree smoke 包含异步 surface pick、GPU 踢树、延迟波前、rustle、风叠加及生长/替换回归，均通过。
- 树木 smoke 仍有同类果实物理 hitch（11.082 / 26.004ms）及重复移除 tree WARN；没有 ERROR/VUID/panic。不能据此宣称性能无回退。
- 最终 `cargo run --release -- --hidden --mute --auto-exit 0.5` 首帧就绪、正常退出；用同 worktree 的 `--latest-log` / `--tail-latest-log` 检查，无 WARN/ERROR/VUID/panic。日志：`re-flora-20261011-214204.300-122306.log`。
- GPU 原生运行统一使用共享锁 `/tmp/re-flora-summer-gpu.lock` 串行。最终全部原生运行前后 `config/gui.toml` SHA-256 一致；未自动打开可见游戏。这些是功能检查，不是性能或完整主观手动验收。

## 交付状态

三批改动独立提交；前两批为 `8ad5cb8b`、`1906744a`，本文件随铺路批次提交。主 checkout 的 `main` 仍为 `77fb7eb9`，来源 `stone-dyn` 仍为 `5c409ff9`，均未修改、合并或推送；Release 二进制在集成 worktree 的 `target/release/re-flora`。后续试玩必须从 `stone-useful` 启动，不能从留存的 wall-policy 实验 worktree 启动。
