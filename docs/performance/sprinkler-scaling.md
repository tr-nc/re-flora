# 喷头扩展性能诊断（garden scope v1）

## 范围与复现

基线：`a66d9c5144b4a687a4e0c3c7cfa5f8ae7b6cc527`，只添加原生夹具与测量脚本后构建。
脚本 `scripts/benchmark-sprinklers.mjs` 驱动生产 source/contact、土壤、整机和光学路径；
不是 CPU 模拟，不属于普通单测。旧 `mist-pop-*` 会覆盖控件、关闭小水滴，且不能代表用户当前预设。
新 `rotary-perf` 的 0/1/4/8/16 实例都经真实放置接口；spread 间距 0.17，overlap 间距 0.0001。
固定空白平地、无植物以隔离喷头增量；不声称等同玩家完整花园。

```sh
flock /tmp/re-flora-garden-delivery-heavy.lock cargo build --release --features no_validation_layer
# 在改优化源码前保存（不要覆盖这个可执行文件）
mkdir -p target/garden-delivery/performance
cp --reflink=auto target/release/re-flora target/garden-delivery/performance/baseline
flock /tmp/re-flora-garden-delivery-heavy.lock node scripts/benchmark-sprinklers.mjs \
  --binary target/garden-delivery/performance/baseline \
  --output target/garden-delivery/performance/before --layouts spread --duties 30
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

独立的两次 Release 调度实验：单个 prepare p50 约 **1.44 → 0.51 ms**，
4 个约 **1.33 → 0.46 ms**，16 个约 **0.91 → 0.36 ms**（相对仅 wetting 压缩的版本）。
归约只做有限 AABB 的 min/max，换归约树不改变源、contact、标量流量、第一触地/墙体或射程；
所有 buffer 仍由原 acquired frame slot 拥有，disabled optical 槽也照常清零。
Rust bounded-work 回归覆盖 enabled/disabled droplets 与第二轮的部分 lane，确保全部槽无漏算/重复。
没有新增 emitter registry、CPU 粒子、wet footprint 或 clock API。

环境补充：驱动 580.159.04；真实 scene pixels **256×144**（screen 1280×720，原保存值 pixel_stride=5、
point/nearest，UI native），不是全分辨率 1280×720 shading。整机 cache 32px、全局 128 views、16 poses，
共享 96 MiB。未降低原保存质量。平地高度 0.5；无地形随机 seed，喷头 seed 由生产
`sprinkler_seed(id,position)` 确定，顺序放置生成同一 phase。world clock 仍为真实时间，不用帧号假造性能。
