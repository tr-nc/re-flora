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
- 另一次携带既有 walk-entry review 环境变量的4秒启动正常退出，但未出现预期 review 标记，**不计为原生恢复回归通过**。
- 上述结果不是性能接受证据，也不是与所有主分支玩法的完整手动验收。
