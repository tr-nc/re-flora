# 喷头扩展性能诊断（garden scope v1）

## 交付摘要

保留两项精确优化：wetting 每工作组稳定压缩非零接触索引；prepare 每实例 256 lanes 并行处理
原有全部样本，以明确的逻辑工作组 API 提交，杜绝 shader/CPU local-size 硬编码不一致。
用户预设双次匹配 Release 的 GPU frame p50：1 个 **5.970→5.232 ms**，4 个 **6.306→5.183 ms**，
16 个 **7.625→6.377 ms**；8 个 **6.282→6.260 ms**，没有明显整帧收益。
连续水、重叠 16 个为 **9.200→6.922 ms**。均未降低画质、覆盖、水量、光学粒子或完整硬件姿态。

这是 GPU 工作量优化，不是玩家实际 FPS 修复的最终验收：当前 FIFO 下始终约 17.1 ms 帧间隔，
未复现完整玩家世界的掉帧。完整前后、阶段、CPU、p90、负面实验和验证边界在下文披露。
中间调度实验的欠调度数字全部作废并已修正；引入原始问题的历史提交仍未知。

## 范围与复现

基线：`a66d9c5144b4a687a4e0c3c7cfa5f8ae7b6cc527`，只添加原生夹具与测量脚本后构建。
脚本 `scripts/benchmark-sprinklers.mjs` 驱动生产 source/contact、土壤、整机和光学路径；
不是 CPU 模拟，不属于普通单测。旧 `mist-pop-*` 会覆盖控件、关闭小水滴，且不能代表用户当前预设。
新 `rotary-perf` 的 0/1/4/8/16 实例都经真实放置接口；spread 间距 0.17，overlap 间距 0.0001。
固定空白平地、无植物以隔离喷头增量；不声称等同玩家完整花园。

```sh
# 仅初始基线夹具提交 a7b7243c、尚未优化时执行；当前分支构建的是最终版，不能覆盖 baseline。
flock -o /tmp/re-flora-garden-delivery-heavy.lock cargo build --release --features no_validation_layer
# 在改优化源码前保存（不要覆盖这个可执行文件）
mkdir -p target/garden-delivery/performance
cp --reflink=auto target/release/re-flora target/garden-delivery/performance/baseline
flock -o /tmp/re-flora-garden-delivery-heavy.lock node scripts/benchmark-sprinklers.mjs \
  --binary target/garden-delivery/performance/baseline \
  --output target/garden-delivery/performance/repeat-before --layouts spread --duties 30 --publication legacy
```

测量前必须用 `no_validation_layer` feature；此项目默认 Release **也显式请求验证层**，
仅用 loader 环境变量禁用该层会 `ERROR_LAYER_NOT_PRESENT`，不是有效性能采样。
正确性验证另用默认 feature 与同步验证，不能混入性能表。

环境：NVIDIA GeForce RTX 3060 Ti，原生隐藏窗口 X11 1280×720，scale=1，自动选择 FIFO
（可用 FIFO/IMMEDIATE，不强制 present mode）。固定 camera offset=(0.75,0.55,0.90)，
目标为夹具 base=(0.75, terrain surface Y, 0.95) + (0,3/256,0)。地形使用既有 flat fixture；
实例放置 phase 由既有生产接口确定，日志保留实际夹具与相机。
控件：30° / 5 RPM / 5 Hz / 30% duty / mist intensity 6 / droplets on；连续 100% duty 为受控变体。
不改持久设置、质量或用户存档；脚本记录并核对 GUI/camera SHA256。
每次 12 秒，首个 profiler 样本之后再剔除 3 秒，重复两次；CPU/GPU 同周期采样。
统计为日志采样帧的 p50/p90，不是逐帧完整分布。CPU total 包含 acquire/呈现等待；
真实帧间隔由相邻 GPU 日志时间差除帧号差推导，不能把 CPU 等待解释成计算瓶颈。

## 实测基线与影响

第一组 spread/30% 的两次运行 GPU frame p50：

| 实例 | 第一次 ms | 第二次 ms | prepare p50 ms（第一次） | wetting | mist | hardware |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 3.920 | 4.140 | — | — | — | — |
| 1 | 6.104 | 5.836 | 1.449 | 0.256 | 0.514 | 0.078 |
| 4 | 6.258 | 6.355 | 1.213 | 0.879 | 0.733 | 0.160 |
| 8 | 6.265 | 6.300 | 0.954 | 1.342 | 0.848 | 0.284 |
| 16 | 7.541 | 7.708 | 0.809 | 2.502 | 1.189 | 0.563 |

