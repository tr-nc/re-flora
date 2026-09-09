# Blacky 地形 probe 查询消融实验

本轮只诊断已经获得用户视觉认可的 `40e906a6` 的运行成本；不把实验变体
合入生产 shader，也不新增性能放行阈值。主问题是：修好黑块是否必须让每个
地形像素对候选 probe 做真实体素线段验证，以及硬半球筛选能解释多少成本。

## 对照定义

- `original`：保存的修复前 `29cbeff9` release 程序，作为历史参考。
- `current`：当前完整修复，一轮八邻域、wrap 权重、精确体素可见性、按可见权重归一化。
- `precull`：只在当前精确查询之前恢复 `surfaceAlignment <= 0` 的硬排除；
  保留下来的 probe 仍用当前 wrap 权重和归一化。没有同时换成 hard 权重。
- `moments`：保留当前 wrap 权重和按可见权重归一化，只将精确可见性换成
  原有距离矩统计可见性。与最初查询不是同一实现。
- `stats`：重新执行当前八邻域候选查询，记录实际线段调用数及 DDA 循环次数；
  另外统计硬筛选能留下的子集。只作独立计数捕获，绝不用于性能计时。

所有变体保留原来的无支持 fallback。变化引起的 fallback 命中率变化属于
消融结果；因此 pass 时间差是整个变体的端到端成本差，不能称为某几行代码的
独立耗时，也不能忽略编译器寄存器分配、分支发散或缓存行为的影响。

`python3 scripts/build_tree_lighting_ablation.py` 从同一生产 shader 构建变体，
每个变体都跑 `cargo check` / `cargo build --release`，并在 finally 恢复生产
源码和默认 release。`target/summer-evidence/blacky/ablation/manifest.json`
记录 binary SHA-256；默认 release 恢复后必须与 current 完全相同。
各诊断 shader patch 保存在同目录，没有提交到生产路径。

## 测量方法

运行入口是 `target/summer-evidence/blacky/ablation/benchmark.py`。
相同 Blacky 镜头、配置字节、树、太阳参数，隐藏叶片及特效以隔离地形查询。
所有 app 由 `/tmp/re-flora-summer-gpu.lock` 串行保护，hidden/mute，无编译并行。

```sh
flock /tmp/re-flora-summer-gpu.lock "$binary" \
  --hidden --mute --windowed --perf --camera-snapshot blacky \
  --no-flora --no-particles --no-clouds --no-god-rays --no-lens-flare \
  --auto-exit 55
```

两个顺序相反的轮次：current / precull / moments / original，然后逆序。
每次仅取同一稳定帧区间 3300..6300，每 30 帧一个原生日志样本；
每轮每项 101 个样本，两轮合计 202。检查区间完整、无 probe_trace 更新、
无丢失计时 scope；每 0.5 秒采样 rustc 进程。截图 FPS、启动耗时不计入。
GPU scopes 互相嵌套，不能相加；app 帧耗时包含等待，不能解释成纯 CPU 运算。



## Release 测量结果

RTX 3060 Ti，物理 1920×1080，tracer 960×540，自动 MAILBOX。
每项每版本合并两轮各 101 个样本，共 202 个。单位 ms。

| 项目 | original | current | precull | moments |
| --- | ---: | ---: | ---: | ---: |
| GPU 总帧 | 1.7985 | 3.0890 | 2.8175 | 1.6280 |
| 地形主 pass | 0.7050 | 2.0060 | 1.7330 | 0.5360 |
| tracer 合计 | 0.8640 | 2.1660 | 1.8920 | 0.6940 |
| App 帧耗时 | 3.8540 | 5.3665 | 5.1575 | 3.6465 |

两轮分别的 GPU 总帧 / 地形主 pass 中位数（ms）：

- original：总帧 1.794/1.801；主 pass 0.705/0.706。
- current：总帧 3.070/3.123；主 pass 1.997/2.010。
- precull：总帧 2.801/2.842；主 pass 1.727/1.739。
- moments：总帧 1.625/1.628；主 pass 0.535/0.538。

所有预期采样帧完整、dropped=0、采样期未观察到 rustc、稳定窗口无
`ddgi.probe_trace` scope。八次运行成功退出且 shutdown failures=0，无
ERROR/panic/VUID/device-lost。原始计时值、每轮中位数/样本数、配置与 binary hash、
场景标记、latest/tail 日志见 `target/summer-evidence/blacky/ablation/perf/`。

