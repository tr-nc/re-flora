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

## 验证与交接

- `python3 scripts/build_tree_lighting_ablation.py`：current、precull、moments、stats、
  restored 均执行 `CARGO_BUILD_JOBS=2 cargo check` 与 `cargo build --release` 成功。
- `CARGO_BUILD_JOBS=2 cargo fmt --check`：通过。
- `CARGO_BUILD_JOBS=2 cargo test -- --skip app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`：
  915 passed、0 failed、1 ignored、1 filtered out。跳过的是此前已确认的相机快照
  fixture 基线失败；用户的五个快照没有为迁就测试被删改。
  曾误用 `cargo test --lib`，该 package 没有 library target，命令拒绝执行；
  随后改用上述实际测试目标通过，日志保留。
- `flock --close /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5`：
  正式恢复版本 smoke 通过；实际脚本通过 subprocess env 设置同一环境变量。
  同 worktree `--latest-log` / `--tail-latest-log 200` 检查成功退出、shutdown
  failures=0，无 ERROR/panic/VUID/device-lost，见 `ablation/hidden-smoke*.log`。
- Python 构建脚本及本轮 artifact runner 通过 `python3 -m py_compile`。
- GUI 和 camera snapshots 与本轮实验开始时的 SHA-256 完全一致；仅用户预存的
  `config/camera_snapshots.toml` 改动仍未提交。

本轮改动文件只有 `scripts/build_tree_lighting_ablation.py` 和本报告。
构建脚本提交 `5603c1c2`、诊断法线补全 `05a1cf9e`；性能记录 `c3436828`；
视觉与计数记录 `937ae44a`。正式修复仍为 `40e906a6`，shader 与 Rust 没有
留下消融代码；生成文件无最终变化，默认 release 的 SHA-256 与 current 一致。
没有修改其它 worktree、全局配置或环境光权威结构，没有打开可见游戏。

未验证项：moments 候选完整遮挡正确性、其它 seed/镜头/probe 密度、编辑过程、
其它 GPU；计数只有一个发布帧，性能只有两个顺序相反的轮次。未实现后续优化，
未将候选变体认定为发布或性能验收通过。


## 后续遮挡验证：已发布字段

继续验证 moments 候选，未更改生产 shader。当前正式版本首先尝试
`--environment-irradiance-capture-target converged --auto-exit 120` 的 sealed/32
捕获：程序返回 0，但没有捕获文件，现有过程验证器报缺少 capture saved event。
因此没有把它记为通过，也没有把下面的 published 测量冒充收敛矩阵。

随后以原有 `ddgi_evidence.plan._correctness_body` 的场景、两种密度、精确参考及
数值判据进行 published 对照。所有捕获实际为 e0 / converging / published，
不是 converged；1440×810，同组 world/hit/metadata 通过参考兼容性检查。
以下只是该阶段的诊断门槛结果，不等于完整 correctness suite。

| 场景 / 间距 | current 参考误差 P99 | moments 参考误差 P99 | current | moments |
| --- | ---: | ---: | --- | --- |
| sealed / 32 | 0 | 1.0395384e-06 | PASS | FAIL |
| sealed / 16 | 0 | 0 | PASS | PASS |
| portal / 32 | 0.0072104897 | 0.021794469 | PASS | FAIL |
| portal / 16 | 0.011872097 | 0.01500428 | FAIL | FAIL |
| walls / 32 | 0.39802872 | 0.39186829 | PASS | PASS |
| walls / 16 | 0.36226754 | 0.35986843 | PASS | PASS |

sealed/32 的候选失败来自最大亮度 1.0511453e-5，略超过原有 1e-5 上限；
current 两种密度均精确为零。这个量级不能描述成肉眼明显漏光。
portal 的原有误差 P99 上限是 0.01；current 在 spacing16 也超限，不能只报告
候选的问题。墙体场景包含 1 体素墙、2 体素墙及阶梯薄墙，两者均通过该阶段
原有阈值；并不表示误差为零。下一步对原版基线与误差位置进行核查。

完整命令、浮点数据、分析器失败原因、latest/tail 日志位于
`target/summer-evidence/blacky/verification/{scene}-{spacing}-{label}/`。
运行脚本为同目录 `run-published.py`；原收敛失败保存在
`converged-sealed-32-current/`。所有 app hidden/mute、flock 串行，GUI 与用户
快照恢复原字节。生产 shader 和 generated 文件没有修改。

原版基线补查：`29cbeff9` 的 portal/16/published 参考误差 P99 为
0.0113067，也超过 0.01；当前为 0.0118721，候选为 0.0150043。因此这项超限
有基线成分，候选进一步变差。原版 sealed/32/published 精确为零。
原版 sealed/32/converged 也在 120 秒后返回 0 而没有捕获文件，与当前相同；
收敛捕获阻塞不能归因于本次消费查询修复，完整收敛验证仍未完成。
命令与日志在 `verification/{portal-16-original,sealed-32-original,converged-sealed-32-original}/`。

