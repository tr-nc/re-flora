# 连续挖掘中的 DDGI 历史保留：外部证据与实验建议

## 结论与范围

**优先保留“仍然有效的估计”，而不是无条件增加历史权重；把几何有效性、辐射变化、采样进度和更新调度拆开。** 建议顺序：① 编辑不重启采样序列；② 按 probe／方向维护有效性与独立的 irradiance、visibility 置信度；③ 给受影响方向更多新样本，同时保留全场有界延迟的刷新；④ 若这些仍不够，再考虑保存可重新验证的射线样本。不要首先迁移到 ReSTIR。

这是研究建议，不是已验证修复。任务给定的现状是：局部 AABB 扩一格，恢复权重 `min(configured, epoch/(epoch+1))`（e0 为零），geometry revision 改变整 epoch 旋转，局部外 Retain，递归读取完整 source-owned tuple。这里不重复本地代码审计。已修复的 source-readiness/global-sky 注入与 metadata 所有权问题以 [本地调查](cave_edit_lighting.md) 为准；不能用过去的亮度注入解释所有剩余闪烁。

证据级别：**A**＝固定提交源码中实际执行的行为；**B**＝第一方论文／文档；**C**＝本文推导或尚未测量的工程假设。论文的场景经验不是本项目性能证据。以下“建议”均属 C，不能宣称物理精确或无偏收敛。

## 1. 原始 DDGI 与 production 扩展真正说明了什么

- 原始 DDGI 用随机旋转的球面 Fibonacci 射线，存储方向性 irradiance 与距离的一、二阶矩，跨帧混合；§4.4 的实验历史权重范围是 **0.85–0.98**，不是统一 SDK 默认值。§4.2 的参考实现每帧更新所有 probe，作者试过相机距离调度，但因参数及管理复杂度放弃用于最终结果。[P19 §4.2–4.4，B]
- 多次反弹通过上一帧场递归传播；因此几何不变、局部射线命中不变，也不意味着间接辐射不变。原论文明确讨论这种跨帧反弹的时间滞后。[P19 §5.3，B]
- Production 论文明确区分 **irradiance 与 distance 的 hysteresis**，支持 per-probe、per-texel 调整；其 irradiance 快速变化阈值若用于 visibility，作者发现不稳定。论文承认全局降低历史权重并非最佳，提出只处理受影响 probe 值得探索，**没有给出可直接照搬的编辑有效性算法**。[P21 §2.3、4.3，B]
- 论文“动态物体 AABB 扩一 probe cell + self-shadow bias”用于**唤醒附近可能参与 shading 的 sleeping probes**，不是证明这个范围以外的光照不受影响。看不见的 probe 若仍为表面提供照明，也必须更新以传播 GI；静态表面附近是持续更新的 Vigilant probes。[P21 §6.2–6.3，B]
- 论文用降采样表示、可见性矩和启发式插值；SDK 也列出薄墙漏光、probe 分布及偏置限制。保留更多历史只能减少某些时间噪声，不能消除有限 probe 表示误差。[P19 §5.2、7；P21 §8.1；DOC “Rules of Thumb”，B]

**本问题的推论（C）：** 连续编辑若重复让仍有用的局部方向回到 e0，就把已积累的低方差估计反复换成少样本估计。远处最终光照几乎没变仍出现尖峰，值得检查“新采样噪声 + 递归传播 + 恢复重启”的组合，而非先扩大重置范围。外部资料不能证明这就是本项目剩余症状的根因。

## 2. NVIDIA RTXGI：固定版本核对，而非口耳相传的参数

本节检查的是 **RTXGI-DDGI commit `f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6`**（提交日期 2024-02-05）。这是独立 DDGI SDK 的一个固定快照，**不是对所有 RTXGI 版本、UE 插件或 sample 配置的声明**。默认值来自 `DDGIVolumeDesc`；实际程序可覆盖它们。[HDR，A]