取消真实体素验证带来的下降远大于单独恢复硬筛选。这是同一消费路径的
端到端消融证据，支持“新增精确可见性路径是主要成本”这一判断，不能精确分解
DDA 运算、访存、寄存器压力、分支等待各自的毫秒数。
`moments` 比 original 更快也不矛盾：它保留了当前的一轮查询及归一化，
并不是回退成原版的 canonical / smooth 查询结构。

两轮不足以建立跨 GPU、跨场景的性能保证；没有使用本测量宣称性能验收通过。
实验 shader 不进入生产代码。


## 实际路径与真实截图

使用现有 `scripts/check_tree_branch_lighting.py` 的 Blacky 精确症状判据，
三个变体均为同一镜头、960×540 浮点捕获；主黑块 65 个像素，邻近木头 80 个。
判据检查十个已知黑块与邻近木头的相对环境光，不要求所有暗像素变亮。

| 变体 | 主黑块 RGB 和中位数 | 邻近木头中位数 | 仍失败的目标体素 | 结果 |
| --- | ---: | ---: | ---: | --- |
| current | 1.759727 | 1.597947 | 0/10 | GREEN |
| precull | 0.004667 | 1.610905 | 9/10 | RED |
| moments | 1.604452 | 1.603518 | 0/10 | GREEN |

我检查了三个版本真实游戏的同镜头截图：硬筛选版本的中央及上方黑块复现；
current 与 moments 没有这些黑块。截图保留叶片、关闭与回归相同的特效，
与无叶片性能运行分开；截图 FPS 不参与性能结论。证据均在当前 worktree：

- `target/summer-evidence/blacky/ablation/capture-{current,precull,moments}/`：
  实际生产消费路径的正常五平面 RFIRR、判据结果、命令、GUI、latest/tail 日志。
- `target/summer-evidence/blacky/ablation/image-{current,precull,moments}/final.png`：
  同镜头 3 秒延迟的真实截图，与浮点捕获是分别运行，不声称同一物理帧。
- `target/summer-evidence/blacky/ablation/capture.py`：本轮顺序运行与计数分析入口。

这直接证伪了“Blacky 必须增加真实体素遍历才能消除黑块”的必要性判断。
此前加入精确查询是为了在放宽方向筛选后仍用真实遮挡约束贡献来源，
但最小修复不能仅凭该方案能工作来确定。本实验没有证明 moments 的墙体、
门洞、薄遮挡、所有 probe 密度及编辑状态都正确，也没有将它作为生产修复提交。
距离矩是方向统计可见性；把极小的非零估计重新归一化可能放大受遮挡贡献。
Blacky 通过只是有价值的便宜候选证据，不能代替这些遮挡回归。

## 精确遍历的实际工作量

独立 stats 程序与性能组相同 Blacky 镜头/分辨率/无叶片设置，捕获 published
字段。318,575 个地形接收像素，22,655 个不同 canonical 接收位置。

| 单帧计数 | current | 硬筛选留下的子集 |
| --- | ---: | ---: |
| 精确线段调用 | 2,325,101 | 1,483,056 |
| DDA 循环迭代 | 38,721,178 | 28,126,785 |
| 每地形像素平均线段数 | 7.298 | 4.655 |
| 每条线段平均循环数 | 16.654 | 18.965 |
| 没有精确可见支持的像素 | 8,131 | 13,631 |

硬筛选少了 36.2% 的线段、27.4% 的遍历循环；并非按同一比例消除所有成本。
留下的线段平均更长，且无精确支持、需要 fallback 的像素增加。
整个主 pass 还包含原有地形求交与其它着色，所以主 pass 仅下降约 13.6%。
取消精确验证的整个变体则下降约 73.3%。这些数据没有把缓存、寄存器或
GPU 分支等待单独计时，不能进一步断言是哪一种微架构瓶颈。

DDA 循环包含空块跳过和体素步进，迭代数不等于逐个体素读取次数或访存事务数。
计数来自独立 published 捕获，不是性能窗口 202 帧的平均，也没有把带计数代码
的运行用于计时。`diagnostic-stats/light.rfirr` 的前两平面专门改作计数，
不是正常 irradiance/world 数据；只由同目录 `counts.json` 的专用分析读取。

若后续继续优化，优先验证便宜候选的遮挡正确性；如果某些情况仍需精确验证，
再考虑复用相同接收点到 probe 的可见性。重复接收位置提供了调查方向，但
连续插值权重并不相同，不能据此直接缓存整份最终光照。本轮未实现这些优化。
