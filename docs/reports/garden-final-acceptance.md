# 花园最终集成验收（scope v4）

## 结论与交付边界

本工作树 `re-flora-agent-garden-performance` / `agent/garden-performance` 从
`68c5f3b59aa45233594f77c591fcc7f3f9314a04` 开始，完成真实花园的性能对照、联合原生生命周期
验收及两个已复现问题的修复。没有修改 main、其它工作树、用户存档、相机或已有保存设置；
没有推送、打包、模型/权重下载、可见窗口或可听播放。所有 build/test/app/perf 命令组都持有
`flock -o /tmp/re-flora-garden-delivery-heavy.lock`，无嵌套锁、后台重任务或编译与计时重叠。
有 DISPLAY 时只在测试子进程移除 Wayland 变量，未强制 present mode。

- **两项精确 GPU 优化在完整花园中仍有效**：30% duty、16 喷头 GPU frame p50
  **26.642 → 22.734 ms**（三轮分位数均值，-14.7%）；喷头逐帧 scope 和的 p50
  **18.286 → 14.027 ms**。真实窗口平均帧率 **35.76 → 41.74 FPS**。
- **没有达到所有人口 60 FPS**：8 个约 49.58 → 53.87 FPS；0/1/4 个约 58.4 FPS，受 FIFO
  限制，不能从 GPU 时间下降宣称这几个人口 FPS 提升。连续 16 个仍约 37 FPS。
- 剩余最大喷头成本是 **mist optical**：主矩阵最终 16 个 p50/p90 **12.430/15.916 ms**，
  而 prepare 仅 **0.231/0.245 ms**。没有降低粒子、灌溉、质量或关闭特效以制造通过结果。
  没有追加未经验证的 renderer 重构；后续精确候选列表优化应是独立测量项目。
- 活跃声音 → 实际花园/全空 runtime Load → 水发布 Ready → 重锚、可恢复失败、Terrain/master
  禁用/启用、全局时钟暂停/恢复、真实湿土、支撑移除及边界编辑均有原生证据。
- 默认验证构建、集成测试、50 项 Slang、660-pose、15 spray / 5 designs 同步验证通过；
  最终普通花园 hidden/muted smoke 正常关闭。**美术与听感仍待玩家批准**。

四切片背景与已保留证据见 [性能](../performance/sprinkler-scaling.md)、
[世界](garden-world.md)、[草湿度](grass-moisture-response.md)、[声音](sprinkler-audio.md)，
以及 [全空发布修复](empty-world-reload.md)。本报告不把缺少的历史覆盖当作通过。

## 1. 具体审查发现与红/绿修复

### 1.1 活跃声音在同步世界替换时没有退休（review medium）

**影响与复现**：旧接入仅把 `allows_world_updates()` 传入音频；该谓词必须允许推进水依赖，
不是“替换完成”的音频就绪事实。接受 load 时没有调用 clear，而 UI load 路径在 spatial frame
之前返回。源码上的遗漏经实际原生回归确认，不只是推测设备缓冲问题。

新增 `RE_FLORA_SPRINKLER_AUDIO_RELOAD=<task-owned snapshot>` 复用既有 audio review：先正常放置
20 个来源并实际运行 16 个声音，再调用与 UI 相同的 `perform_runtime_terrain_load`。修复前
`audio-reload-red.log` 在 load 返回后报 **(active=16,pending=0,retiring=0,render_owned=16)**，
违反立即退休契约。准确历史引入提交未单独二分；本轮集成起点受影响。

**根因修复**（`7db7bbf8`）：接受 load 后立即走现有 `SpatialFrame::clear_sprinklers`；持久化 owner
提供独立 `allows_sprinkler_audio()`，Loading / PublishedAwaitingDependents / 致命冻结状态禁止
重新准入，但不阻断完成水发布所需的更新。清理/重试仍由原有有界声音 owner 负责，没有第二时钟。
纯 policy 回归覆盖成功等待、可恢复失败、突变后失败；原生回归覆盖真实 active → load 的调用模式。

最终 `0208ce46` 增强断言，`exact-audio-reload-{garden-final,minimal-empty}.log` 分别确认：

1. load 前 active=16；同步调用不推进 world tick/fraction；已确认水暂停，返回时 **Water=Quiesced**。
2. load 后 active/pending=0；session 来源的 **ID、位置、animation phase 逐项精确保留**。
   加载空世界不会擅自删除这些非地形 session 喷头，也不会按新地形自动重放置它们。
3. 正常推进依赖期间没有重新准入；水 **Running**、terrain **Ready** 后重新锚定到本帧权威快照。
4. 再加载故意不存在的 task-owned 路径，触发实际 preflight / mutated=false 失败；立即退休，世界
   仍可更新/重试，随后恢复 16 个声音。最后正常移除，四类计数全为零、没有超出 16 总槽。
5. 保留 20 Hz、0/100% duty、0 RPM / 光学独立、暂停/恢复与移除的既有检查。

这两份日志各有 **恰好一个预期 ERROR**（intentionally-missing 文件）；不谎称它们零 ERROR。
无 VUID、SYNC-HAZARD、panic，均 shutdown failures=0。静音 native 检查的是实际声音/引擎所有权，
不是人类听到的“无尾音”证明；离线生产包络/DSP、设备延迟边界沿用声音切片记录。

