# 喷水器光学渲染：剩余热点与优化路线

日期：2026-10-21。检查版本：`27835e14`。本轮为源码、既有 Release 证据与一手资料调研，
**没有实现优化、修改画质、运行新的性能实验或取得硬件计数器采样**。
这是运行中每帧的喷水雾/水滴成本，与上一轮 `tracer.comp` **启动时创建管线**的等待不同。

后续的[工业级水雾方案调研](industrial-water-mist-rendering.md)补充了成熟引擎与公开源码：
将局部 raster proxy 与核级 compute 筛选列为并行比较的保真候选，并另外评估低成本混合表示；
并非将本篇的候选顺序视作已经验证的最终方案。

## 结论

优先研究 **保守筛选真正可能影响像素的光学核**，不是减少喷水量、256 个共享喷射/接触样本、
128 个独立光学水滴，或先降低分辨率。实施顺序建议：

1. 补诊断，区分无效候选扫描、真正命中后的地形裁切，以及硬件执行瓶颈。
2. 分别试紧包围盒和稳定的活跃核筛选，作为小而可验证的无画质降级优化。
3. 主攻 **工作组共享的光学核级屏幕候选掩码**，把现有“喷头级 tile 剔除”细化。
4. 若真实命中的体素裁切才是主要成本，转向该路径；不能因候选减少就假定问题解决。
5. 半分辨率雾、时间重用等是另一类有视觉代价的方案，放在后面单独 A/B。

这些是有源码依据的候选，**尚无新方案的加速倍数或 60 FPS 保证**。

## 1. 已测量的热点，不是猜测

本轮重新读取 `target/garden-delivery/final_acceptance/measurement/summary.json` 的 66 次运行，
核对了[最终验收报告](../reports/garden-final-acceptance.md)中的 after 数据。
光学 pass、tile 与 prepare 源码从该次验收到检查版本没有变化。

条件：RTX 3060 Ti / i5-12600KF；真实花园 130190 flora、4 trees；窗口 4096×2560，
**内部场景已经是 820×512**；保存的 30°、5 RPM、5 Hz、30% duty、mist=6，水滴开启。
性能构建使用 Release + `no_validation_layer`，自动 FIFO，hidden/mute；每格是三次
暖机后运行各自 p50 的算术均值，非 pooled 分位数。

| 喷头数 | 整帧 GPU ms | 雾/水滴光学 pass ms | 实际平均 FPS |
|---:|---:|---:|---:|
| 1 | 9.269 | 0.389 | 58.50 |
| 4 | 12.440 | 4.232 | 58.45 |
| 8 | 16.606 | 7.672 | 53.87 |
| 16 | 22.734 | **12.430** | **41.74** |

16 个喷头的其它 p50：prepare **0.231 ms**，wetting **0.670 ms**，硬件外形 **0.677 ms**，
tiles **0.009 ms**。连续出水的 16 喷头光学 pass 为 **15.614 ms**。
不能把各 pass 的分位数相加当作整帧，也不能把 GPU ms 的倒数当作实际 FPS。

若仅以 60 FPS 的 16.667 ms 作规划参考，当前整帧 p50 距离它约 6.1 ms，p90 距离约 9.8 ms；
还须留 CPU/呈现余量。这是预算差，不是预测能省多少，也不是本轮新增的验收门槛。
继续只抠已经很小的 prepare/wetting 不是优先方向。

## 2. 当前算法实际在做什么

主要证据：
[光学 shader](../../shader/slang/sprinkler_mist.comp.slang)、
[tile shader](../../shader/slang/sprinkler_mist_tiles.comp.slang)、
[prepare](../../shader/slang/sprinkler_mist_prepare.comp.slang)、
[资源所有者](../../src/tracer/sprinkler_resources.rs)。

1. 每个喷头每帧发布 256 个连续雾核、128 个光学水滴槽；核结构 64 B。
   真实接触另行发布给 wetting，不能因为雾不可见就跳过生成接触。
2. prepare 已把每核环境光/太阳可见性算好；不是逐像素重新查询整套光照。
3. 16×16 tile 用 bitset 标记相交的**喷头**。目前没有光学核级候选列表。
4. 光学 pass 是 8×8 compute 工作组，覆盖整个内部场景。每个像素检查 tile 中的喷头，
   再用整个喷头 AABB 的外接球作 ray 测试；通过后扫描该实例至多 **256+128=384 槽**。