CPU total p50 约 16.9–17.5 ms，acquire 约 14.9 ms；record 约 0.4–0.7 ms。
FIFO 下帧间隔 p50 约 17.1 ms，没有在这个隔离场景复现肉眼 FPS 下降，
但已复现单个喷头约 2 ms GPU 增量以及 16 个时约 3.6 ms 增量。
因此不能声称原始玩家场景已完全复现或 FPS 症状已解决；实测优化目标是减少真实生产 GPU 工作。
prepare 非线性下降也提示小 population GPU 利用率/频率影响，不能简单用总量除实例数推算。

## 基线后排序的可证伪假设

1. **浇水接触遍历冗余**：16 个时 wetting 约 2.5 ms。若只遍历非零 contact 而保持顺序和完整接触，
   wetting 应下降且相同源/种子的土壤结果不变。
2. **prepare 源标定重复**：单个 prepare 约 1.4 ms。若固定的发射标定/小水滴反求重复计算占主导，
   精确复用应主要降低 prepare，而非把工作移到 CPU 或下一帧。
3. **光学候选扫描**：16 个 mist 约 1.2 ms。独立诊断光学粒子开关或细分调度可区分粒子与连续雾成本，
   而 tile 当前只有约 0.008 ms，不优先改 tile。
4. **整机绘制缓存**：16 个约 0.56 ms，有增长但不是最大项；应保留四种造型、16 整机姿态和全局视角数。
5. **CPU 提交/同步**：record 无明显人口增长，FIFO acquire 主导 total；目前证据不支持 CPU 计算是主瓶颈。

## 证据保留

`target/garden-delivery/performance/` 保存 baseline 可执行文件、SHA256、before/ 与 before-variants/
原始 stdout/stderr 日志、summary.json。controller 清理工作树前需复制这些证据（无需整个 target cache）。
引入性能问题的提交未知；没有逐提交复测证据，不能把当前基线或历史覆盖修复称为引入点。

本报告随保留优化及最终验证补充；单测耗时不作为性能证据。

## 优化一：在浇水消费边界稳定压缩非零接触

确认机制：`sprinkler_wetting.comp.slang` 对包围盒内每个 soil receiver 遍历全部 256 个年龄槽。
但首次到达只占少数槽（平地每个速度带一个）；无水槽的 density 必为零。
现在每个 64-lane 工作组只检查一次全部槽，把非零索引在 shared memory 中按原顺序压缩；
所有 receiver 只遍历这个精确子序列。**不是减少 source 样本**，不改变 256 个 airborne/contact
发布槽，不降低覆盖/剂量，不延迟一帧，也不增加第二个湿区。

正确性理由：密度函数对 `center_flux.w<=0` 精确返回零；压缩仅删除这些零项，
保留每个非零 contact 的原索引、浮点累加顺序、terrain sweep 与 receiver 唯一写入；
实例仍依次 dispatch，重叠 R8 atlas 不发生并发更新。所有线程在 uniform early return 后
共同参加两次 group barrier；shared list 256 uint + count，新增约 1 KiB/workgroup。
代价是每组初始化/串行稳定 scan，但摊销掉每个 receiver 的 256-slot 扫描。
暂停和运行时控制仍每帧来自本帧 contact，四种硬件与缓存 ABI 无改动。

## 已否决实验：精确复用源标定的 dyadic 节点

第二项是待证伪的计算重复假设，不是已确认根因。代码检查发现：每个 optical slot 的面积分布反求做八次二分，
每次调用六步 Newton 地面射程标定；每个年龄槽又重复求相同带边界权重。
这些是源的参考平地标定，不依赖当前地形、时间、风或接触结果。正常用户 30% duty 下，
原版 prepare 的粒子 on/off 差约单个 0.47 ms、16 个 0.24 ms；这只是 matched 开关的
**边际 GPU 时间**，不声称是可加的独立 kernel 时间（GPU 占用和频率会互相影响）。

实验版 prepare 每组协作算一次 `referenceRangeSquared(n/256), n=0..256`，再复用到全部年龄槽、
128 个独立粒子槽的二分查询。原算法恰好只访问这些二进制分数；**没有插值、没有降低二分精度、
没有近似射程或新分布**。每个粒子的 seed、出生年龄、速度分散、阀门和真实地形 sweep 完全不变。
八个带边界也是该表中的精确节点，源出水归一化保持原公式。