### 1.2 全局时钟暂停后湿土仍变化（验收中新发现）

**原生红证据**：`wet-pause-red.log` 用真实 Water brush 建立湿土；暂停后先等待 9 帧，再在
frame250/269 读回实际 soil。world tick=110、fraction=0.88740766 相同，soil sum 却 **3088→3016**。
这是土壤状态继续变化，不是材质的第二计时器或截图噪声。

**根因**：spread/dry 以及 sprinkler dose 的提交仍按渲染帧运行，没有消费同一个 world-running
事实。可能原因排序为帧级 moisture 调度、未清空的在途工作、独立材质计时；等待后变化及入口
检查确认第一项，材质本来只读 atlas。准确历史引入提交未追溯。

**修复**（`6f147472`）：spread/dry 在 `world_updates_running` 下提交；喷头暂停时仍调用既有
stage 入口把 amount 置零，避免复用上帧剂量。不改运行时的传播/干燥概率、预算、SprayClock 或
wet-material 函数，无新 footprint/timer。该事实同时控制真实 WorldClock 的 advance。

`wet-pause-green.log`：250/269 两次 soil **count=18743,sum=3177**、tick=110、
fraction=0.89031035 **完全相等**；295 恢复后 tick=119、soil sum=3055。暂停窗口已让在途 GPU 工作
settle；变化来自正常恢复的传播/干燥，不假称局部湿度总和守恒。正常运行事实恒为 true，性能
矩阵的水、画质、剂量语义未改变；这是测量后的正确性门控修复，不是第三项性能优化。

### 1.3 现有混音控件与支撑/边界

同一 audio review 现在真实修改并通过 `set_mix` 应用既有内存混音，不触发保存，也始终保留 CLI
全局静音：Terrain 禁用（frame205）与 master 禁用（278）均 active/pending/retiring/render_owned=0；
启用后（210/295）active=16。引擎总数从 21 降为 5 再回 21，另外 5 条非喷头声音不误计为 ghost。

`c2bd38a9` 给现有 grass review 增加 opt-in `support` 场景，普通快速测试不打开设备：

- Water 后真实 Dig 移除 **300** 个 dirt；采样由 count=4293,sum=5895 变为 count=3993,sum=4995，
  证明移除了实际湿支撑，而非只调用一个无效果接口。
- 世界边缘 x=0.002 的 brush 跨越边界，真实 Grow/Water/Dig 路径完成，移除 **309** 个 dirt。
- invalid column `u32::MAX` 经生产 resolver 拒绝；后续继续原生渲染和正常退出，无 atlas wrap/VUID。

证据 `support-native.log`。这覆盖代表性删除、越界编辑和无效入口，不是穷举所有材质/所有世界
边界；无效/nonsoil shader 解码与无效 atlas 地址的穷举继续由 50 项 Slang 中的草材质 contract
保证。未为截图保留失效草实例或另建湿区。

## 2. 精确匹配的 CURRENT-feature 对照构建

### 构建身份及源差异

测量代码为 `57a39c7f`（已含运行时声音修复、全部新世界/湿材质/声音/空发布功能）。
在**本工作树干净、串行的临时源码实验**中只恢复 `a7b7243c` 的以下完整实现：

- `shader/slang/sprinkler_mist_prepare.comp.slang`：64 lanes；
- `shader/slang/sprinkler_wetting.comp.slang`：每 receiver 扫描全部 contact。

其余源、grounded harness、功能、ABI 与控制完全相同。先保存 optimized binary，再替换上述
两文件，`cargo build --release --features no_validation_layer`，保存 diagnostic binary；EXIT trap
恢复优化源码。`matched/diagnostic.patch` 是完整源码 diff，**只有这两文件**。没有 cherry-pick
旧 renderer、回退新声音或删除植物。此 before **不是原始历史 executable**，历史
`target/garden-delivery/performance/baseline` 未覆盖、未用于本表。

| 证据二进制 | SHA256 |
| --- | --- |
| `matched/current-feature-diagnostic` | `acea1f8d573d18f17fffae098cfde9cc9447544f0a3e2158293e114914f3b426` |
| `matched/optimized` | `9ca2f51776ab6a5fca0f8a8141d8ad927192d7c37242556101aed0bfcb0c2291` |

两文件的源码副本、hash、完整 diff、构建日志均在 `matched/`。
进一步对实际 build cache 的 reflection SPIR-V 做了 `spirv-dis`：prepare 分别反射
**LocalSize 64 1 1 / 256 1 1**，wetting 两边均 **64 1 1**；所有 descriptor set/binding、数组 stride、
struct member offset 比较一致（prepare 的 ABI 抽取 diff 唯一差异就是 LocalSize）。保留
`*-abi.txt` / `*.spvasm`，不是只看注释假设。