| 项目 | 源码中实际值／行为 | 使用边界 |
|---|---|---|
| `probeHysteresis` | **0.97** | 旧历史的权重；header 注释警告接近 1 更稳定但动态响应更慢，约 0.9 或更低会闪烁。[HDR] |
| `probeIrradianceEncodingGamma` | **5.0** | 新 irradiance 先 `pow(x,1/gamma)`，再与已编码历史混合。[HDR, BLEND] |
| `probeIrradianceThreshold` | **0.25** | shader 判断 `maxComponent(old-new) > threshold`，然后 `h=max(0,h-0.75)`。是有方向性的旧值高于新值判据；**不是绝对差，也不是除以旧值的百分比**。默认触发后 h=0.22。[HDR, BLEND] |
| `probeBrightnessThreshold` | **0.10** | 若 `luminance(new-old)>threshold`，把 delta 乘 **0.25**；它作用于编码空间差值，不能解释成物理亮度上限或百分比。[HDR, BLEND] |
| distance 历史 | `lerp(new.rg,old.rg,h)` | 这个 shader 使用同一 volume 基础 h，但没有上述 radiance 阈值分支；不能把论文的双通道独立调节当作该 SDK 已提供两个参数。[BLEND] |
| 零历史 | 旧 texel 向量平方和为零时 h=0 | 与“每次几何修订一律 h=0”不是同一个条件。[BLEND] |
| 暗化量化 | 暗化时各分量最小步幅参考 `1/1024`，且不超过剩余 delta | 连 F32 路径也使用此阈值以加速暗化；不是理想线性 EMA。[BLEND] |

**不能直接复制这里的亮度 delta 抑制。** 它只是准确记录第一方机制，不是本文推荐的亮度盖帽方案；用户关心的是有效样本与响应，不是掩盖能量错误。尤其 shader 的阈值语义比 header 的“ratio／large change”注释更具体，以上以执行代码为准。[HDR, BLEND，A]

与此不同，**2021 论文** §4.3 报告：变化量超过最大值 25% 时 h 减 0.15，超过 80% 时 h=0；小灯变化 irradiance h 降 15% 持续 4 帧，大灯变化降 50% 持续 10 帧，大物体变化 irradiance 降 50%／10 帧、visibility 降 50%／7 帧。这些是论文启发式，不是上表 SDK 默认；其代码示意也不应替代精确的量纲定义。论文作者强调尽量避免低 visibility hysteresis，并承认全局启发式尚未充分探索。[P21 §4.3，B]

另一重要边界：SDK 文档承认部分 texel 长时间累积后仍有非零 variability，提出稳定后暂停 volume、事件发生时重新启用。**本任务不采用“远处没变化就冻结”**：持续编辑、间接传播及未观测变化使事件完备性很难保证。可借用 variability 做调度信号，而不能把低 variability 当作几何／光照有效性证明。[DOC “Probe Variability”，B；取舍为 C]

## 3. 什么历史有效？不要混淆四种状态

以下是基于 DDGI 数据含义提出的**有效性模型（C）**，不是 NVIDIA 已实现的 edit-history API。[P19 §4–5；BLEND；DOC “Probe Data”]

| 缓存对象 | 保留所需证据 | 必须重新观察的原因 |
|---|---|---|
| probe 身份／位置 | 同一 world-space probe，且 relocation 状态对应 | probe 跨墙移动或重新分配后，原方向的距离与照度不再对应同一积分中心 |
| 单射线几何／visibility | 原点及方向相同，射线受测试区间内没有相关几何改变 | 开洞会让旧最近命中消失；填洞可能在旧命中之前加入遮挡；旧 miss 也会失效 |
| 命中点材质／直接光 | 命中面、法线、材质、发光、光源及 shadow 依赖仍有效 | 第一段射线没经过挖掘区，不代表命中点到灯的遮挡不变 |
| 间接 radiance | 上述都成立，而且 transport 输入重新评估或继续刷新 | 远处开洞可以经多次反弹改变同一命中点的辐射 |

### 几何感知的最小失效范围

