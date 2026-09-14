# 附着叶片 Flutter 频率：一手研究与调参建议

调研日期：2026-09-15。仅调研，未修改游戏。结论：**不能把“风越大，叶片每秒抖得越快”当成通用规律，也不能宣称超过阈值后所有叶片频率恒定。**较稳妥的游戏抽象是结构主导的基准节奏、有限且可选的风速调制，以及独立的振幅曲线。

## 证据及适用边界

1. **Tadrist 等（2018），整棵幼樱树风洞实验。**正文 §2 写 Prunus Cerasus，测试约 1–8 m/s；Fig.3a 的示例局部叶片模态约 8 Hz，枝条模态约 1 Hz。Fig.3b 的弱风速依赖、饱和及高风下降，测量量是叶片运动速度贡献，不是所有叶片的 `频率—风速` 曲线。§4/Appendix B 用叶柄扭转固有频率解释局部运动，用湍流激励解释整体枝条运动；高风时弯曲、重叠限制局部运动。§5 明确该树不代表更大总体，且别的叶片可有耦合模态或涡激振动。因此支持“局部与整体分开”，不足以证明通用频率平台。[期刊原文](https://doi.org/10.1098/rsif.2018.0010)、[PMC 全文](https://pmc.ncbi.nlm.nih.gov/articles/PMC6000177/)。原文短摘录：“we do not consider that this tree is specifically representative of a larger population.”

2. **Tadrist 等（2015），Ficus benjamina 真叶与人工叶的风洞/扭转模型。**§2.1 示例约 4 m/s 开始扭转 flutter，阈值取决于朝向；Table 2 两片真叶静空气扭转固有频率分别 14.1、8.4 Hz，这不是扫风得到的 flutter 频率。§4 用气动力改变有效阻尼解释失稳；§5 指出风导致的静弯曲/扭转会改变有效扭转刚度、频率与振型，强风还会混合更多自由度。不要把其简化单自由度模型说成严格 lock-in 证据。[作者单位全文 PDF](https://yakari.polytechnique.fr/Django-pub/documents/tadrist2015rp-1pp.pdf)、[期刊 DOI](https://doi.org/10.1016/j.jfluidstructs.2015.04.001)。核验位置：PDF 第 4 页 Table 2、第 7 页 Eq.4–5、第 8 页 §5。短摘录：“changing the angles and the effective torsion rigidity of the petiole and thereby the torsion frequency and mode shape.”

3. **Zhu & Shao（2017），73 片 tulip tree 叶、0–27 m/s 风洞。**发布者摘要及 Fig.2 给出三类振动状态、两类稳定状态、五类临界风速；迎风正反面和形态影响状态。实际叶片可以随风速经历低频摆动、稳定、较高频振动、再次稳定等过程，不能用一条无限线性增频描述所有状态。[发布者原文及摘要/图注](https://pubs-en.cstam.org.cn/article/doi/10.1016/j.taml.2016.12.002)。证据边界：此轮成功核验发布者摘要/图注，PDF 抓取失败；不据此编造每段的 Hz 或精确转折速度。

4. **McCloskey 等（2017），人工 cottonwood 叶/PVDF 叶柄。**Results → Frequency、Fig.1：恒定风下人工叶频率随风增大；更高风变混乱、最终近乎停止。必须标注这是人工结构，不是真叶普遍规律；约 4.5 Hz 是电信号主频带，文中说机械 flutter 频率约为全波整流后电频率的一半，不要误报成机械摆动频率。[PLOS 原始实验](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0170022)。短摘录：“Both frequency and peak voltage increased with wind speed.”



## 当前分支审计（ffb07215）

- `shader/slang/leaf_flutter.slang::leafTorqueTarget`：风强只乘振幅曲线；平滑噪声节点速率固定为每片 2.8–4.3 次/秒，第二频带乘 2.37。这是激励变化率，不能直接叫“每秒完整摆动次数”。
- `vegetation_response_solver.slang::advanceLeafTorsion`：机械固有频率固定 1.8 Hz，阻尼比 0.32。实际运动频谱同时受激励和这个响应器影响；只加速噪声可能让其被响应器滤掉，出现“更快但更不明显”。
- `vegetation_response.comp.slang`：正常步长上限 1/120 秒，遇长间隔放宽并限制子步数。状态和发布姿态同源；这不等于每片叶子的可见更新频率。
- `vegetation_response.rs` 的发布间隔为 1/(4 × pose_hz)；`sampleLeafAngle` 由固定 seed 选择四个槽中的一个。因此当前 GUI 的 10 poses/s 是每片的可见频率，而不是 40。范围 2.5–20。
- 数学采样风险：10 Hz 采样不能无歧义表示 >=5 Hz 的单一正弦成分，更不用说宽带摆动。内部积分更快不能消除最后显示采样的混叠。这是链路推导，不是本次已重现实机故障。

## 建议的最小产品接口

保持局部抖动与整体风偏移独立。在 Leaves → Wind Motion 内新增折叠的 Flutter Frequency，不增加主层级杂项：

1. **Base Flutter Frequency (Hz)**：基准机械/动画节奏，建议试验默认 1.8 Hz，暂定范围 0.5–12 Hz。默认为了延续当前观感，不是声称真实叶片都为 1.8 Hz。
2. **Strong-Wind Frequency Scale**：强风频率相对基准的倍数，建议默认 1，范围 0.5–2。1 表示不因风强自动加速；>1 加速，<1 放慢。后两者是可调表现，不是假称实验通律。
3. 次级 **Frequency Wind Curve**：独立 Start / Full / Bias 与 Hz 预览；初值沿用现有振幅阈值以减少第一次调试成本，但保存后独立，不强迫振幅与频率共享曲线。

建议映射：
`f(U, seed) = f_base × leafVariation(seed) × lerp(1, strongScale, S_frequency(U))`。
S 为已使用的有界 smootherstep/bias 曲线，满风以上保持端点；没有依据默认无限随风加速。个体差异应稳定，范围需公开说明；无风时振幅为零，不用强迫频率降至零来“停止”。

风强阈值仍是游戏单位，不标为 m/s；没有流场物理校准前不能把论文临界风速直接填进滑杆。频率 Hz 应标作目标/固有节奏：不规则激励下并非严格每秒完成固定次数。

## 实现前必须解决的两个问题

**真实控制可见节奏，而不是只控制噪声。** 同步缩放响应器时间尺度和激励节奏，保持阻尼比及振幅曲线独立，复用现有有界位移/速度状态。需要扫频测量输出主频、RMS/峰值，不能只检查参数被上传。该方案是受研究启发的动画代理，不是完整自激气动模型。不要直接改成固定周期正弦。

**变频不能跳相。** 不可直接用 worldTime × currentFrequency；风变化或拖滑杆时会重映射历史时间、跳到另一个噪声位置。使用持续积分的局部相位/时间状态，按现有叶片生命周期迁移，不重置位置速度。相位归一化应不产生接缝，长时间运行需测试精度。

**显示采样单独处理。** 保留原整体植被离散姿态设置，不悄悄提高整个世界 tick。若开放高频，建议叶片局部 flutter 单独以可见帧节奏取连续机械状态，整体 offset 继续原离散节奏。并非插值能恢复已丢失高频。根据有效显示帧率提示可分辨范围：严格 >2 samples/cycle 只是窄带必要条件，建议 6–8 samples/cycle 是待视觉验证的工程目标，不是植物定律。不要静默 clamp 用户频率；必须显示目标值与受限情况。若暂不升级这一路径，则高频控件不能宣称已正确可见。

## 后续验收计划与本轮边界

- 本轮只调研与设计，没有改 Rust/shader/GUI、没有重启或结束试玩、没有发布依赖。既有 Cargo.lock 与 GUI 用户改动保留。
- 实现时所有新增项走声明式保存；覆盖保存重载以及新控件只出现一次。
- deterministic tests：零风归零、稳风持续、频率端点/曲线、拖动参数不跳相、长时间相位精度、叶片生命周期迁移、不同 dt 的轨迹/频谱、极端值位移有界。
- 在若干固定风速和参数下测输出频谱、过零周期、RMS 和峰值；宽带噪声不只以过零次数判频。
- 对比 10/20/60 等可见采样与足够密的参考轨迹，检查假慢/倒转/节奏拍频；使用相同种子，不通过随机重采样掩盖。
- 持 GPU 锁 release hidden muted 做 ABI/运行安全验证；连续帧或用户试玩验收动态外观，release 测量独立报告性能。此次没有声称通过这些未来实现验收。