CPU `Tracer::record_sprinkler_mist_prepare` 两边都调用
`record_workgroups_with_descriptors(... Extent3D::new(push.count,1,1) ...)`；该 API 直接提交组数，
不按 local-size 再除一次，因此 64/256 均恰好每实例一组。wetting 仍使用同一反射 invocation
路径。warmup frame180 对**所有实例**读回 count×256 lobes，逐项 finite 且每实例有 live 源；
并确认实际 active audio=min(count,16)。0 人口不做不存在 buffer 的 readback，不把未执行的
scope 当作零成本 dispatch。128 optical droplets/接触、四种硬件及全部姿态的更广语义由
Slang / spray / designs 合约联合保证，不把 lobe 检查冒充逐滴视觉审批。

### 场景、质量及采样

- 同一 task-owned realized snapshot：`world/implementation/garden-final.rflterrain`，
  SHA256 **dc9e980eaaacef3eedcaa934f8c7ddd636a0f309f4c0f2c3277c503e9ed51211**；
  日志确认恢复 **130190 flora（180 authored）、4 trees**，无 trunk restamp。每次重新加载同一字节，
  不重新生成“类似花园”，不访问用户世界文件。
- RTX 3060 Ti（既有主机 i5-12600KF，driver 580.159.04），真实 native borderless screen
  **4096×2560 / scene 820×512**，saved stride=5、point/nearest；每份性能日志都断言该尺寸。
  自动 **FIFO**。不是旧 1280×720 / scene256×144 平地场景。
- `garden-arrival` 相机 position=(0.72,0.5682811,1.46)，yaw=25.201117°、pitch=-8°、FOV=60°；
  日光 time_of_day=0.43，固定太阳，不暂停真实世界/喷水时钟。
- 使用保存的 30°、5 RPM、5 Hz、30% duty、mist intensity=6、droplets=true、wetting=3、patch=2、
  drying=0.05；硬件 cache/分辨率/global views/16 poses、全部光学和其它画质均保留保存值。
  只在 continuous 变体把 duty 改为100；音频引擎与逐源生命周期保持运行，仅全局输出静音。
- **固定4×4前缀**，不是随人口重排：x=0.62+(i%4)×0.10，z=1.30-floor(i/4)×0.10；每个 source
  经自身 column 的生产支撑 resolver 放置，实际 Y 范围 **0.49218738–0.5234374**，不是统一平地高度。
  overlap 用0.0001间隔，量化后共点是刻意最大重叠压力，仍有独立稳定 ID/phase。`GARDEN_PERF_SOURCE`
  留完整位置和 phase；`analyze.mjs` 逐字确认所有 before/after/重复/duty 的同布局同人口来源一致。
  4 喷头最终图能实际看到近处3台与真实喷雾，远/侧源不靠全部离屏来减成本。
- 没有 --flat-terrain、--no-flora、--no-particles、零水量或质量覆盖；夹具不 stamp terrain、不追加花草。
  真实 wetting/spread/dry 照常运行。每次启动的相机、保存 GUI、snapshot hash 都核对。
- **66 次**：主矩阵 5人口×2版本×3轮=30；spread100/overlap30/overlap100 各4/16，共36压力运行。
  每轮每case交错 before/after；第二轮反转 after/before。每次14秒正常场景，首次 profiler 后至少5秒，
  且实际 GPU readback marker 后再1秒，才进入统计；因此慢的16人口也在 readback 之后取样。
  每帧记录 CPU/GPU，而非每30帧；每次 GPU 样本 **193–526**，无 dropped scopes。
  没有截图、readback 或编译进入取样区。
- 下表 ms 均为**每轮 p50/p90 的三轮算术均值**，不是 pooled 总分布；逐轮样本数和整帧数值也列出。
  `sprinkler.sum` 先在同一帧相加 prepare+wetting+tiles+mist+hardware，再求分位数，**不是分位数求和**。
  其它嵌套 GPU scope 不重复加到它里面。
- FPS 来自暖机后 CPU 日志首尾 wall-time / frame-number，代表实际提交节奏，非 `1000/GPU_ms`
  推导的理论 FPS；毫秒日志精度及隐藏窗口环境仍有限制。CPU total 包含 acquire/present 等待，
  tracked/untracked 是既有 profiler 分类，不能把 acquire 误作 CPU 算法热点。

复现（两份匹配二进制已在证据中；必须先完成构建再运行）：

```sh
flock -o /tmp/re-flora-garden-delivery-heavy.lock node scripts/benchmark-garden.mjs \
  --before target/garden-delivery/final_acceptance/matched/current-feature-diagnostic \
  --after target/garden-delivery/final_acceptance/matched/optimized \
  --output target/garden-delivery/final_acceptance/repeat-new --seconds 14 --repeats 3 --matrix all
node target/garden-delivery/final_acceptance/analyze.mjs
```

第二条分析脚本使用本次固定 measurement 目录；重复测量应复制/调整其输入目录，不覆盖本次 raw logs。
新 CLI `--help` 与非法 matrix 的退出码2已实际检查，保留 help/error 输出。

## 3. 完整性能表

`before`=上述 current-feature diagnostic；`after`=优化版。GPU/CPU 单位都是 ms，
每格 p50 / p90。0 人口 sprinkler.sum 是分析定义的空和，**不是执行了零耗时喷头 pass**。

### 30% spread 主矩阵 GPU

