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
