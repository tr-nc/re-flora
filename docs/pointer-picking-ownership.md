# 异步指针意图的所有权

`src/app/core/terrain_picking/session.rs` 的 `PointerPickSession` 是输入意图的
in-process module；`terrain_picking.rs` 是 App adapter。GPU 资源、场景版本和
异步读回继续由 `src/tracer/surface_pick.rs` 拥有，不引入另一套调度器。

## 行为合同与 interface

- `request` 在可用且没有 pending 意图时才调用提交函数。提交失败不占用名额；
  后续请求不能覆盖已经捕获的点击。
- 提交时捕获工具、植物品种、placeable、半径和是否要求继续持按。
  `PointerContext` 每次从真实 `PlayerToolRuntime` 取得当前事实。
- `complete` 只消费相同 ticket 的 Edit 答案，且仅消费一次。场景过期、工具变化、
  超时或持按失效会拒绝动作，但仍结束该次意图。无关 ticket、Kick 和 Validation
  不能消费 Edit 意图。
- 初次点击允许在松手后完成，后续连续 dab 必须仍持有原动作。返回的射线和命中
  都来自捕获答案；确定未命中也不能替换成 hover 的命中。
- Hover 使用最新**已完成**答案，不以最新请求阻塞反馈。旧答案或重复答案不延长
  缓存寿命。观察时再次检查场景版本和 250 ms 有效期。
- `cancel` 同时丢弃 pending 意图和 hover，返回可取消的传输 ticket；已在 GPU 中
  飞行的旧 Edit 答案之后不能触发动作。`expired` 只用于提前回收传输，完成规则
  自身也检查超时，不依赖 App 先执行某个清理步骤。

App 只负责取得当前输入事实、请求／取消 GPU 工作、提供 Tracer 的场景有效性判断，
并执行被接受的动作。屋顶模型遮挡、可编辑范围、植被／地形修改和连通性收尾仍由
原所有者检查。Kick 的独立冷却与捕获意图没有并入该 module。

## 验证 seam

测试通过和生产相同的 request / complete / hover / cancel interface，使用真实
工具选择、按下／释放调用模式和显式时间，不构建窗口或 GPU。覆盖快速松手、原动作
持按、提交失败、忙碌／不可用、选择变化、无关／重复答案、取消后的在途答案、
超时临界值、场景变化及离散石块放置。替换旧的孤立 hold helper 测试。

测试 fixture 只构造不透明传输答案；真实场景版本、GPU 解码和帧所有权仍由原有
Tracer 测试和隐藏 Release raster-tree smoke 验证。实际 Soil 持续倾倒／释放可用
`RE_FLORA_SOIL_BAG_REVIEW=stability` 配合 `--rooftop-poc --hidden --mute` 验证。
自动测试不替代玩家对交互手感的确认，也不是性能验收。

## 本次验证（2026-10-16）

- `cargo fmt --check`、`cargo check`、`cargo test`（1603 app + 4 collision，7 ignored）、
  `cargo build --release` 通过。Clippy 完成，警告位于未修改的既有代码。
- 普通隐藏静音启动与 raster-tree smoke 无错误；真实帧异步 picking 的世界坐标、
  rest 坐标和 frame-fence 校验通过。
- Soil fixture 的行为断言通过：38 个持按帧、0 个缺失帧、4096 个新增体素，
  释放／工具切换隐藏正确、物料守恒。该运行**不是无 Vulkan 错误的通过**，见下文。

### 影响范围

Apple M4 Pro / macOS 的 `--rooftop-poc` 启动出现 Vulkan validation error。
本次和原始基线都继续运行并通过 Soil 行为断言；普通启动和 raster-tree smoke
未观察到该错误。其他设备是否受影响未知。

### 根因

日志确认 `vkCreatePipelineLayout` 的同一 shader stage 请求 10 个 storage image
binding，超过设备 `maxPerStageDescriptorStorageImages=8`。这是布局与设备能力
的合同冲突；具体哪条 pipeline 生成路径尚未定位，不能归因于指针意图 module。

### 引入时间

在独立 detached worktree 重新构建原始基线 `190c9249`（2026-10-13），运行完全
相同的 Soil fixture，复现同一错误与相同的 38／0／4096 行为结果。因此本次两项
重构没有引入它；最初引入的提交和日期未确认。

### 干净修复

后续应在实际 pipeline layout 所有者落实设备 descriptor limit 的约束，再用该
原生启动复现验证；不能关闭 validation 或仅忽略日志。本次未改动 shader／布局，
也不声称修复这个独立问题。Soil 的无 Vulkan 错误验收仍被基线问题阻塞。
日志保存在 `target/architecture-review/picking-soil-stability.log` 和
`target/architecture-review/roof-baseline-soil.log`。
