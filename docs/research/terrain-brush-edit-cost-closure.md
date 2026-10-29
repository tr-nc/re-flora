# 地形与普通草刷成本：闭环调研

2026-10-29；产品源码基线 `288a907f`（研究文档提交 `316b4be0` 不改代码）。本轮在隔离 worktree 中补计时和固定回放，未向产品合入诊断或优化代码。

[HTML 报告](../reports/terrain-brush-edit-cost.html) · [第一轮调查](terrain-brush-edit-cost.md) · [完整测量数据](../evidence/brush-cost-closure/results.json)

## 决策摘要

1. **确认第一优先级是同步提交边界，而不是地形笔刷 shader 数学。** 本机常规渲染队列尚有工作时，编辑在主线程等自己的 job，顺带等此前共享队列工作。密草挖地的体素修改 GPU 中位数只有 **14.6 μs**，取样 **5.5 μs**；对应 CPU wait 中位数 **6.33 ms**。不能再把原来的「约 7 ms」描述成修改算法自身成本。
2. **草刷自身也确有约 3.3 ms/单 chunk、6.6 ms/跨两 chunk 的事务成本。** 这些是在显式提前排空 GPU 的诊断模式下量到的，不含被挪到计时区外的等待；不是优化结果。surface 的重建和整块 occupancy 是主要 GPU 工作，密草 hydration 对齐是另一个可测 CPU 成本。
3. **微小笔刷与重复同一笔几乎一样贵。** 无草小点半径由 14 voxel 降到 2 voxel，编辑 pass 仍约 155 μs；密草已满区域重刷仍然跑完整流程。固定开销和无变化工作比进一步优化 capsule 距离公式更值得做。
4. **否定两个“看上去变快”的捷径。** 提前 `wait_idle` 只搬走等待，没有消除它；改 IMMEDIATE 呈现也没有稳定收益。都不作为修复交付。
5. **不改现有连线兜底。** 80 ms 节流、单个 pending pick 和只保存成功端点仍限制轨迹精度。优化单笔成本与输入采样策略是两件事。

## 实验设计与有效性

- Apple M4 Pro / macOS / MoltenVK；原生 hidden + mute，5120×2880 屏幕、1024×576 场景，默认自动选择 FIFO、3 个 swapchain images。主表**不启用 `--perf` 面板**；使用 Release 构建和限定模块日志开启分段 GPU timestamps。
- 66 次独立运行，**1,980 笔测量 + 198 笔预热**。每次 3 笔预热后 30 笔正式测量；每组两个进程，反向场景顺序复测。
- 首先跑 13 场景的带 `--perf` 扩展矩阵（两轮），随后发现该面板本身有渲染成本，因此补跑不带面板的 6 个主场景、4 个 drain 诊断场景、2 个 IMMEDIATE 场景（均两轮）。两套数据严格分组，不混池。
- 调用的是生产 `App::apply_surface_flora_regeneration_at / apply_surface_flora_removal / apply_surface_terrain_removal / apply_surface_terrain_placement`。没有把 authored-flora benchmark 当成普通草刷，没有跳过 surface/contree/publication/hydration。
- 固定平地泥土 slab；普通草根数量：空 0、稀 784、密 50,176。密草点操作影响一个 chunk，内含 12,544 个原始草根；跨界操作影响两个 chunk。合成根 fixture 用于可重复压力，不声称代表随机自然草分布。
- 点半径 14 voxel（tiny 为 2）；long 为同 chunk 内约 195 voxel 线段，cross 为跨两个 chunk 的约 143 voxel 线段。具体坐标见诊断补丁。
- 每笔先在未计时阶段恢复相同地形/实例，再渲染 8 帧后编辑。预编辑重型 GPU 读回和 O(N log N) 校验特意放在 reset 阶段，避免校验先消耗队列积压；编辑前只复制 CPU hydration 值用于结果核对。编辑后的验证也不在计时区内。
- 每笔验证根不重复、幸存草 growth/spawn/hydration 不被 add/remove 意外改变、重复草添加幂等；排序后计算实例结构和体素材质哈希。**全部 13 个场景在所有重复、诊断模式中，输入/输出根数量、修改体素数量、实例哈希、体素哈希完全一致。** 哈希不包含自然随时间变化的 moisture bits；hydration 在单笔前后逐根核对，不靠跨运行哈希替代。
- 所有 66 次运行成功完成，日志未出现 ERROR、VUID、panic。没有以少量样本的最快值冒充优化收益。