| 布局/duty/N | 版本 | frame | prepare | wetting | mist optical | hardware | sprinkler逐帧和 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| spread/30/0 | before | 8.702 / 8.930 | — | — | — | — | 0.000 / 0.000 |
| spread/30/0 | after | 8.301 / 8.529 | — | — | — | — | 0.000 / 0.000 |
| spread/30/1 | before | 10.014 / 10.645 | 0.769 / 0.826 | 0.184 / 0.625 | 0.394 / 1.470 | 0.059 / 0.061 | 1.622 / 2.518 |
| spread/30/1 | after | 9.269 / 9.669 | 0.241 / 0.255 | 0.038 / 0.068 | 0.389 / 1.472 | 0.058 / 0.060 | 0.740 / 1.819 |
| spread/30/4 | before | 13.939 / 15.833 | 0.758 / 0.793 | 0.755 / 1.616 | 4.257 / 5.563 | 0.182 / 0.185 | 5.981 / 7.448 |
| spread/30/4 | after | 12.440 / 14.281 | 0.243 / 0.252 | 0.161 / 0.228 | 4.232 / 5.547 | 0.183 / 0.185 | 4.846 / 6.176 |
| spread/30/8 | before | 19.158 / 22.182 | 0.731 / 0.766 | 1.727 / 2.972 | 7.451 / 10.579 | 0.347 / 0.349 | 10.586 / 13.568 |
| spread/30/8 | after | 16.606 / 20.041 | 0.238 / 0.250 | 0.324 / 0.419 | 7.672 / 10.654 | 0.346 / 0.349 | 8.612 / 11.617 |
| spread/30/16 | before | 26.642 / 30.008 | 0.710 / 0.740 | 4.006 / 5.559 | 12.783 / 15.984 | 0.677 / 0.682 | 18.286 / 21.533 |
| spread/30/16 | after | 22.734 / 26.421 | 0.231 / 0.245 | 0.670 / 0.858 | 12.430 / 15.916 | 0.677 / 0.682 | 14.027 / 17.574 |

### 连续 / 重叠压力矩阵 GPU

| 布局/duty/N | 版本 | frame | prepare | wetting | mist optical | hardware | sprinkler逐帧和 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| spread/100/4 | before | 15.178 / 17.386 | 0.855 / 0.889 | 0.864 / 1.856 | 5.273 / 6.765 | 0.183 / 0.185 | 7.274 / 8.975 |
| spread/100/4 | after | 13.871 / 15.936 | 0.284 / 0.295 | 0.203 / 0.300 | 5.319 / 6.867 | 0.183 / 0.186 | 5.999 / 7.539 |
| spread/100/16 | before | 31.933 / 35.529 | 0.802 / 0.832 | 5.102 / 6.930 | 16.333 / 19.785 | 0.677 / 0.685 | 23.120 / 26.589 |
| spread/100/16 | after | 25.763 / 30.139 | 0.273 / 0.287 | 0.871 / 1.105 | 15.614 / 19.731 | 0.676 / 0.684 | 17.456 / 21.582 |
| overlap/30/4 | before | 13.032 / 13.948 | 0.753 / 0.794 | 0.790 / 1.675 | 2.703 / 3.686 | 0.180 / 0.182 | 4.855 / 5.772 |
| overlap/30/4 | after | 11.492 / 12.263 | 0.238 / 0.249 | 0.158 / 0.229 | 2.723 / 3.697 | 0.179 / 0.182 | 3.374 / 4.304 |
| overlap/30/16 | before | 26.430 / 29.661 | 0.699 / 0.730 | 4.105 / 5.606 | 12.392 / 15.889 | 0.479 / 0.514 | 18.069 / 21.089 |
| overlap/30/16 | after | 22.142 / 25.848 | 0.226 / 0.236 | 0.671 / 0.820 | 12.319 / 15.877 | 0.475 / 0.513 | 13.740 / 17.320 |
| overlap/100/4 | before | 13.893 / 14.899 | 0.869 / 0.913 | 1.149 / 2.130 | 3.415 / 4.663 | 0.179 / 0.181 | 5.992 / 7.117 |
| overlap/100/4 | after | 12.158 / 13.021 | 0.290 / 0.302 | 0.208 / 0.300 | 3.435 / 4.679 | 0.180 / 0.183 | 4.186 / 5.385 |
| overlap/100/16 | before | 31.271 / 34.926 | 0.800 / 0.836 | 5.320 / 7.267 | 15.694 / 19.740 | 0.477 / 0.512 | 22.578 / 26.273 |
| overlap/100/16 | after | 25.083 / 30.020 | 0.269 / 0.282 | 0.887 / 1.086 | 15.084 / 19.826 | 0.477 / 0.514 | 16.862 / 21.513 |

### CPU 与真实帧间隔

