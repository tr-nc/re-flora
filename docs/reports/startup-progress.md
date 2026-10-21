# 启动静默等待：定位与进度日志

## 影响范围

2026-10-21 的用户日志在 `22:14:25.449 Selected physical device` 后，直到
`22:14:53.440 Loading complete` 才再次输出：静默 **27.991 秒**。
已在同一台 RTX 3060 Ti 上以默认验证层的 Release、hidden、mute 启动路径复核。
“选择显卡”只是最后一条可见消息，并非实际的长耗时步骤。

## 已确认原因与边界

两个问题需要分开：

1. **为何没有进度**：默认过滤器只保留 summary INFO 和所有 WARN/ERROR；
   初始化内部 INFO 被隐藏，而且同步 Vulkan 调用阻塞窗口线程时没有进度心跳。
2. **时间花在哪里**：同步创建 Vulkan 渲染管线，而不是读取着色器模块或生成花园。

最终默认日志运行的阶段墙钟时间（不是 GPU 帧耗时，也不是性能基准的统计均值）：

| 阶段 | 耗时 |
|---|---:|
| 窗口与 Vulkan 上下文 | 0.144 s |
| 着色器模块 | 0.138 s |
| 渲染资源 | 0.100 s |
| 计算管线 | **23.492 s** |
| 图形管线 | **3.912 s** |
| 地形与模型缓存 | 1.832 s |
| 世界对象与初始光照 | 0.304 s |
| App 初始化至首帧完成，总计 | **30.272 s** |

另一次开启逐管线 DEBUG 的诊断运行进一步定位：

- 计算管线阶段 21.603 s，其中 **44 次 `vkCreateComputePipelines` 合计 21.589 s**。
- 主光线追踪管线 **`tracer.comp` 单项 18.797 s**；其次 `raster_tree_lighting.comp` 1.103 s。
- 图形管线阶段 7.541 s，其中 **29 次 `vkCreateGraphicsPipelines` 合计 7.487 s**；
  该次最慢的是 `particle_billboard.vert + water_droplet.frag`，3.359 s。
- 这是单次 API 调用的主机墙钟测量，包含当前启用的验证层和驱动工作；
  尚未分离两者，也没有证据把耗时全部归为缓存未命中或某个驱动内部编译步骤。
  诊断与默认运行不能混成一个性能对比，图形阶段有明显波动。
- 代码目前同步创建管线，传入空 `VkPipelineCache`。这说明没有应用显式提供的
  管线缓存，**不等于证明驱动缓存失效**。本次没有改缓存、着色器、验证层或质量。

## 引入时间

`04b4eb8f`（2026-10-03，`keep ordinary run logs focused on core events`）引入了
现有 summary-only 默认过滤策略，可由 `src/main.rs` 的历史直接确认。
它解释了为什么默认日志隐藏内部初始化消息；**不能据此断定 28 秒启动耗时也由该提交引入**。
本次没有二分启动性能退化的起点，也未将其归因于最近的花园/音效改动。

## 修复与验证

- `src/util/startup_progress.rs` 提供 App 持有的单一启动观察器：真实阶段的
  `begin` / `done`、阶段/累计耗时，长阶段每两秒 `waiting`。
- 使用已有 summary INFO 通道，正常启动无需环境变量。线程只报告所观察到的阶段，
  不接触 Vulkan、窗口或游戏状态；心跳表示“仍在等待”，不伪造百分比、ETA 或驱动进展。
- 报告一直持续到首个成功呈现的游戏帧完成 GPU 工作；此后 `ready` 并唤醒/join 线程。
  提前退出/错误或栈展开只报告 `aborted`，不会误报完成，也不在游戏过程中继续打印。
- `crates/re-flora-vkn/src/pipeline/{compute_pipeline,graphics_pipeline}.rs` 的
  `[PIPELINE_CREATE]` DEBUG 记录提供可选的着色器名称与 API 调用计时，不刷普通启动日志。
- 使用方法及日志契约见 [`../runtime_logging.md`](../runtime_logging.md)。

通过：`cargo fmt --check`、`cargo check`、`cargo test`（1671 app + 4 collider +
1 integration；原有 11 app / 4 integration 忽略项未启用）、vkn 独立 66 tests、
`cargo build --release`。三个新增测试覆盖阶段切换/去重、主线程阻塞期间的心跳、
成功与错误退出的线程回收；原有默认日志过滤测试仍通过。

原始日志在 `node scripts/check-startup-log.mjs <log>` 下为 **RED**；新默认与诊断
启动为 **GREEN**。最终默认运行有 12 次心跳，最大 STARTUP 消息间隔 **2.0011 s**，
首次帧完成后无后续启动消息，也不输出逐管线 DEBUG。

真实缺失快照的启动用例正确记录 `aborted`，不记录 `ready`；现有 AppController
以 panic / exit 101 结束该错误用例，符合此项负向验证的预期，并未修改其错误处理。
正常 hidden/muted 运行没有 ERROR/WARN、VUID、SYNC-HAZARD 或 panic；详细诊断运行
结束于 `[SHUTDOWN] phase=complete failures=0`。GUI 与相机文件 SHA256 不变。

本机证据保留在 `target/startup-review/`：原始日志、默认/诊断日志、阶段汇总、
`pipeline-attribution.json`、错误路径日志、测试与构建日志。

**本次交付解决“启动期间不知在做什么”，并定位了耗时所在；没有宣称缩短启动时间。**
进一步降低启动耗时，应针对 `tracer.comp` 的管线创建调用区分验证层/驱动工作及缓存行为，
另做保持渲染行为不变的 Release 测量，而不是降低画质或关闭功能掩盖等待。