5. 每槽先检查 amplitude，然后变换到核局部坐标、求椭球交段。真正命中且标记靠近地形时，
   才执行 `mistClipLobeToTerrain`；它从核中心向本像素交段中点进行至多 128 步体素遍历。
6. 紧支撑密度核 `(1-r²)²` 已有**解析线积分**，不是沿每条视线笼统 march 256 步。
   所以“改成解析积分”“加 tile 剔除”“光照每核只算一次”都不能当成尚未实现的新优化。

一个通过 16 个实例粗包围测试的像素，最多扫描 6144 槽。820×512 全屏都通过全部实例时，
松上界为约 **25.8 亿次槽访问/帧**。实际有 tile、ray 剔除和条件分支，不能把这个上界写成
已测工作量或显存带宽；相同地址还可能由 warp/cache 复用。它说明候选粒度有改进空间。

**目前没有证明这 12.430 ms 全部来自空扫描。** 若真正命中后的地形遍历占主导，
精确候选剔除不会减少这些必须保留的命中。源码机制与 GPU 成本归因必须区分。

## 3. 第一优先级：不改图像模型的工作量削减

### A. 紧包围盒、仅光学贡献者的 bounds、稳定活跃索引

- 目前每实例 ray 测试使用 AABB 的外接球。先试直接与已有 AABB 求交；细长、扁平喷雾
  的外接球可能覆盖很多空域。AABB 必须保守处理平行射线、相机在内、近裁面和浮点边界。
- 当前光学 bounds 会包含仍为浇水生成的连续核，即使 flow 关闭或 density=0。
  可让**光学** bounds 只涵盖当前模式实际绘制的贡献者；water bounds/contact 完全不变。
  legacy 雾仍有自己的积分路径，不能错误套用新流动雾的范围。
- 从本帧完整发布槽中稳定提取非零且当前可绘制的索引，删除原本就 `continue` 的项；
  保持 instance、雾核、水滴的原有累加顺序。不使用“很小就忽略”的新阈值。
- 活跃列表可在 prepare 发布元数据，或在消费工作组内生成；比较总成本后再选。
  参考已有 [wetting 稳定压缩](../performance/sprinkler-scaling.md)，但不能假定光学会获得同样收益。
- 30% duty **不等于**恰好 70% 槽无效：出生区间、飞行年龄、首次接触和相位都影响活跃集合。
  必须实测，而不是按 duty 估算加速比。

这是小步候选；应分别测量，不把多项改动绑成一份无法归因的性能数字。

### B. 主攻：细到光学核的屏幕候选，跨像素复用

设计方向：保留现有喷头级 tile bitset；对一个 8×8 工作组涉及的每个喷头，协作构建
**可能覆盖这块屏幕的活跃雾核/水滴掩码**，再由组内像素遍历这一精确候选子集。

- prepare 已知道每核 center、radii、朝向和保守世界 AABB。可一次性生成保守屏幕矩形，
  而不是每个像素重复做筛选准备。近裁面穿越/相机位于体积内要回退到保守范围；
  保留像素边界 guard。误多收候选可以，漏掉贡献不可以。
- 位序沿用原 node 顺序，按 word/`firstbitlow` 升序遍历；仍执行原来的精确椭球测试、
  terrain clip、解析积分和最终合成。有限支持外贡献严格为零，才允许精确剔除。
- 每喷头 384 位仅 **12 个 uint = 48 B** 的工作组掩码，可逐喷头复用。
  这是掩码本身的大小，不包括其它暂存、投影元数据或构建成本。
- 如新增每核 16 B 屏幕矩形，16 喷头为 96 KiB/帧槽；容量 256 时为 1.5 MiB/帧槽。
  3 个槽时后者 4.5 MiB。它是数据/带宽换计算，不能先宣称“免费”。
- **不要默认分配全局稠密的 tile×全部核 bitset**：820×512、16×16 tile 共1664块，
  16 喷头需 1.219 MiB/帧槽；容量256需19.5 MiB/帧槽；若内部升至4096×2560，
  容量256就达480 MiB/帧槽。按容量常驻、再乘in-flight槽并不划算。
  可先用局部共享掩码；如以后采用稀疏列表，溢出必须有完整回退，不能丢水滴。
- 共享屏障是实质性改造：现有 kernel 的越界像素 `return`、逐像素的 instance `continue`
  不能直接包进带 group barrier 的循环。组内实例循环须一致，边缘无效像素仍参加同步，
  仅停止像素读写；不假定所有 GPU 的 subgroup 都为32。[S4]