| 布局/duty/N | 版本 | CPU total | tracked | untracked | acquire | record | 平均间隔 ms / FPS |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| spread/30/0 | before | 16.724 / 17.686 | 0.057 / 0.071 | 0.632 / 0.678 | 14.932 / 15.936 | 0.678 / 0.800 | 17.118 / 58.42 |
| spread/30/0 | after | 16.715 / 17.678 | 0.057 / 0.070 | 0.628 / 0.668 | 14.935 / 15.938 | 0.679 / 0.774 | 17.116 / 58.42 |
| spread/30/1 | before | 16.739 / 17.701 | 0.057 / 0.072 | 0.634 / 0.678 | 14.936 / 15.934 | 0.703 / 0.794 | 17.120 / 58.41 |
| spread/30/1 | after | 16.722 / 17.696 | 0.057 / 0.070 | 0.633 / 0.668 | 14.933 / 15.938 | 0.699 / 0.786 | 17.095 / 58.50 |
| spread/30/4 | before | 16.759 / 17.720 | 0.058 / 0.070 | 0.635 / 0.682 | 14.936 / 15.934 | 0.712 / 0.797 | 17.116 / 58.42 |
| spread/30/4 | after | 16.734 / 17.711 | 0.057 / 0.071 | 0.634 / 0.678 | 14.927 / 15.936 | 0.707 / 0.833 | 17.109 / 58.45 |
| spread/30/8 | before | 20.160 / 23.286 | 0.068 / 0.090 | 0.651 / 0.748 | 17.742 / 21.112 | 0.787 / 1.039 | 20.171 / 49.58 |
| spread/30/8 | after | 17.812 / 21.085 | 0.059 / 0.080 | 0.643 / 0.687 | 15.885 / 19.029 | 0.735 / 0.949 | 18.566 / 53.87 |
| spread/30/16 | before | 27.913 / 31.196 | 0.087 / 0.122 | 0.679 / 1.127 | 25.040 / 28.724 | 0.996 / 1.247 | 27.967 / 35.76 |
| spread/30/16 | after | 23.761 / 27.545 | 0.069 / 0.095 | 0.658 / 0.885 | 21.461 / 25.289 | 0.850 / 1.089 | 23.960 / 41.74 |
| spread/100/4 | before | 16.951 / 18.346 | 0.058 / 0.077 | 0.634 / 0.678 | 15.001 / 16.426 | 0.718 / 0.902 | 17.328 / 57.71 |
| spread/100/4 | after | 16.767 / 17.715 | 0.056 / 0.074 | 0.636 / 0.751 | 14.928 / 15.937 | 0.708 / 0.887 | 17.134 / 58.36 |
| spread/100/16 | before | 33.277 / 36.940 | 0.097 / 0.140 | 0.749 / 1.412 | 30.030 / 33.868 | 1.066 / 1.380 | 33.362 / 29.98 |
| spread/100/16 | after | 26.965 / 31.362 | 0.078 / 0.109 | 0.669 / 1.060 | 24.134 / 28.732 | 0.944 / 1.143 | 26.955 / 37.10 |
| overlap/30/4 | before | 16.727 / 17.707 | 0.057 / 0.068 | 0.637 / 0.692 | 14.918 / 15.922 | 0.713 / 0.844 | 17.117 / 58.42 |
| overlap/30/4 | after | 16.749 / 17.708 | 0.058 / 0.072 | 0.638 / 0.680 | 14.934 / 15.934 | 0.707 / 0.874 | 17.121 / 58.41 |
| overlap/30/16 | before | 27.585 / 30.947 | 0.082 / 0.125 | 0.679 / 1.235 | 25.017 / 28.393 | 0.953 / 1.275 | 27.954 / 35.78 |
| overlap/30/16 | after | 23.180 / 27.007 | 0.074 / 0.101 | 0.678 / 0.727 | 20.970 / 24.697 | 0.882 / 1.057 | 23.762 / 42.09 |
| overlap/100/4 | before | 16.742 / 17.708 | 0.058 / 0.071 | 0.634 / 0.670 | 14.930 / 15.931 | 0.712 / 0.802 | 17.084 / 58.53 |
| overlap/100/4 | after | 16.729 / 17.716 | 0.057 / 0.071 | 0.636 / 0.681 | 14.930 / 15.934 | 0.707 / 0.825 | 17.113 / 58.44 |
| overlap/100/16 | before | 32.620 / 36.332 | 0.095 / 0.122 | 0.727 / 1.031 | 29.655 / 33.701 | 1.084 / 1.310 | 32.877 / 30.42 |
| overlap/100/16 | after | 26.131 / 31.360 | 0.082 / 0.114 | 0.674 / 1.005 | 23.695 / 28.718 | 0.954 / 1.195 | 27.000 / 37.04 |

### 0 人口边际（同版本三轮 frame p50 均值相减，非独立 scope）

| N | before Δ ms | after Δ ms | before→after 整帧 ms / 改善 |
| ---: | ---: | ---: | ---: |
| 0 | 0.000 | 0.000 | 8.702 → 8.301 / 4.6% |
| 1 | 1.312 | 0.968 | 10.014 → 9.269 / 7.4% |
| 4 | 5.237 | 4.139 | 13.939 → 12.440 / 10.8% |
| 8 | 10.457 | 8.305 | 19.158 → 16.606 / 13.3% |
| 16 | 17.940 | 14.432 | 26.642 → 22.734 / 14.7% |

### 各轮原始样本数、整帧分位数

每格顺序 r0 / r1 / r2；GPU frame 为每轮 p50/p90（ms），不是 pooled 分布。