建议保存／推导 **probe→方向→影响扇区**，而不仅是 probe 中心在不在编辑 AABB。对实际新增／删除几何的保守 bounds 做射线区间相交：未相交只能证明相应第一段几何仍有效，不能证明整个光路辐射有效。删除旧命中与添加近端 blocker 都要覆盖；旧 miss 检查到 tracing domain 的有效远界。多个分离编辑可先保留分离 bounds／空间块版本，避免一个大 union 把两者之间的大量空间误判 dirty。bounds 与依赖不全时宁可标记“不确定并加速重测”，不能标记永久有效。（C）

**方向 texel 不等于一条 ray。** irradiance texel 是很多方向的 cosine-weighted 聚合，distance moments 也是方向加权聚合；单个失效 ray 可贡献多个 texel。只有聚合值时，无法精确减去那条旧贡献，也无法从矩恢复遮挡分布。可先用保守角域降低相关 texel 置信度；若需要“只替换无效样本”，就必须增加方向分桶／贡献缓存、样本权重与版本信息，并重新构建受影响聚合。[P19 §4.4；BLEND，A/B；设计为 C]

### relocation 与 visibility 的特殊处理

SDK 用固定方向射线稳定 relocation/classification，且把这些 fixed rays 排除在 radiance/distance blending 外，避免规则方向污染估计；随机方向依然用于照明更新。这是“稳定几何诊断”与“持续积分探索”分离的直接先例。[DOC “Fixed Probe Rays”；RAYS；BLEND，A/B]

建议移动 probe 时，保留旧 source tuple 作为旧场是合理的，但**不能把旧中心的距离矩当成新中心的距离矩**。小位移是否允许近似复用，要结合最近几何间距、命中面变化与方向视差；不能仅设统一世界单位阈值。跨表面／内外状态变化应使新 probe 的相关历史失效，优先预算新样本。旧 visibility 反映已拆墙时会迟迟挡光；旧 free-space 反映已填洞时会漏光——两种情况都不能无限高 h 保留。（C；数据坐标依据 COMMON, RELOC, P19 §4.4）

## 4. 稳定采样不是固定不变的少量射线

原论文与该 SDK 都在更新中改变随机旋转。SDK `Update()` 调用 `ComputeRandomRotation()`，`DDGIGetProbeRayDirection()` 对非 fixed rays 使用旋转 Fibonacci 方向；没有证据表明它用 geometry revision 做 seed，更没有证据它实现了下面建议的 per-probe progression。[P19 §4.2；UPDATE；RAYS，A/B]

**建议（C）：** 序列身份由稳定 probe ID／空间身份及累计成功采样索引决定，geometry revision 仅参与有效性判断，不参与重启全场序列。每个 probe 的样本索引跨编辑单调前进；取消的任务、未发布数据和重复样本需要明确记账，不能把同一批方向重复当成独立新证据。稀疏调度下按该 probe 的实际更新进度推进，避免经常未更新的 probe 总撞上某些全局 epoch 相位。可以每 probe 用固定 scramble，但持续前进／轮换；不是永久固定同一小组方向。

**Common random numbers（本文数学推导，C）：** 比较编辑前后估计 `A,B` 时，`Var(A-B)=Var(A)+Var(B)-2Cov(A,B)`。在几何大部分不变时用同一随机输入重新求交、重新 shading，可能产生正协方差，降低差分噪声；**它不等于直接重用旧 radiance**。协方差若不为正便无保证。单纯“不因编辑重新 seed”也不等于前后严格配对；可先用于 A/B 和配对探测方向。全场同步旋转还可能造成空间相关闪动，per-probe scramble 值得单变量实验，但没有本项目收益证据。

STBN 第一方论文／说明表明，单独为每帧选择空间 blue-noise 并不保证好的时间谱；其时空序列有利于时间滤波，并强调 history rejection 后的 progressive 行为。**这不是将一张屏幕 blue-noise texture 填进 probe 方向就自动成立**：本项目有球面映射、不同更新频率、余弦积分、递归反馈和取消任务，需要重新验证映射后的覆盖率及时间谱。先修序列连续性，再试低差异／STBN，不要把“随机种子不变”误认为完成算法。[STBN，B；迁移判断为 C]