`sprinkler_source_calibration.slang` 在 16 个 phase、128 optical slots、0/30/100% duty 和全部
256 年龄槽 × 64 个带内位置上比较完整 source 的每个字段，**268,288 案例逐字段精确相等**；
原有 droplets/advection、contact/terraces/radial/continuity/valve 测试仍运行。
但两次匹配 Release 中 prepare 的 p50 与仅压缩 wetting 的版本几乎相同：
单个约 1.45 ms、4 个约 1.33 ms、16 个约 0.92 ms。理论重复并非当前主导成本，
可能已被编译器优化，或被地形追踪/光照/低并行度完全掩盖；未用更细硬件计数器证明其具体原因。
**因此撤回整个表/可选 source 参数/测试，不增加约 1 KiB shared memory 和 barrier。**
保留实验补丁、CPU 测试及 before/after 原始数据在 target 证据目录，而非生产代码。
不能用这次单测正确性代替性能收益。

## 正确性验证的环境问题（与 GPU 性能分开）

首次 Wayland 验证中，系统主题 portal 在 100 ms 内未响应，产生
`sctk_adwaita::config ... color-scheme timeout` ERROR；应用仍正常退出且无 Vulkan 错误，
但脚本严格拒绝 ERROR，故该次不记为通过。改为仅在测试进程移除 WAYLAND_DISPLAY/SOCKET，
使用同一 X11 native surface 重跑，未改用户桌面/首选项或游戏逻辑。
完整 Rust 在优化一后通过：1655 app + 4 collider + 1 integration，8 个 app ignore；
49 项 Slang CPU 与 15 项同步验证原生 spray/soil/墙/阶梯/粒子/resize/放置移除通过。
否决的标定表实验也曾通过 50 项 Slang CPU（含新增源复用回归），但未保留。
最终汇总记录见后续表与日志。

## 优化二（保留）：让同一实例的独立样本并行执行

标定表被否决后，检查生产调度发现：原 prepare 每个实例只 dispatch 一个 **64-thread group**，
每 lane 连续处理 6 个样本（256 continuous + 128 optical）。单/少数喷头时大量 SM 空闲，
每组只有两个 warp 承担长轨迹 sweep、光照及全部槽；“少数实例却高固定成本”与基线吻合。
改为每实例 **256 lanes**，同一原始 index 仍恰好计算一次，lane 最多处理两槽。
shared bounds 从 3 KiB 增至 12 KiB，归约从六轮增至八轮；这是吞吐/延迟与共享内存占用的权衡，
不是减少样本/碰撞步数/阴影/粒子，也没有分帧摊销或 stale water。
原有其它 shader 已使用 256-thread groups，没有引入高于项目既有的工作组大小需求。

首次调度实验暴露了验证缺口：CPU 仍按 `count * 64` 传入 invocation extent，
256-lane shader 只生成 ceil(count/4) 个组，导致多实例欠调度。该阶段全部多实例性能数字
**作废**，原始证据目录标为 `invalid-underdispatch-*`，不得引用为收益。
本地中间提交 `e5bebdff` 包含此问题；交付必须包含其后的工作组 API 修正，不能单独挑取该提交。
现在 `ComputePipeline::record_workgroups_with_descriptors` 明确接收逻辑组数，prepare 直接提交 count 个组，
不再让调用方复制 shader local size。原 invocation API 继续保留，并用整数 div_ceil 替代 float ceiling。
原生夹具在 warmup 第 30 帧读回 **全部实例** 的 256 个 lobe，逐实例检查有限值和非零活跃源；
不仅核对 CPU instance count，也不只读第一个实例。该同步检查完全在暖机排除区间内。
最终正确调度的重复结果见后表。
归约只做有限 AABB 的 min/max，换归约树不改变源、contact、标量流量、第一触地/墙体或射程；
所有 buffer 仍由原 acquired frame slot 拥有，disabled optical 槽也照常清零。
Rust bounded-work 回归覆盖 enabled/disabled droplets 与第二轮的部分 lane，确保全部槽无漏算/重复。
没有新增 emitter registry、CPU 粒子、wet footprint 或 clock API。

环境补充：驱动 580.159.04；真实 scene pixels **256×144**（screen 1280×720，原保存值 pixel_stride=5、
point/nearest，UI native），不是全分辨率 1280×720 shading。整机 cache 32px、全局 128 views、16 poses，
共享 96 MiB。未降低原保存质量。平地高度 0.5；无地形随机 seed，喷头 seed 由生产
`sprinkler_seed(id,position)` 确定，顺序放置生成同一 phase。world clock 仍为真实时间，不用帧号假造性能。