这是**编辑 API 事务成本**测量，排除输入事件/拾取等待、fixture reset、验证和未来帧派生工作；不是鼠标到屏幕的端到端延迟，也不是连续拖拽吞吐基准。

## 1. 默认 FIFO、不带性能面板：实际调用成本

每行合并两轮共 60 笔，median 为中间两值平均，p95 为 nearest-rank。单位 ms。

| 场景 | median | p95 | 两轮各自 median |
|---|---:|---:|---|
| 空地加草，半径 2 | 6.990 | 14.307 | 5.302 / 7.415 |
| 空地加草，半径 14 | 7.236 | 15.971 | 5.244 / 8.448 |
| 密草加草，单 chunk | 9.513 | 17.146 | 7.343 / 9.879 |
| 密草加草，跨两个 chunk | 12.709 | 20.102 | 10.602 / 13.187 |
| 密草挖地，单 chunk | 10.908 | 18.611 | 8.726 / 11.197 |
| 密草重复同一笔，无新增根 | 9.620 | 16.920 | 7.389 / 9.886 |

**环境漂移不能隐去。** 第二轮调用时间比第一轮普遍高约 2–3 ms；surface GPU 时间却仍约 1.24 ms/块，grass occupancy 约 .70 ms/块。CPU wall time 受提交时队列状态、桌面/驱动调度和呈现依赖影响。本表是本机此回放的分布，不是跨设备预算，更不能从不同时间段各取一个 median 宣称 20% 提速。

### 扩展矩阵（单独开启了 `--perf`，不要与上表直接比较）

| 场景 | median / p95 ms | 解释 |
|---|---|---|
| 稀草添加，单 chunk | 8.141 / 15.349 | hydration 约 .029 ms |
| 密草添加，同 chunk 长段 | 10.184 / 17.734 | hydration 约 .625 ms，edit GPU 约 .157 ms |
| 密草删除，单 chunk | 10.291 / 17.636 | 与添加一样付整块成本 |
| 密草删除，跨两 chunk | 13.093 / 20.847 | 固定成本随 chunk 数增长 |
| 空地挖地 | 7.828 / 15.230 | 去除草存活环节后的参照 |
| 密草挖地，跨两 chunk | 15.422 / 22.955 | 真正修改 4,584 个 voxel |
| 密草填地，单 chunk | 11.470 / 19.147 | 修改 616 个 voxel，仍需草存活发布 |

每个场景都是 60 笔。完整 13 场景和各个 probe 分布保留在 JSON，不把面板开销误算成普通玩家草刷固有成本。

## 2. 因果验证：把等待提前，时间只是搬家

诊断操作：在 API 调用之前执行 `device.wait_idle()`，分别记录 **preflight** 和 **edit**，报告逐笔相加后的分布，不能只报 edit。

| 场景（均无面板） | 正常 edit median | 排空后 edit median | preflight median | preflight+edit 的逐笔总和 median |
|---|---:|---:|---:|---:|
| 空地加草 | 7.236 | 2.820 | 3.562 | 6.432 |
| 密草加草 | 9.513 | 3.277 | 6.592 | 9.921 |
| 密草跨界加草 | 12.709 | 6.622 | 6.546 | 13.320 |
| 密草挖地 | 10.908 | 4.702 | 6.668 | 11.484 |

median 不可相加，最后一列是在每笔上求和再统计。结果受到上述轮间漂移影响，但明确否定“wait_idle 让密草刷从 9.5 ms 优化到 3.3 ms”的说法。

更直接的地形 probe：

- 正常密草挖地：`modify_gpu` median **14.583 μs**，`sample_gpu` **5.480 μs**，CPU `wait` **6,331.5 μs**。
- 排空后：`modify_gpu` **17.605 μs**，`sample_gpu` **3.042 μs**，CPU `wait` **215.875 μs**；约 6.7 ms 在 preflight，而不是消失。
- 独立沿用原场景 far 回放也复现：一笔约 7.27 ms job wait / 14.2 μs modify；排空时约 7.27 ms preflight / .274 ms job wait / 18.8 μs modify。参见 [queue-probe.log](../evidence/brush-cost-closure/queue-probe.log)。