## 5. 排序后的可行路线

下面均为待验证设计，不提供照抄数值。

| 优先级 | 路线 | 为什么值得做 | 代价／失败方式 |
|---|---|---|---|
| **1** | **跨编辑保持采样进度**，独立 A/B | 改动概念最小；排除 geometry revision 驱动的大范围噪声相位改变 | 不能解决真正无效的 visibility；有限周期或错误记账可能形成盲区 |
| **2** | **per-probe／方向置信度；irradiance 与 visibility 分开更新** | 保留未受影响方向的成熟历史，不让连续操作不断抹除它；与论文细粒度 hysteresis 思路一致 [P21 §4.3] | 聚合历史无法精确拆贡献；过于宽松会漏光／拖影，过于保守又退化成重置 |
| **3** | **局部重要性加速 + 全场保底刷新** | 把预算给 dirty visibility、relocated probes、变化残差和真实照明贡献；稳定区域继续低速探索，而非冻结 [P21 §6.2 的传播要求] | 仅靠局部 dirty 会漏非局部变化；全局预算不足时传播很慢；加权方向采样需正确 PDF／权重 |
| **4** | **显式方向／射线贡献缓存，几何重验后重 shading** | 最接近“只丢弃无效样本”；第一段仍有效时可避免重复求交但更新 radiance | 显存、带宽、依赖跟踪和失效成本高；只重验第一段不足以验证 shadow／多反弹 |
| **5** | **reservoir 重采样体系** | 可研究更灵活的时空候选复用 | 不是 EMA 的直接升级，改动面大；必须维护概率权重、映射与 visibility；不作为本轮首选 [RGI] |

### 自适应参数要依据什么

建议输入：实际有效新样本数、几何失效比例、probe 位移相对 spacing／附近表面距离、方向性 hit-distance 变化、经稳健尺度归一化的 irradiance 残差、噪声基线及最大数据年龄。区分“高噪声”与“真实变化”：仅残差大就降 h 会在噪声最大时进一步放大噪声；可用稳定配对方向、几何证据、多次一致变化佐证。几何变化即使暂时 irradiance 差很小，也应刷新相关 visibility。（C）

h 应按 **有效更新次数／时间** 校准，而非照搬论文“几帧”。理想恒定目标的线性 EMA `H_k=h H_(k-1)+(1-h)X_k` 中，旧误差衰减为 `h^k`；h=0.97 的半衰期约 **22.8 次更新**，降到初始误差 10% 约 **75.6 次更新**。若 probe 非每帧更新，墙钟响应更慢。可用目标时间常数表达 `h=exp(-Δt/τ)`，再根据有效样本和变化证据调整，但长间隔后的单个低样本更新仍不能因 Δt 大而盲目覆盖历史。（公式推导，C；不是 SDK 编码混合的精确响应模型）

低置信度区域优先增加 ray 数或分多次完成更新；如果必须低 h，同时确保新估计有足够证据。设置最长探测间隔及公平预算，避免不断新到的挖掘任务使旧变化永远排不上；编辑边界外保留历史不应等价于停止观察。（C）

## 6. 偏差、延迟与“最终正确”的可检验定义