| 布局/duty/N/版本 | CPU samples | GPU samples | GPU frame ms | 实际平均 FPS |
| --- | --- | --- | --- | --- |
| spread/30/0/before | 522 / 524 / 523 | 522 / 523 / 523 | 8.694,8.925 / 8.655,8.887 / 8.756,8.979 | 58.39 / 58.46 / 58.41 |
| spread/30/0/after | 524 / 525 / 525 | 523 / 525 / 525 | 8.737,8.976 / 8.079,8.308 / 8.088,8.304 | 58.42 / 58.43 / 58.42 |
| spread/30/1/before | 522 / 523 / 522 | 522 / 523 / 522 | 10.245,10.904 / 10.145,10.757 / 9.651,10.274 | 58.43 / 58.47 / 58.34 |
| spread/30/1/after | 525 / 526 / 527 | 525 / 525 / 526 | 9.479,9.874 / 9.450,9.854 / 8.878,9.278 | 58.44 / 58.48 / 58.57 |
| spread/30/4/before | 522 / 520 / 521 | 521 / 520 / 521 | 14.160,15.991 / 14.110,16.086 / 13.547,15.423 | 58.50 / 58.35 / 58.42 |
| spread/30/4/after | 523 / 524 / 524 | 522 / 523 / 524 | 12.845,14.741 / 12.236,14.078 / 12.239,14.023 | 58.37 / 58.46 / 58.51 |
| spread/30/8/before | 437 / 437 / 448 | 436 / 437 / 448 | 19.253,22.442 / 19.374,22.366 / 18.848,21.738 | 49.16 / 49.18 / 50.41 |
| spread/30/8/after | 472 / 484 / 485 | 472 / 484 / 485 | 17.216,20.409 / 16.286,19.872 / 16.316,19.843 | 52.90 / 54.29 / 54.42 |
| spread/30/16/before | 300 / 302 / 301 | 300 / 302 / 301 | 26.634,30.045 / 26.486,29.972 / 26.806,30.006 | 35.74 / 35.78 / 35.76 |
| spread/30/16/after | 365 / 366 / 376 | 365 / 365 / 376 | 23.136,26.574 / 22.850,26.575 / 22.215,26.115 | 41.32 / 41.41 / 42.50 |
| spread/100/4/before | 513 / 513 / 517 | 512 / 513 / 517 | 15.350,17.522 / 15.321,17.573 / 14.864,17.063 | 57.54 / 57.53 / 58.07 |
| spread/100/4/after | 522 / 524 / 522 | 522 / 523 / 522 | 14.069,16.168 / 14.103,16.187 / 13.440,15.453 | 58.30 / 58.46 / 58.32 |
| spread/100/16/before | 229 / 230 / 236 | 229 / 230 / 236 | 32.074,35.814 / 32.027,35.563 / 31.698,35.210 | 29.79 / 29.86 / 30.28 |
| spread/100/16/after | 326 / 326 / 326 | 326 / 326 / 326 | 25.705,30.178 / 25.778,29.998 / 25.805,30.242 | 37.09 / 37.12 / 37.09 |
| overlap/30/4/before | 521 / 521 / 521 | 521 / 520 / 521 | 13.266,14.258 / 13.212,14.070 / 12.619,13.515 | 58.41 / 58.42 / 58.44 |
| overlap/30/4/after | 523 / 524 / 524 | 523 / 523 / 524 | 11.283,12.049 / 11.889,12.646 / 11.305,12.093 | 58.36 / 58.42 / 58.44 |
| overlap/30/16/before | 254 / 263 / 263 | 254 / 263 / 263 | 26.824,29.987 / 26.273,29.610 / 26.194,29.387 | 35.30 / 35.98 / 36.04 |
| overlap/30/16/after | 322 / 336 / 337 | 322 / 336 / 337 | 22.555,26.581 / 21.951,25.443 / 21.921,25.520 | 41.14 / 42.57 / 42.57 |
| overlap/100/4/before | 522 / 523 / 521 | 521 / 523 / 521 | 13.874,14.887 / 13.922,14.953 / 13.883,14.858 | 58.49 / 58.63 / 58.48 |
| overlap/100/4/after | 524 / 524 / 524 | 523 / 523 / 523 | 12.148,13.016 / 12.132,13.012 / 12.195,13.035 | 58.43 / 58.44 / 58.44 |
| overlap/100/16/before | 193 / 201 / 200 | 193 / 201 / 200 | 31.761,35.193 / 30.967,34.905 / 31.084,34.680 | 30.01 / 30.66 / 30.59 |
| overlap/100/16/after | 270 / 269 / 268 | 270 / 269 / 268 | 25.166,29.926 / 25.029,30.057 / 25.054,30.077 | 37.11 / 36.97 / 37.03 |

### 收益、偏移及剩余瓶颈解释

1. 30% duty 的 prepare 从约0.71–0.77ms降为0.23–0.24ms；16个 wetting 从4.006降为0.670ms，
   正是保留优化的目标。连续16个 wetting 从5.102降为0.871ms。没有把工作搬到 CPU 或下一帧。
2. **不能把0人口的4.6%表观下降归功于喷头优化**：那里没有喷头 dispatch。未修改的草 raster
   在0人口 before/after 三轮均值为1.936/1.540ms；单轮有约1.34/1.94ms两档，与旧世界切片观测
   相同。可能与 GPU 动态状态/未固定的植被响应有关，未用硬件计数器定位。故同时给0人口边际，
   不把整帧百分比当精确因果估计。主矩阵1/4/8/16的边际下降分别0.344/1.098/2.152/3.508ms。