### 观测边界与剩余优先级

- **不把 FIFO 等待当作计算**：这些匹配场景始终约 58 FPS（约 17.1 ms 实际帧间隔），
  不能声称“FPS 提升 x%”。总 GPU frame 才是所报告的 Release GPU 收益；prepare/wetting
  的下降不能简单相加。玩家重植物/其它相机的完整花园仍需 controller 或玩家后续验收。
- **8 个实例的整帧收益很小**：未改代码的 shadow/DDGI 等 pass 也整体变慢，optical 有案例上涨；
  这与低负载/FIFO 下 GPU 动态运行状态相容，但没有基线逐帧硬件频率计数，不能确认具体原因。
  不隐藏这些成本，也不声称每个人口都获得整帧收益。优化没有把计算移到这些 pass 或下一帧。
- **P1：optical per-ray 候选列表**。生产 shader 对每条命中实例包围体的 ray 扫描全部
  256 + 128 optical 槽；tile 本身成本极低。可研究精确活跃索引/局部支持候选压缩，保持有限支持、
  地形裁切与独立小水滴。剩余预算是实测 optical scope；潜在节省尚未测量，不给虚构倍数。
- **P2：prepare 轨迹/光照剩余工作**。每个 sample 仍按原 sweep 找首次地形接触；若进一步复用，
  必须证明不同出生方向/速度/飞行年龄的 first contact 不丢失，不能套独立地面圆盘或跨帧旧缓存。
  64→256 lanes 已解决少实例低并行度；257-node 标定表实验没有收益，已否决，不再优先猜优化数学。
- **P3：hardware draw**。16 个约 0.6 ms，整机缓存和 128 全局视角未缩减。后续先测实际 native/cache
  条件与屏幕覆盖，不回到按实例复制大 cache，也不减少四种造型或固定 16 整机姿态。
- **P4：更高人口和玩家世界**。未把容量 256 当作性能验收，也未覆盖所有 GPU/全分辨率/植物密集场景。
  后续在可恢复的用户世界副本测真实 GPU headroom，继续自动 present mode；不得覆盖用户存档或质量。

控制补充：保存的 wetting speed=3、wet patch size=2、drying speed=0.05；本夹具沿用这些值。
固定日光 time_of_day=0.43，停用自动昼夜，仅内存中生效。GUI/camera 原始输入副本及 SHA256 留在证据目录。

## 最终匹配实测表（完整工作组修正之后）

每格为 **p50 / p90（ms）**，展示两次独立运行分位数的算术均值，而非 pooled 逐帧总体分布。
所有最终非零人口都有 warmup 逐实例 GPU 活跃源检查；原始日志/summary.json 保留每次样本数。
表中仅使用 before、final、before-variants、final-variants；不使用 invalid-underdispatch 证据。

### 用户预设：spread / 30% duty / droplets on

| 数量 | CPU frame 基线 | CPU frame 最终 | GPU frame 基线 | GPU frame 最终 |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 17.333 / 17.634 | 17.186 / 17.660 | 4.030 / 5.546 | 3.829 / 5.215 |
| 1 | 17.146 / 17.764 | 17.116 / 17.688 | 5.970 / 6.378 | 5.232 / 5.860 |
| 4 | 17.355 / 17.546 | 17.257 / 17.517 | 6.306 / 6.688 | 5.183 / 5.881 |
| 8 | 17.209 / 17.509 | 17.314 / 17.783 | 6.282 / 6.632 | 6.260 / 6.440 |
| 16 | 17.151 / 17.473 | 17.234 / 17.657 | 7.625 / 8.136 | 6.377 / 6.668 |

| 数量 | GPU 阶段 | 基线 | 最终 |
| ---: | --- | ---: | ---: |
| 1 | prepare | 1.412 / 1.554 | 0.487 / 0.541 |
| 1 | wetting | 0.246 / 0.278 | 0.070 / 0.092 |
| 1 | mist + optical droplets | 0.456 / 0.611 | 0.438 / 0.704 |
| 1 | hardware draw | 0.078 / 0.080 | 0.082 / 0.086 |
| 4 | prepare | 1.218 / 1.257 | 0.422 / 0.462 |
| 4 | wetting | 0.876 / 0.996 | 0.201 / 0.295 |
| 4 | mist + optical droplets | 0.746 / 0.870 | 0.726 / 1.004 |
| 4 | hardware draw | 0.161 / 0.164 | 0.164 / 0.183 |
| 8 | prepare | 0.945 / 0.990 | 0.425 / 0.442 |
| 8 | wetting | 1.359 / 1.577 | 0.504 / 0.585 |
| 8 | mist + optical droplets | 0.863 / 1.252 | 1.142 / 1.605 |
| 8 | hardware draw | 0.284 / 0.319 | 0.301 / 0.333 |
| 16 | prepare | 0.810 / 0.825 | 0.337 / 0.345 |
| 16 | wetting | 2.497 / 3.008 | 0.823 / 0.925 |
| 16 | mist + optical droplets | 1.215 / 1.567 | 1.566 / 1.934 |
| 16 | hardware draw | 0.565 / 0.602 | 0.579 / 0.613 |