- 新资源仍由 `SprinklerRendererResources` 的 acquired frame slot 持有，resize/删喷头/模式
  切换要清除或重写相关范围。无 CPU 同步读回，无上一帧候选，无第二套发射器时钟。

这借鉴 tiled/clustered shading 的“把影响源分配给局部样本”思想。[S1]
但雾是**相机至表面之间的空气区间**，不是只有不透明表面上的点；不能直接套表面法线背面
剔除或只根据不透明表面所在深度 slice 剔除雾。先做2D保守候选，再按证据考虑更复杂分层。

### C. 只在证据支持时做：局部数学/数据访问优化

`mistSprayToLocal` 两次使用同一 basis；`mistRayInterval` 与 `mistKernelLineIntegral`
重复表达部分二次式参数。可以研究共享系数、冷热字段或读取顺序，但编译器可能已经复用。
手动延长变量生命周期可能增加寄存器压力，额外预计算也可能增加带宽。

此前源标定表虽然正确，却在 Release 中没有收益，已撤回；不能重演“源码看起来重复就一定快”。
依据 shader profiler 的指令、寄存器、spill 与 stall 再决定。[S2]

## 4. 其它路线及代价

| 路线 | 可能收益来源 | 为什么不作为第一步 |
|---|---|---|
| 精确加速地形裁切 | 减少真正命中核后的 voxel 查询 | 先测其占比；目前已有核附近的保守 occupancy-block 查询，不能当新方案重复提。不能将复杂台阶/墙角偷换成一个地面平面 |
| 按核 raster/splat，尤其细水滴 | 用几何覆盖枚举受影响像素，避免逐像素扫全部核 | 是架构备选而非必然降质，但新增 render target、带宽/overdraw、混合精度、深度/玻璃处理；需要更大验证面 |
| 雾低分辨率、水滴维持当前分辨率 | 降低低频雾的像素积分数 | 有质量代价；当前内部已经820×512，再减半是410×256，薄雾、细水滴、草叶边缘很敏感 |
| 时间重投影/分帧更新 | 跨帧摊销光学成本 | 旋转、5–20Hz脉冲、移动相机、编辑地形容易拖影/陈旧；不能让视觉和真实接触脱节 |
| 动态BVH/硬件ray query | 直接找射线相交的核 | 构建/遍历/资源同步成本和跨设备支持；需遍历所有贡献而非只取最近命中。16喷头先不引入该复杂度 |

**地形裁切的约束**：
[实现](../../shader/slang/sprinkler_mist_geometry.slang)的 receiver 随像素而变，核中心朝不同
receiver 可能遇到不同面。每核只缓存“一个命中面”并不普遍等价。
如采用空区跳跃或经证明的单平面快路径，仍须保留已发布 geometry revision、未知/越界/
预算耗尽的保守语义，并为台阶、薄墙、墙角保留完整回退。不能单纯缩短128步预算来提速。

**raster/splat 的等价性边界**：当前模型先求和 `opticalDepth` 与 `source`，再合成一次。
若改为 raster 枚举核，可以继续在 fragment 里算同一解析积分，累计两者，再统一合成；
它不必退化为普通贴图水滴。但不能直接换成常规逐粒子 alpha blending，二者不等价。
还要验证 FP 累加顺序/精度、保守覆盖、早深度测试和镜头位于核内的情况。

**低分辨率不是保真首选**：NVIDIA GPU Gems 3 第23章支持低频烟雾离屏低分辨率渲染，
也明确指出高频粒子不适合、深度降采样和放大会有 halo/细节损失。[S3]
半宽半高只意味着该层像素数变为1/4，**不是整帧快4倍**。如探索它，应先分离雾与细水滴，
通过 Debug 复选框保留原模式，先实际看图再讨论性能接受；不静默修改玩家保存画质。

另：按累计 opacity 提前结束也不是本算法的精确优化；后续核仍可改变 source 的加权颜色。

## 5. 下一轮实验的最小证据链

### 先补归因

在诊断运行记录：活跃雾核/水滴数；tile/组候选数分布；粗包围通过与精确核命中次数；
进入 terrain clip 的比例和实际 voxel 步数。分别观察 mist-only / droplets-only / combined，
但开关差值只是边际成本，不能当作可直接相加的独立成本。