3. **成本偏移不隐藏**：8人口 optical p50 7.451→7.672ms（+0.221）；连续4个5.273→5.319ms；
   overlap4也略涨。16人口未改草 raster 反而1.335→1.745ms（+0.410），部分抵消喷头 scope 和
   的收益。相反有些case未改pass变快。没有声称阶段改善可简单相加为整帧收益或解释所有噪声。
4. 首要剩余热点：主矩阵16个 **mist optical 12.430ms**（p90 15.916），continuous16约15.614ms；
   prepare已不足0.3ms，wetting约0.67–0.89ms，hardware spread16约0.68ms，tiles约0.009ms。
   应先研究精确有限支持/活跃候选压缩，而非再次猜标定表、减水量/滴数或压缩8字节flora stride。
5. 非喷头固定成本仍约8.3ms，包含花园草、花、树、阴影与照明；例如 shadow prepass约2.3ms、
   grass raster约1.3–1.9ms、flora lighting cache约0.85ms。嵌套pass不能重复加总。
   CPU record主矩阵最终16约0.85ms、acquire约21.46ms；证据不支持把FIFO/acquire等待当作
   CPU提交算法瓶颈。音频reconciliation的独立旧切片微秒数据不是本表的全CPU成本替代品。
6. 真实重花园终于复现8/16人口掉帧；两项优化有帮助但没有消除症状。此处不是任意相机、任意
   世界、256喷头或所有GPU的预算保证。保持候选视觉供玩家后续审阅，不以未达60FPS遮掩结果，
   也不因此未经请求砍掉效果。

## 4. 最终视觉与真实湿草证据

证据根目录均为 `target/garden-delivery/final_acceptance/`，图片在 `images/`。
本轮实际打开检查了以下原生截图的1280/1600宽 review 派生图；原PNG保留，派生图仅缩放/拼接，
不改生产材质、曝光、相机或保存值，不是概念图：

- `fresh-arrival.png` / `loaded-arrival.png`（原4096×2560）：同一到达视角的开阔通道、两侧草花、
  成熟树轮廓一致；载入没有重复stamp树/再生草花。截图看起来一致不是bit-identical或精确存档
  校验的替代；精确存档证据沿用empty/world切片及本轮实际load路径。
- `garden-working-sprinklers.png`（原4096×2560）：在真实载入花园中4个接地喷头，近处3台清楚可见，
  有方向性水雾和独立亮滴；道路/花草未换成测试平地，所有既有特效保持开启。
- `sprinkler-response-{0,1}/near-both-b.png`：真实喷头浇水、同材质/草/相机的关闭与开启对照；
  `sprinkler-material-comparison.png` 为相同矩形crop，**左off、右on**。两边frame120真实 soil
  **count=17728,sum=3724**，frame180 **sum=4613**，邻居 **count=2259,sum=0** 完全相同。
  喷雾带来的泛白不可解释成湿材质；本图中的草暗化较温和，受当前高曝光黄绿配色与雾遮挡影响。
  不把微小跨进程DDGI/dither差异全部归因材质。原切片manual-wet/cached/native的更强对照继续有效，
  未为填表重跑所有视觉组合。

湿草仍是 **根部土壤atlas的四档湿度代理**，不是叶面储水、反射或独立湿区；没有增加第二个水分
时钟。观察到候选暗化/实际水源与玩家认可音色、美术是不同验收。没有可见启动或播放WAV。

## 5. 聚合验证及最终二进制

本轮执行记录（不是拿debug单测计时作性能证据）：

| 检查 | 实际结果 / 日志 |
| --- | --- |
| `cargo fmt --check` / `cargo check` | 通过，`aggregate-fmt.log`、`aggregate-check.log`、`final-fmt.log`；最后fixture加强后再check通过 `final-lifecycle-check.log` |
| `cargo test` | **1668 app / 4 library / 1 integration passed**；11 app、3 clipboard、1 display opt-in ignored，`aggregate-tests.log` |
| `cargo test -p re-flora-vkn --lib` | **66 passed**，`vkn-tests.log`，含workgroup/invocation映射 |
| `cargo test --manifest-path crates/petalsonic/Cargo.toml` | **194 unit + 6 integration + 2 doctest passed**、12 opt-in ignored，`petalsonic-tests.log` |
| `SLANGC=/home/terence/vulkan-sdk/x86_64/bin/slangc python3 scripts/run_slang_tests.py` | **全部50通过**，`slang-tests.log`；编译器既有pragma-once警告保留 |
| 同 pinned SLANGC，`cargo test sprinkler_shader_pose_matches_water_outlets -- --ignored --nocapture` | **660 GPU/CPU pose cases passed**，`pose-tests.log` |
| `cargo test audio::sprinkler_audio::tests -- --ignored --nocapture` | **2个真实引擎所有权检查通过**（固定不存在设备，不打开可听输出），`native-engine-tests.log` |
| default Release `node scripts/validate-sprinkler-model.mjs --spray --seconds 15` | **15 scenes passed**，真实同步验证，`spray-console.log`、`spray/summary.json` 与全部日志/soil JSON/PNG |
| default Release `node scripts/validate-sprinkler-model.mjs --designs --seconds 15` | **5 scenes passed**，四造型、16完整poses、7次live控制切换，`design-console.log`、`designs/` |
| 运行时声音/替换/混音/暂停 | 见第1节；最后加强精确保留断言后的 `exact-audio-reload-*.log` 也通过 |
| 支撑/边界编辑、实际草水分 | `support-native.log`、`wet-pause-green.log`、`sprinkler-response-*.log` 通过 |
| 最终 `cargo build --release` | 默认验证层构建成功，`delivered-build.log`，不是遗留no-validation计时binary |
| 普通fresh花园 `cargo run --release -- --hidden --mute --auto-exit 0.5` | `delivered-smoke.log` / console，camera=fresh-arrival、4树180花、shutdown failures=0 |
| 同工作树 `--latest-log` / `--tail-latest-log 200` | 已实际执行并检查，`latest-log.txt`、`latest-log-tail.txt`；无ERROR/VUID/SYNC-HAZARD/panic |