逐像素差分（同一 world XYZ、环境光场身份）显示 portal/32 有 23,403 个地形
像素比当前暗超过 0.01，只有 950 个亮超过 0.01；候选主要新增偏暗区域，
不是整体抬亮。一个峰值点 world=(0.562562,0.855198,1.054687) 的参考亮度
0.142374、current 0.149623、moments 0.052630。portal/16 也以偏暗为主。

walls/32 有点 world=(1.554158,0.746742,1.187500)，current 与参考均为零，
moments 为 0.399618；宽松的全图 P99 阈值不能排除这种局部差异。
还需核查这些点的精确支持，不能仅以差异断言真实物理光照应当为零。
各组差分在 `verification/{scene}-{spacing}-difference.json`，入口 `compare.py`。
portal/sealed 的 direct-light planes 字节一致；walls 不一致，因此这里只比较
环境光，未把完整最终颜色视为严格逐像素对照。

### 局部精确支持与截图复核

独立 stats 捕获与当前/候选的 receiver XYZ、hit mask、geometry/radiance/field
身份及 update_epoch 一致。前两平面是计数，不是光照与世界坐标；接收平面的
第四分量有 79 个值不同，因此只以不变的 XYZ 对齐，未误当成全平面字节一致。

walls/32 有 7,118 个像素满足：当前环境光为零、候选亮度大于 0.1，且当前
精确查询没有可见支持。0.1 仅用于描述范围，不是新增放行阈值。上述峰值点
执行四条线段、合计四次 DDA 迭代，四条可见性均为零。即全部在第一轮退出；
这也提示起点落入实体等接收点问题需要检查。**没有可见 probe 支持不等于
物理光照必然为零**，不能以这个统计把该处候选变亮直接定性成真实漏光。
具体原因尚未通过起点坐标/占据证据区分，本轮没有扩展修改偏移策略。

记录在 `verification/walls-32-exact-support.json`；计数来自
`walls-32-stats/light.rfirr`，专用分析入口 `support.py`。

实际截图来自 hidden 游戏的固定环境测试场景，间距32、FOV60、同相机，
3秒延迟；我已逐张检查，门洞有局部亮度差，薄墙的部分黑线在候选中消失。
没有对图片后期提亮，也没有把这些变化直接称为正确修复。
截图与浮点捕获是分别运行，不能当成同一物理帧；路径为
`verification/image-{portal,walls}-{current,moments}/final.png`。
截图入口 `visuals.py`；每个截图目录保存实际命令与 latest/tail 日志。

### 重复性与本轮交接

portal/32 和 sealed/32 的 moments 失败均各重复一次；两次捕获的环境光
payload SHA-256 分别完全一致，分析器返回同一失败。可运行的红灯入口是
`python3 target/summer-evidence/blacky/verification/repeat.py`，结果见
`{portal,sealed}-32-moments-repeat/analysis.json`。这锁定了候选实际路径的
退化，未仅依赖截图肉眼判断。

本轮结论：**简单地用距离统计替代精确验证，虽然解决 Blacky 且明显更快，
但尚不能替换生产修复。** 已证实门洞新增偏暗、密闭空间的极小非零光；薄墙
存在缺少精确支持时的大幅局部差异，但起点有效性未进一步诊断，不能将它
直接定性为物理漏光。没有证明每个像素都必须进行完整精确遍历，也没有
实现新的优化方案。

本轮重新执行 `CARGO_BUILD_JOBS=2 cargo fmt --check`、`cargo check`，均通过；
`cargo test -- --skip app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`
为 915 passed / 0 failed / 1 ignored / 1 filtered out；跳过同一已知相机 fixture
基线失败。正式 release 的 `--hidden --mute --auto-exit 0.5` smoke 通过，
同 worktree latest/tail200 无 ERROR/panic/VUID/device-lost，shutdown failures=0。
上述日志在 `verification/{fmt,check,test,hidden-smoke,hidden-smoke-tail}.log`。
没有重新测性能，本轮不改变此前 release 消融结论，也不宣称性能验收通过。

仅修改本报告；生产源码、默认 release 二进制 SHA-256、generated 文件均未改变。
GUI 已恢复，用户预存的 camera snapshots 改动保留且未提交。所有 app 均 hidden、
mute、flock 串行，没有打开可见游戏。完整收敛状态、编辑过程、其它 GPU 仍未验证。

## GUI 对比开关（用户授权体验）