计数采用采样/工作组归约等低扰动办法；详细计数和 GPU readback 不进入正式计时区。
已有 pass timestamp 不能区分 ALU、cache/memory、分支发散、寄存器/occupancy。[S2]
本机 PATH 可找到 `ncu`、`nsys`，未找到 Nsight Graphics 入口；本轮没有跑硬件 profiler。
CUDA 的 Nsight Compute 不应被当作 Vulkan shader profiler 的替代品。

### 正确性与性能分开

- 基于同一帧已准备的 lobes、场景深度、光照和未加雾背景比较原/新光学输出，
  不能拿不同世界时刻的旋转喷头截图做像素差。候选集合允许多收、不允许漏贡献；
  保持原加法顺序的筛选应争取相同输出，其它浮点重排需单独给出误差证据。
- 保留256个喷射/接触样本、128个水滴槽、归一化出水、真实首次接触、16硬件姿态，
  包括无雾仍浇水、全局 no-particles、仅水滴、legacy、暂停、0%/100% duty。
- 覆盖近裁面、相机在体积中、斜视、玻璃第一界面、薄墙、台阶、地形编辑、世界边界；
  特别测非8/16倍数分辨率、0→多→0喷头、resize与frame-slot重用，检查同步验证错误。
- 若实施比较开关，采用现有声明式保存控件：unchecked 原路径，checked 实验路径；
  不用只有CLI的对照代替游戏内 A/B。
- 先用0/1/4/8/16、30%/100%、spread/overlap和多个相机，暖机、交错至少三次匹配
  Release 测量。看**整帧、prepare、候选构建、optical、内存和实际帧间隔**，避免只是
  把12ms搬到新pass。正式GPU计时关闭验证层；正确性另外跑默认/同步验证。
- 可沿用 [`scripts/benchmark-garden.mjs`](../../scripts/benchmark-garden.mjs) 的同快照/输入
  hash/全实例发布检查与 warmup 规则；新候选构建 scope 必须纳入同帧 sprinkler 合计。
  新 before 是**当前优化版**，不再用恢复旧prepare/wetting的历史诊断二进制当基线。
  使用新证据目录，完整持有 `flock -o /tmp/re-flora-garden-delivery-heavy.lock`；串行重任务，
  不覆盖存档/GUI/相机，不改变 present mode，测试结束恢复默认 Release 构建。

## 6. 一手来源与适用边界

- **[S1]** Olsson、Billeter、Assarsson，2012，
  [Clustered Deferred and Forward Shading — Chalmers机构条目](https://research.chalmers.se/en/publication/161725)，
  DOI `10.2312/EGGH/HPG12/087-096`。已读机构摘要：按2D tile/3D相似属性组织样本并分配影响源，
  可减少无关光照计算。本文据此借鉴组织方法，不把论文光源吞吐数字外推为我们的雾性能。
  作者站点 PDF 本次抓取超时，未宣称读过全文或验证其实现细节。
- **[S2]** NVIDIA，
  [Nsight Graphics Shader Profiler](https://docs.nvidia.com/nsight-graphics/UserGuide/shader-profiler.html) 与
  [GPU Trace Overview](https://docs.nvidia.com/nsight-graphics/UserGuide/gpu-trace-overview.html)。
  官方解释 `# Reg`、`# Warp`、shared memory、Active Threads、Long Scoreboard、Math Pipe、
  Barrier 等指标。支持“先分辨GPU瓶颈再调组大小/数据布局”，不证明我们的 shader 已被某一项限制。
  功能可用性还取决于实际工具版本、调试信息、权限和许可。
- **[S3]** Iain Cantlay / NVIDIA，
  [GPU Gems 3, Chapter 23: High-Speed, Off-Screen Particles](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-23-high-speed-screen-particles)，
  尤其23.1、23.3、23.6、23.7。已读正文：低频雾适合低分辨率，高频细节及深度边缘有代价。
  借鉴原理，不沿用旧DirectX限制或GeForce 8800 GTX性能数字。
- **[S4]** Microsoft，
  [GroupMemoryBarrierWithGroupSync](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/groupmemorybarrierwithgroupsync)。
  官方说明所有组线程须到达同步点，发散分支中的调用行为未定义；对应候选协作代码不能
  保留原本逐像素早退后再随意插屏障的实现方式。

本轮只新增此调研文档；上述优化、计数器和 A/B 控件均未实现。