- **线性 EMA 的有限历史效应（推导，C）：** 静态目标、持续新观察且 `0<h<1` 时初始旧误差消失；但恒定 h、有限 rays 的估计一般仍有非零稳态方差，不能宣称噪声趋于零。SDK 文档也明确指出 variability 可能不归零。[DOC]
- **无限保留会破坏适应性（推导，C）：** h=1 或永久跳过可能受影响的 probe 会让旧误差不再衰减。累积均值的样本数无限增长也可能使新变化响应极慢；需要变更时重新评估有效历史量，而非全局清空或无条件继承无限样本数。
- **数据相关拒绝有选择偏差（推导，C）：** 只保留看起来较暗／较平滑的样本，或按噪声峰值拒绝新样本，会改变估计目标。即使几何验证正确，“存活样本集合”也可能偏向不受编辑影响的方向；不能把它当作全方向均匀的新样本集合。要维护原方向分桶／权重、补齐失效方向并持续探索，或推导相应选择概率修正。由几何／依赖有效性主导，残差只作置信度和调度启发；方向重要性采样须保持必要支撑并处理 PDF。
- **递归固定点不是物理精确解（推导，C）：** 持续公平更新、过时数据逐步退出、场景最终静止、反馈数值稳定，只能为趋近该 DDGI 离散近似的稳定结果提供条件；还受 visibility 矩、插值、有限分辨率、边界和 nonlinear encoding 影响。[P19 §5、7；P21 §4.2、8.1] 反射率和反馈行为不满足稳定条件时，更多历史不自动保证收敛。
- **ReSTIR 必须另算账：** 原始 ReSTIR 是直接光 reservoir 重采样，论文分别描述 biased/unbiased 估计变体；不是“任意历史样本无偏复用”。ReSTIR GI／RTXDI GI 使用次级表面、采样 PDF、radiance 与 reservoir 权重；官方接口明确要求 target PDF、Jacobian 验证、当前／时间 visibility 查询。DDGI texel 的 EMA 没有这些元数据，不能通过增加 h 获得 reservoir 理论保证。[RESTIR, RGI，B] 本文没有核验所有 ReSTIR GI 变体的完整收敛证明，也不以其理论为本项目背书。

因此本文“最终正确”的工程目标是：**编辑停止后不残留可避免的历史错误，达到独立高质量参考允许的误差；编辑中真实开洞／闭洞仍及时响应。** 不是物理无偏保证。

## 7. 建议测量：既防闪烁，也防“平滑但错了”

以下是实验方案（C），本次未运行应用或性能测试。

1. **场景矩阵：** 已收敛封闭洞穴持续浅挖（不打穿）；最终真实开洞；重新封洞／新增遮挡；细小亮口；relocation 邻近墙；两个分离编辑区域；远处由多反弹照亮的表面。相机、曝光、材质／灯光、编辑时刻固定；记录 geometry publication 与 probe publication 时间而不只记帧号。固定采样 seed 配对对比，并以多个 seed 重复，防止只优化单个随机序列。
2. **近／远 ROI 分开：** 固定不会混入天空、直接发光物或新出现表面的墙面 ROI。保存线性 HDR 间接光；另报最终 display 图感知结果，避免 tonemap/TAA 掩盖问题。每帧对像素的 `|L_t-L_(t-1)|` 报 P50/P95/P99/max、空间平均与空间 P99；再对这些帧级量报时间百分位。报告尖峰次数、幅度、持续时间、超过预先声明阈值的面积比例，而不仅是 ROI 平均亮度。
3. **分离真实变化与闪烁：** 浅挖场景用最终状态近似恒定信号；开闭洞场景另测响应曲线。条件允许时用若干中间几何 checkpoint 的高质量离线结果判断每个阶段的目标，不能把所有中间帧都与最终开洞图相比。
4. **最终误差：** 同一最终几何比较高样本、充分迭代且独立初始化的 DDGI，检查历史路径依赖；另比较足够收敛的 path-traced reference，分离 DDGI 表示偏差。报告 linear MAE/RMSE、P95/P99 绝对误差及相对误差（暗部需声明 denominator floor），参考自身噪声也应报告。不能把“旧实现长时间后的图”叫物理真值。
5. **响应：** opening 与 closing 分别记录进入目标变化 10%／90% 所需时间、进入并持续保持误差带的时间、overshoot、残留漏光。时间从几何实际发布算起；连续挖掘期间也要证明新光照持续产生，而非等松手才更新。
6. **估计器诊断：** 每 probe／方向的有效历史年龄、置信度、h、接受／失效样本数、连续序列索引、relocation 次数、最大刷新间隔；统计 irradiance 与 distance residual。检查 distance 二阶矩与一阶矩的一致性、NaN/负方差处理，以及远处更新是否饥饿。
7. **成本：** release-mode 相同编辑 replay，分别测 trace、shade、blend、invalidity、relocation、额外 buffer 带宽和总 GPU 帧时间，报 P50/P95/P99 与峰值，另报 CPU 调度及显存。先做单变量消融（序列／置信度／调度／缓存），再组合；视觉收益与性能预算分别判断，不把低均值 FPS 当验收。