现在提供同一个程序内的即时切换：R 配置面板底部的 **Environment Probes**
区，`Cheap terrain lighting (experimental)` 复选框。
关闭使用当前精确验证；打开使用此前 moments 候选的同一权重与归一化算法。
默认关闭，仅在本次进程有效，不写 GUI 参数、不修改环境光权威事实，也不重建
probe 场。勾选时显示尚未通过遮挡验证的候选说明。
`--ddgi-terrain-moments` 可令同一个开关以勾选状态启动，用于自动验证和此次体验。

运行路径：UI 调用 Tracer 的会话模式 setter，下一帧写入 ShadingInfo 的
`ddgi_terrain_moments`，仅地形消费查询选择 visibility estimator。probe transport、
栅格查询、光源与已有 fallback 保持原路径。启动和切换都有 `[DDGI]` 模式日志。

验证：fmt/check 通过；cargo test 跳过同一已知相机 fixture 后 916 passed、
0 failed、1 ignored、1 filtered out。新增 CLI 测试确认普通启动必须默认关闭。
同一 release 程序两种启动状态的 Blacky 实际捕获均为 GREEN，十处黑块均通过。
主点环境光 RGB 和分别为 1.759727、1.604452，与此前两独立候选一致。
证据在 `target/summer-evidence/blacky/gui-toggle/{exact,moments}/`，有实际命令、
浮点数据、判据结果和 latest/tail；`moments.sh` 只是为旧回归 runner 传递新增旗标。
GUI 鼠标切换由用户接下来手动体验；自动捕获验证的是两种模式的完整 GPU 路径。

本次新增动态分支未重测 release 性能，不能直接套用旧独立程序的毫秒值。
历史 `build_tree_lighting_ablation.py` 针对之前的固定源码接缝；新程序的两模式
比较直接使用 CLI/GUI 开关，不再需要生成那两个临时 shader 变体。
共享文件改动限于开关与必要传递；没有编辑 gui_adjustables.rs 或环境光权威结构。
`src/auto-generated/gpu_structs.rs` 由 cargo check 更新一个模式字段及相应 padding，
未手改 generated 文件。用户的 camera snapshots 预存改动未提交。

开关实现提交 `2aa7fb90`。随后标准
`flock --close /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5`
smoke 通过；latest/tail200 确认正常退出、shutdown failures=0，无错误。
两模式的 RFIRR 环境光与 world 平面分别与旧独立候选逐字节一致。
证据在 `gui-toggle/{smoke,tail}.log`、`gui-toggle/latest.txt`。
接下来用户手动试用从当前 worktree 以 `cargo run -- --camera-snapshot blacky --ddgi-terrain-moments`
启动；这是手动视觉/交互验收，不以调试运行 FPS 代替 release 性能测量。

## 用户体验后的默认选择：Cheap

用户体验 Blacky 与门洞后明确选择性能，授权 Cheap 为默认并接受观察到的视觉差异。
因此普通启动现在使用 moments 路径，R / Environment Probes 的
`Cheap terrain lighting` 默认勾选；取消勾选仍即时切回精确验证。
本次会话的手动切换不保存，下一次普通启动恢复 Cheap。

CLI `--ddgi-terrain-exact` 提供明确的精确模式覆盖；`--ddgi-terrain-moments`
保留为显式 Cheap 选择，同时指定两者报参数冲突。GUI 不再把已选择的默认
模式展示为待决定候选；此前的数值差异、基线限制和验证证据保持记录，
用户接受取舍不意味着那些数值门槛现在通过。没有新增性能测量或放行阈值。

仅修改 `src/cli.rs`、`src/app/core/mod.rs` 和本报告；shader、generated 文件
没有进一步变化。fmt/check 通过；同一已知 fixture 跳过后的 cargo test 为
917 passed / 0 failed / 1 ignored / 1 filtered out。
默认启动和显式 exact 的 Blacky 实际 GPU 捕获均 GREEN，十个目标无失败；
主点 RGB 和分别 1.604452 / 1.759727，证明默认路径已改变且精确路径仍可选。
证据位于 `target/summer-evidence/blacky/cheap-default/{default,exact}/`，
含实际命令、捕获、结果、latest/tail 日志；相邻 `cheap-default-{fmt,check,test,build}.log`
记录构建验证。GUI 已恢复，用户相机快照预存改动保留未提交。

默认切换提交 `74eb5986`。随后标准 release hidden/mute/auto-exit0.5 smoke 通过，
启动日志确认默认 `terrain_consumers=experimental-moments`（保留原诊断日志标识），
shutdown failures=0，latest/tail200 无 ERROR/panic/VUID/device-lost；见
`cheap-default/{smoke,tail}.log`、`cheap-default/latest.txt`。本轮未自动打开可见游戏。