Tile 约 0.005–0.010 ms，不是主因。0 个没有喷头阶段，不能把缺少 scope 当作一次已执行的零成本 dispatch。
阶段分位数不可相加为 frame 分位数；更宽并行度会改变资源占用和 GPU 动态运行状态，整帧以实测为准。

### 连续水：100% duty / droplets on

| 布局 | 数量 | GPU frame 基线 | GPU frame 最终 | prepare p50 基线 → 最终 | wetting p50 基线 → 最终 | optical p50 基线 → 最终 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| spread | 0 | 4.450 / 8.203 | 3.856 / 5.453 | — | — | — |
| spread | 4 | 6.380 / 6.524 | 5.938 / 6.146 | 1.290 → 0.526 | 0.867 → 0.287 | 0.915 → 1.071 |
| spread | 16 | 8.402 / 9.028 | 6.306 / 6.742 | 0.917 → 0.362 | 2.766 → 0.896 | 1.502 → 1.763 |
| overlap | 0 | 3.910 / 5.253 | 3.976 / 5.451 | — | — | — |
| overlap | 4 | 6.490 / 6.681 | 6.316 / 6.524 | 1.198 → 0.504 | 0.825 → 0.296 | 1.267 → 1.591 |
| overlap | 16 | 9.200 / 9.897 | 6.922 / 7.444 | 0.907 → 0.314 | 2.601 → 0.794 | 2.848 → 2.894 |

连续模式基线还测了 1/8 个，最终变体重点复测 0/4/16；完整 0/1/4/8/16 双次前后对照以上面的用户预设为准。

### 独立小水滴诊断（不是关闭功能的优化）

| 版本 | 数量 | prepare on | prepare off | optical on | optical off |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline | 1 | 1.412 / 1.554 | 0.944 / 1.099 | 0.456 / 0.611 | 0.349 / 0.606 |
| baseline | 16 | 0.810 / 0.825 | 0.568 / 0.583 | 1.215 / 1.567 | 1.003 / 1.299 |
| final | 1 | 0.487 / 0.541 | 0.462 / 0.519 | 0.438 / 0.704 | 0.363 / 0.634 |
| final | 16 | 0.337 / 0.345 | 0.316 / 0.325 | 1.566 / 1.934 | 1.324 / 1.655 |

这里是同一生产 kernel 的 feature marginal，不是独立 droplet timestamp；差值不是精确可加成本。
未为时间戳拆分/重复光学渲染，从而避免改变测量对象。CPU total 包含 FIFO acquire。

## 最终验证与交接

最终源码（含逻辑工作组修正）全部通过：

- `cargo fmt --check`，以及对独立依赖 crate 文件的 `rustfmt --edition 2021 --check`。
- `cargo check`（由构建生成 shader 结构；没有手改或最终 tracked generated 文件差异）。
- `cargo test`：**1655 app + 4 collider + 1 integration** 通过；8 个 app ignored。
- `cargo test -p re-flora-vkn --lib`：**66** 通过，含工作组/调用数转换、整数 ceil 与大小变化回归。
- `python3 scripts/run_slang_tests.py`：**49** 通过，含 contact/radial/terraces/continuity/droplets/valve。
- 默认验证层 Release `--spray --seconds 12`：**15** 个同步验证场景通过。
  14 层阶梯真实湿体素分别为 `[105,330,501,707,914,1024,1269,1292,1480,1641,1668,1752,1832,929]`，
  全部非零；墙、近中远入水、0 duty、运行时控件、小水滴独立开关、resize、放置移除检查通过。
- 默认验证层 Release `--designs --seconds 15`：**5** 场景通过，覆盖全部四造型、七次 cache/native/
  A/B/candidate 切换；固定 16 整机姿态、全局 views、其它模型 cache key 与正常退出检查通过。