**来源支持因果解释**：[`VulkanContext::submit_render_commands`](../../crates/re-flora-vkn/src/context/vulkan_context.rs):167–184 把正常渲染提交到 general queue；编辑也提交同一 queue 并同步等待 job。帧提交还有 swapchain image-available 信号依赖。证据能定位到“自身 kernel 之外的共享队列/提交完成等待”，不能仅凭这些计时进一步断言每一微秒属于某个 draw call、vsync 或 OS 调度。

### IMMEDIATE 不是解决办法

已有 CLI override 仅用于诊断，未改默认配置。不开面板的两轮中，密草添加合并 median/p95 **10.687/19.889 ms**，密草挖地 **12.290/21.990 ms**，不优于本轮 FIFO。带面板的较早实验反而看到较低耗时，说明没有稳定跨时段收益，不能把呈现模式变化当作确定修复。它也不改变整块重建和同步资源依赖。

## 3. 剥离排队后，单笔自身预算在哪里？

以无面板、排空模式的密草单 chunk 添加为例：

| 层级 | 中位数 | 注意 |
|---|---:|---|
| 完整 edit 调用 | **3.277 ms** | 不含 preflight；是诊断下的固有服务成本近似 |
| surface CPU wall | 1.630 ms | 内含 GPU **1.237 ms** |
| 整块 occupancy 的 GPU passes | .699 ms | 属于后面的草事务，不要重复累加 CPU/GPU时间 |
| 其中 capsule edit GPU | .155 ms | 只缩这个 pass，收益量级受此上限限制 |
| CPU hydration 读回/映射/上传 | .499 ms | 处理约 12.8k 个该 chunk 的草根 |

跨两个 chunk：surface GPU **2.481 ms**、occupancy GPU **1.393 ms**、CPU hydration **1.123 ms**，完整调用 **6.622 ms**。这些相互包含的层级不是可直接相加的总账。

密度变化主要体现在 CPU 对齐：扩展矩阵中 sparse 约 .029 ms、dense point 约 .503 ms、dense long 约 .625 ms；全体积占用图 GPU 耗时大致相同。无变化重刷仍然支付相同 surface/occupancy/hydration 事务。

因此前一轮的“整块处理值得优化”成立，但需要进一步排序：**先处理同步完成边界和不必要的 surface 重建；不要只把 capsule shader 裁成局部，就宣称解决了整个草刷瓶颈。** capsule edit 只有约 .155 ms，而完整无排队调用约 3.3 ms。

## 推荐的实施边界（未实施，不是收益承诺）

### 第一项：编辑事务不在输入/主线程中间强制结算 GPU

目标是减少主线程停顿，并让编辑与正常帧明确排序。将编辑的录制、提交、完成、发布作为同一个 owner 管理的事务；使用隔离 scratch 或明确 retirement，提交后异步轮询，在完成边界发布数量、库存、revision 和观察者结果。尽量避免在已经排队的多帧渲染之后继续同步插入多次小 job。

**必须保留**：统计与库存精确一次结算，图形与拾取不能使用混合 revision，atlas 写入不踩 in-flight 渲染读，CPU 读取不能早于 GPU 完成，取消/过期输入不能变成新工具的一笔。

这并不保证 GPU/呈现延迟同时少掉全部 6 ms；异步化首先解决主线程阻塞，真实 input-to-present 延迟仍需整链测量。不能靠删除 wait、单独挪到后台线程或换队列绕开资源依赖。

### 第二项：普通草不再为不变地形重新生成整块表面

保持权威 terrain revision，复用可种植表面/活跃砖信息或从笔刷局部 atlas 提取所需表面。共享 surface 是 scratch，必须绑定 chunk+revision，不能直接省掉 `build_surface`；也不推荐无条件为每个 chunk 永久保存一张 64 MiB 全图。预期可减少的工作已测到是约 1.24 ms GPU/块，而非凭感觉猜测。