聚合全测在 `c2bd38a9` 生产源码上执行；之后 `0208ce46` 仅增强opt-in载入fixture的断言，不改
生产路径或快速测试输入，check/build及两份联合native已针对该最后版本重跑；没有伪称重新跑过
未重跑的完整套件。最后普通native日志为
`target/re-flora-logs/re-flora-20261021-165452.312-249197.log`，完整副本已保留。
最终二进制 `target/release/re-flora` SHA256：
**06420d416ea597f4594fe26c2b5b5172dce6ac582f01795b6b1e1c0c6907152f**。

`final-evidence-audit.json` 记录 **94份零错误native日志**（66性能+20同步验证+8最终图/边界/暂停/
smoke）全部正常关闭，以及两份各含1个预期missing-file ERROR的恢复日志；故意red、早期fixture
迭代失败日志明确保留，不计入通过数量。无Vulkan validation hazard被豁免。

## 6. 文件、资源与交接

### 本轮本地提交 / 共享接口

- `7db7bbf8`：`src/app/core/{terrain_persistence,mod,sprinkler_audio_review}.rs`；新增内部音频readiness
  谓词和load接受点退休，正常依赖推进接口不变。
- `57a39c7f`：`scripts/benchmark-garden.mjs`、`src/app/core/{sprinkler_review,mod}.rs`；加载花园的固定
  接地population夹具、每帧性能日志开关，无默认质量/保存变化。
- `6f147472`：`src/app/core/{mod,sprinkler_audio_review}.rs`；同一world-running事实门控土壤提交；
  增加真正Terrain/master混音和湿土暂停断言。
- `c2bd38a9`：`src/app/core/grass_stem_review.rs`；真实支撑/边界编辑opt-in检查。
- `0208ce46`：精确保留session来源及水暂停证明的native断言。
- 本报告独立文档提交。**无 tracked generated diff**，无手改生成源码；所有Rust/shader相关变化
  均经cargo check/build。未修改本地PetalSonic或vkn实现；显式crate测试为集成复核。

### 必须带走的证据（无需复制整个target）

1. `target/garden-delivery/final_acceptance/measurement/`：66份raw log、原始summary.json；
   `analysis.json` / `tables.md` / `analyze.mjs`，含每次CPU/GPU所有scope分位数、样本数与来源匹配。
2. `matched/`：两份二进制、sha256、两个版本源码、diagnostic.patch、构建日志、SPIR-V disassembly
   与ABI抽取。诊断binary作为可复验的**证据**保留，不是当前try-out binary。
3. `spray/` / `designs/`：同步验证日志、原始PNG、JSON及summary；全部聚合/最终native日志与
   `final-evidence-audit.json`。
4. `images/`、图像对应的原生日志；原有四切片图像/报告继续有效，未复制无关build cache。
5. 继续保留输入 `target/garden-delivery/world/implementation/garden-final.rflterrain` 及
   `audit/minimal-empty.rflterrain`；历史 `performance/baseline` 未改。

临时shader源码已恢复，串行诊断没有创建额外worktree或单独target树；临时 `/tmp` 源码/编辑脚本
已删除，没有存活重任务。现有两个feature build缓存复用，不删除未知/先前worker资源。证据二进制
与只读源码副本仍需controller复制审阅，因此不删除。main及其它已有worktree/branch不由本worker
清理；controller最终合入、main重建、聚合报告和task-owned worktree回收仍按原计划。

保存输入前后hash一致（报告中的GUI hash含先前切片已增加的草checkbox，本轮没有GUI diff）：

- `config/gui.toml`：`a1b5acbb9c3d4f7d9e65124039d804fcf319b0785b56a93f0bba8e86452cfd0a`
- `config/camera_snapshots.toml`：`d45b3c3875c2896a7b5c83b19520f8401bf29fc7208063fde5bb95191bf7f29a`

**剩余批准与不确定性**：玩家真实试听/声音美术、花园与湿草视觉审批；不同设备、更多喷头、不同
视角/世界的性能预算；未定位未改草raster两档波动的硬件原因。不存在已知需要掩盖的crash、存档
覆盖或Vulkan错误；仍不能据此声称“无成本”“全部世界60FPS”或已获玩家批准。