- `cargo build --release` 成功，默认 Release 可执行文件已就绪（正确性构建，非 no-validation 性能副本）。
- `RUST_LOG=info cargo run --release -- --hidden --mute --auto-exit 0.5` 正常退出；
  已用 `--tail-latest-log 200` 检查实际运行日志，无 ERROR/VUID/SYNC-HAZARD，
  `[SHUTDOWN] phase=complete failures=0`。
  原日志为 `target/re-flora-logs/re-flora-20261021-121341.535-131880.log`，副本位于下列证据目录。
- 最终 **26** 个性能运行（用户预设 10 + 连续变体 12 + 独立粒子诊断 4）都检查逐实例 GPU 活跃源，
  无验证层、无强制 present mode，双次重复，真实 Release。独立 1/4/16 quick 检查另有三份原始日志。
- GUI SHA256 `0ba324998187f9a2d19c249983ebe4fa481171f91fe29d172a48b0e041779886`；
  camera SHA256 `d45b3c3875c2896a7b5c83b19520f8401bf29fc7208063fde5bb95191bf7f29a`，前后一致。
  无用户存档、默认质量、首选项或相机改动；没有可见/可听运行、push、release/package。

### controller 在清理工作树前复制

全部在 **`target/garden-delivery/performance/`**，不要复制整个 mutable target cache：

1. **必须**：`baseline`（优化前无验证层 Release）、`final-binary`（优化后无验证层 Release）、
   `before.sha256`、`manifest.json`、`gui-input.toml`、`camera-input.toml`。
   final-binary SHA256：`7fa942b46871653d5a5ec57de2877e6393e238b3ef7920e73b6841734e7679cd`。
2. **有效性能原始证据**：`before/`、`before-variants/`、`before-no-droplets/`、`after-wetting/`、
   `final/`、`final-variants/`、`final-no-droplets/`、`verified-quick/`，含所有日志和 summary.json。
3. **正确性**：`validation-final/` 含全量检查控制台、smoke 原始/尾日志、15 spray 和 5 hardware
   原生截图/JSON/同步验证日志；`validation-wetting/` 保留第一项优化验证摘要。
4. **负面实验（明确不可当收益）**：`rejected-calibration.patch`、`rejected-calibration-test.slang`、
   `rejected-calibration-results/`；`invalid-underdispatch-*` 保留欠调度证据。中间 exe
   `calibration`/`wide-prepare` 不属于交付候选，controller 不应运行或拿它们代替 final-binary。

复测最终版：

```sh
flock -o /tmp/re-flora-garden-delivery-heavy.lock cargo build --release --features no_validation_layer
flock -o /tmp/re-flora-garden-delivery-heavy.lock node scripts/benchmark-sprinklers.mjs \
  --output target/garden-delivery/performance/repeat-final --layouts spread --duties 30
# 完成测量后，恢复验证层/用户可试的默认 Release 构建：
flock -o /tmp/re-flora-garden-delivery-heavy.lock cargo build --release
```

`flock -o` 让锁只由父 flock 持有，避免 sccache daemon 继承 fd 导致命令组结束后仍占锁；
全程不嵌套锁、不并行运行 app。脚本 `--help`/非法参数退出码 2 已检查。

### 共享文件/接口建议

- **无 SprayClock/source/air/contact ABI 或 clock 改动**，不改变 audio worker 的权威阀门时钟接入，
  不改变 wet-material worker 读取现有 moisture 的语义。新工作组 API 只是 Vulkan 调度单位；
  不能把它当作另一个模拟 clock。
- `src/tracer/mod.rs` 仅把 prepare 的调用改为逻辑组数；合并后续渲染工作时务必保留
  `record_workgroups_with_descriptors(... count ...)`，不能恢复 `count * 64`。
- wetting 的 live index list 仅为 shader 内部 shared 临时索引，不是公开 contact 格式；
  共享缓冲仍完整保留 256 contact/lobe 槽、128 droplets、80-byte contact 与 acquired-slot 生命周期。
- 应整体合入本分支（包括 `49a538c0` 修正），不要单独集成 `e5bebdff` 中间提交。
  集成其它花园/音频/湿材质变更后，需要 controller 在 integration worktree 再做聚合 Release 验证。

原始 FPS 报告的历史引入点未确定，完整玩家花园掉帧与人工视觉批准仍待后续验收；
本次交付已实现并实测精确优化，不以这些未验证项冒充成功，也不以关闭功能作为解决方案。