### 第三项：实例和根身份增量维护

remove/trim/terrain survival 按现有根处理；add 按局部可种植表面处理并保持排他/幂等。减少全图 clear/round-trip 和全量 hydration 重排。有限种子追加已证明仓库有单根增量能力，但不能直接把其种植语义套到 capsule 密草覆盖上。

### 暂不投入

- 不调整 capsule 兜底、不先缩短 80 ms；不靠更密补点叠加更多全量事务。
- 不优化占几十微秒的 terrain modify 数学，或约几十微秒的候选 buffer 清零作为首项。
- 不将“提前 wait_idle”或“切 IMMEDIATE”作为优化。
- 不牺牲法线质量；soil-cover patch 已增量化，上一轮测得 .2 ms 左右，不优先重写。

## 可复现材料与验证

保留在 [`docs/evidence/brush-cost-closure/`](../evidence/brush-cost-closure/)：

- `diagnostic.patch`：完整隔离计时与回放源码；适用于 `316b4be0`，`git apply --check` 已通过。不是待合入的产品补丁。研究提交为 `938911ad`、`5a8b7efc`。
- `run.mjs`：每个案例启动真实 hidden Release、检查成功/错误标记、解析 event scopes；只记录受控诊断环境，不保存继承的秘密环境变量。
- `collect.mjs`：检查 66 次运行的数量、跨模式结果一致性，再生成可移植数据；`results.json` 保留所有逐笔 probe 和 checksum、逐轮统计、命令及 binary SHA256。
- `queue-probe.log`：初始队列因果 probe 原始片段。完整本地原始日志和当时的基准 binary 保留在 `target/perf/brush-cost-closure/`，不依赖已删除的 worker。

最小复现流程：

```bash
# 在干净、隔离 checkout 中，不对正在使用的主工作目录打诊断补丁。
git worktree add ../re-flora-cost-repro --detach 316b4be0
cd ../re-flora-cost-repro
git apply ../re-flora/docs/evidence/brush-cost-closure/diagnostic.patch
cargo check
cargo build --release
mkdir -p target/research/bin
cp target/release/re-flora target/research/bin/baseline
cp ../re-flora/docs/evidence/brush-cost-closure/run.mjs target/research/run.mjs

node target/research/run.mjs baseline plain-a1 \
  add-empty-tiny add-empty-point add-dense-point add-dense-cross dig-dense-point noop-dense-point
node target/research/run.mjs baseline plain-drained-a1 \
  add-empty-point add-dense-point add-dense-cross dig-dense-point
# plain-drained-a2 / plain-a2 反向顺序；标签含 immediate 时才显式覆盖呈现模式。
# 不含 plain 的标签会启用 --perf，这组只能与同样带面板的组比较。
```

完整命令由 `results.json.runs[].command/env` 保存，复现时替换隔离目录的绝对路径。`collect.mjs` 要求全部 12 个 label 目录齐备，不会静默遗漏失败样本。

验证：worker `cargo fmt --check`、`cargo check`、`cargo test`（4 + 1,772 passed，15 ignored）、`cargo build --release`、默认 hidden/mute .5 秒运行均通过；主工作目录已经重新 `cargo build --release` 并通过 hidden smoke，Release binary 不含诊断补丁。HTML 已在浏览器检查 1440×1000 和 390×844，检查桌面各段与窄屏截图，无页面横向溢出；窄屏表格内部滚动。没有更改用户 tracked GUI 配置或存档，没有启动可见游戏，没有推送。

## 未关闭的问题（明确保留，不冒充完成）

- 尚未实施事务异步化或稀疏表面缓存，因此**没有优化后性能验收或提速百分比**。
- 尚未测真实鼠标事件 → GPU pick → 编辑完成 → 首次显示的端到端 p95；本轮闭环的是单笔成本归因与优化优先级。
- 窗口/帧排队状态与桌面负载有漂移；未用 Vulkan 系统级 trace 精确分割此前各个队列任务。不能直接把“自身 kernel 外等待”全归咎于 FIFO。
- 数据来自 M4 Pro/MoltenVK；独立显卡/Windows/Linux 上的吞吐和 host-memory 成本需另测。
