# Pavement 低墙恢复（2026-10-17）

## 影响范围

- 恢复前的 main 只有 Slabs、Brick、Dirt 铺路，旧低墙入口和实现均不在主线，
  不是控件折叠或最近移除 Walking Traversal A/B 造成。
- 恢复后：**R → Pavement → Pavement: build low walls**，再 **V + 左键拖动**。
  Earth 为连续微收口土墙；Stone、Brick 为实际错缝的分层砌体。
  Tab 换材质，Shift + 滚轮调厚度，Ctrl + 滚轮调高度。GUI Save 保存偏好。
- 铺路原有回拖、内弯平滑仍仅用于石板/砖路；不把它们扩展成墙的回拖或整体 Undo。
  Rooftop PoC 仍不可使用本工具。

## 根因

此前选择性集成仅带入铺路，明确排除了低墙的 recipe、生成器、状态、保存字段及验证入口。
补一个 checkbox 不会恢复实际绘墙；必须恢复完整的建造 consumer，同时适配主线现有
candidate 预检、发布与精确体素所有权。

本次初轮原生验证另发现适配错误：直接用铺路的正间隙 coverage 判断墙体，
相邻墙柱共享端面也会被判 Overlap。症状为第二柱被拒绝，尚未到达真正的上方障碍。
现由 ConstructionRecipe 选择 coverage 规则：铺路保持原有砖缝 clearance；
墙柱仅拒绝内部重叠，使用旧墙实现的 -1e-6 数值容差。不是跳过保护或放宽所有建造。

## 何时引入

- 低墙布局来自 a0bcf1e1；3780d3f3（2026-10-07）接入旧分支的运行时模式和保存控件。
- b12e36fe（2026-10-11）把 Pavement 选择性集成进主线时，明确未带入低墙。
  这是主线缺失的时间点，不表示此前 main 曾包含该功能。
- 本次从 a512c18b 中恢复低墙相关源代码及原生验证，按当前接口适配，
  没有整分支合并，也没有恢复 collider-lab、宽胶囊或旧相机实验。

## 干净修复与验证

### 所有权与不变量

- src/app/low_walls.rs 只负责墙的布局：独立厚度/高度、连续土墙、石/砖真实错缝，
  共享 PathStroke 断面及闭合 CellSolid。每一柱要么完整，要么全部拒绝。
- src/app/core/pavement/recipe.rs 冻结一次手势的建造种类，提供各布局自己的取样、
  预览及覆盖规则。铺路仍进入原有 adaptive courses、平滑与回拖实现。
- src/app/core/pavement/low_walls.rs 检查全底面支撑、边界和闭合完整墙柱，
  不使用任何实验碰撞表示。
- candidate.rs 的同一预检/发布入口处理两类建造：检查实时 atlas 中完整柱体及
  体素壳，保护上方障碍；深处不可能被墙改写的天然岩石不借用铺路的浅底层排斥规则。
  地面快照与尺寸冻结；同一手势不能射到自己刚生成的墙顶并继续自增高度。
- 墙的覆盖记录每柱一次；分层预览不冒充多次水平覆盖。沿用 EditHistory 的新写入
  体素所有权，不另建持久化墙对象。生成结果经普通地形发布、Dig 和地形/植被存档。
- config/gui.toml 声明四个 low_wall_* 字段。旧文件自动补缺失字段；
  旧原型 true/false、材质和尺寸保留，只刷新标签及 enabled_if。加载不改写磁盘。
  生成文件仅 src/app/generated/gui_adjustables_gen.rs，由 cargo check 再生。
- 取消仅丢弃未发布尾部；土墙顶若足够宽，下一次新手势仍可像普通土壤一样加高。
  没有自动闭合接头、围墙内填充、整墙 Undo 或持久化可编辑曲线。

### 已完成检查

- cargo fmt --check、cargo check、cargo build --release 通过。
- cargo test：App **1634 passed / 7 ignored**，另有辅助 suite **4 + 1 passed**。
  Clipboard/display 原生专用用例仍按原策略忽略，不视为已执行。
- 11 项 low_wall 相关测试：尺寸极值、非有限输入、闭合正体积、弯道完整层数、
  真实错缝/无半砖伪缝、共享端面与自交、上方保护/边界/支撑、
  高度预览及无退回恢复、真实 egui checkbox 点击、保存重载及旧配置迁移。
- scripts/validate-low-walls.sh（隐藏、静音 Release）通过：
  三种材质 × 两种高度，每例 6/6 柱；高度冻结、无内部半砖裂缝、无内部填充；
  新障碍在冻结地面之后写入仍被整段拒绝，拒绝不改 atlas，合法新目标可原手势恢复；
  同手势重放不再生成，已有砌体不能叠铺，深处基岩字节不变；
  重启核对 **169 个体素见证点与 6 株植被计数**；
  Dig 分别移除土/石/砖 **126 / 139 / 137** 个体素，存档文件保持不变。
- scripts/validate-pavement.sh、scripts/validate-pavement-prototype-native.sh、
  scripts/validate-pavement-recovery.sh 通过：原有三种路面、左右正常/压缩弯道、
  精确体素/植被回拖与取消、分支、静止指针、平滑、拒绝后恢复、保存重启均保留。
- 最终 cargo run --release -- --hidden --mute --auto-exit 0.5 通过；
  同 worktree --latest-log / --tail-latest-log 检查正常退出、failures=0，
  日志诊断未发现 ERROR/panic/Vulkan 错误。所有原生验证均核对 GUI 文件未被运行改写。
- 已查看六种墙样例截图和 Pavement 控件截图。未自动启动可见游戏，
  未声称 OS 鼠标端到端、音频听感、玩家实际行走或性能验收。

### 本地证据

- target/low-walls-all-tests.log、target/low-walls-unit.log
- target/low-walls/{build,seed,reload,dig,controls,smoke}.log
- target/low-walls/{candidates,controls}.png
- target/low-walls/placed.rflterrain、target/low-walls/witness.json
- target/validate-{low-walls,pavement,pavement-prototype-native,pavement-recovery}-restored-walls.log