若进入实现阶段，遵循仓库的运行时 Debug checkbox A/B 要求：unchecked 为原模式，checked 为单个候选。**本报告没有实现、运行、视觉批准或性能验收结论。**

## 8. 证据索引与尚未支持的结论

以下已直接读取论文／源码，不以搜索摘要代替验证；GitHub 链接固定提交。P19 PDF 的网页抽取失败，另下载 PDF 并用 `pdftotext` 核对 §4.2–4.4、5.3 及图 12；P21 为固定 arXiv v3 全文。

- **[P19]** Majercik et al., *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields*, JCGT 8(2), 2019：[论文 PDF](https://jcgt.org/published/0008/02/01/paper-lowres.pdf)，[NVIDIA 第一方索引](https://research.nvidia.com/publication/2019-05_dynamic-diffuse-global-illumination-ray-traced-irradiance-fields)。基础数据表示、采样、混合、多反弹。
- **[P21]** Majercik et al., *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production*, JCGT 10(2), 2021：[固定 v3 全文](https://arxiv.org/html/2009.10796v3)，[正式 PDF](https://jcgt.org/published/0010/02/01/paper-lowres.pdf)。§4.3 的参数是论文经验，不是 SDK 默认。
- **[HDR]** [DDGIVolume.h](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/include/rtxgi/ddgi/DDGIVolume.h)：`DDGIVolumeDesc` 的默认值与注释。
- **[BLEND]** [ProbeBlendingCS.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeBlendingCS.hlsl)：实际混合、编码、阈值、fixed-ray 排除与 distance 分支。
- **[DOC]** [DDGIVolume.md](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/docs/DDGIVolume.md)：更新顺序、relocation、classification、fixed rays、variability、使用限制。
- **[UPDATE]** [DDGIVolume.cpp](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/src/ddgi/DDGIVolume.cpp)：`Update()`、`ComputeRandomRotation()`。
- **[RAYS]** [ProbeRayCommon.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/include/ProbeRayCommon.hlsl)：固定／随机方向生成。
- **[COMMON]** [ProbeCommon.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/include/ProbeCommon.hlsl)；**[RELOC]** [ProbeRelocationCS.hlsl](https://github.com/NVIDIAGameWorks/RTXGI-DDGI/blob/f33e496ca31b3f0eec1c4e2cbaa8bb620e337fa6/rtxgi-sdk/shaders/ddgi/ProbeRelocationCS.hlsl)：probe 世界坐标与 relocation；不构成历史重投影正确性证明。
- **[STBN]** Wolfe et al., [论文第一方页面](https://research.nvidia.com/publication/2022-07_spatiotemporal-blue-noise-masks)，[作者技术说明 Part 2](https://developer.nvidia.com/blog/rendering-in-real-time-with-spatiotemporal-blue-noise-textures-part-2/)：时间谱／progressive 特性；没有本项目 DDGI 的直接实验。
- **[RESTIR]** Bitterli et al., [第一方论文页面](https://research.nvidia.com/labs/rtr/publication/bitterli2020spatiotemporal/)：直接光、biased/unbiased 变体；不等于 DDGI。
- **[RGI]** Ouyang et al., [ReSTIR GI 第一方论文页面](https://research.nvidia.com/publication/2021-06_restir-gi-path-resampling-real-time-path-tracing)；[RTXDI GI 集成文档，固定 commit a6efab966b7c3b272da0461578eb56ac61c7cbff](https://github.com/NVIDIA-RTX/RTXDI/blob/a6efab966b7c3b272da0461578eb56ac61c7cbff/Doc/RestirGI.md)：sample/PDF/radiance、Jacobian 与 visibility 接口。

**未获证据支持：** 没找到第一方 DDGI 对“连续体素挖掘、逐方向可证明有效历史复用”的完整现成方案；没有证明编辑 AABB 外照度不变；没有证明稳定序列必然减少本项目闪烁；没有可安全照抄的 relocation-history 阈值或本项目 hysteresis 默认；没有证明任意历史复用无偏；没有测量上述路线的 GPU 收益。最值得借鉴的是**细粒度置信度、几何与辐射分开、稳定诊断与持续探索分开**，而不是某个魔法 h。

## 9. 与本项目实现的逐项核对

以下为整合时对 `6a91614a` 的只读代码审计，不是运行实验。另独立读取了固定 RTXGI header、blending shader 与 P21 全文，核对了第 2 节默认值／实际阈值及“扩一格用于唤醒”的区别。

| 本地事实 | 对方案选择的影响 |
|---|---|
| [GUI 配置](../config/gui.toml) 的 `ddgi_history_retention` 为 **0.99**；[资源层](../src/ddgi/resources.rs) GeometryUpdate 设置 Accumulating，恢复序号取新 field 的 update_epoch；[滤波策略](../shader/slang/ddgi_filter_policy.slang) 按 `n/(n+1)` 限制局部历史 | e0/e1/e2/e3/e4 的上限仍是 **0 / 0.5 / 0.667 / 0.75 / 0.8**。继续提高滑杆不能绕过这个重启；需要区分“新几何版本”与“仍可信的累计历史量”。 |
| 同一滤波策略对局部以外请求 Retain；[执行函数](../shader/slang/ddgi_filter_execution.slang) 直接复制 source | 不能声称所有远处 texel 都被清零。要查实际 dirty 覆盖、插值邻域、后续全场更新和各通道，而不是先调半径。 |
| [请求合并](../src/ddgi/terrain_refresh.rs) 用 AABB union，再按 probe spacing 向每侧扩一格 | 分离编辑可能被一个大框覆盖；“需要检查的范围”和“必须失效的历史”值得分开。范围本身不是非局部 GI 影响的界限。 |
| [epoch rotation](../src/ddgi/resources.rs) 的 seed 含 geometry/radiance revision 和 update_epoch，整个 epoch 使用同一旋转 | 编辑会换全场采样相位；序列连续性是可单独验证的变量，但尚未证明是用户闪烁的主因。 |
| [恢复结束逻辑](../src/ddgi/resources.rs) 清除局部范围后进入 TopologyRecovery，历史上限为 [0.93](../src/ddgi/config.rs) | 后续远处更新并非始终使用 GUI 的 0.99。该 config 文件仍有“私有候选直到稳定才可见”的历史注释；当前 `promotion_is_ready` 允许完整 e0 发布，不能根据旧注释判断画面时序。 |
| [probe relocation](../shader/slang/ddgi_probe_relocate.slang) 是几何相关的确定性搜索；[递归 query](../shader/slang/ddgi_query.slang) 读取 source-owned metadata 与 atlas | 未发现按 geometry revision 随机搬动 probe 的逻辑。保留历史时仍需验证坐标／状态变化，不能破坏刚修好的 source tuple 一致性。 |
| [资源布局](../src/ddgi/resources.rs) 保存聚合 irradiance／visibility，射线数据是 batch-sized transient buffer；当前 [每 probe 64 rays](../shader/slang/ddgi_config.slang) | 先研究保留有效的聚合估计。若要逐条保留、删除旧 ray 的贡献，是新的缓存与依赖设计，不是改一个 h，也不能忽略显存／带宽成本。 |

建议第一阶段只建立时间序列测量和两个独立实验：**采样进度跨编辑连续**、**几何证据允许时不重置成熟历史**。分别记录实际保留量、近／远 ROI 闪烁、开闭洞响应及独立重建后的最终误差；二者单独有效后再组合。每次更新的 h 要结合实际 probe 刷新频率解释，不是每显示帧的 h。

本轮只新增调研文档；没有修改渲染、GUI 参数或生成文件，没有运行应用，也没有宣称候选方案已通过视觉／性能验收。
