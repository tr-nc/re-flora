# God-ray 画质基准与工业界低成本方案

## 范围与结论

基线：`1f97e441`，保留全主渲染分辨率，作为本项目**画质 reference**。它仍是 32-step 随机积分加时域解析，不是数学/物理意义上无误差的 ground truth。本次只调研，不更改默认算法、用户 GUI 或未提交的 `glitch` 相机。

用户判断成立：光效的大部分区域通常比几何轮廓低频，不必每处都按全分辨率执行同样昂贵的积分。但不能由“深度平滑”直接推出“光效平滑”，也不能承诺稀疏采样与全采样在任何场景逐像素相同。

建议下一步仅做一个可实时比较的候选：**同一套积分器，按区域选择采样密度；最终结果和时域仍留在全分辨率**。从 2×2 区域开始，几何边缘与不可靠区域直接走基准采样，平滑区域共享较少的积分结果。不要直接恢复旧的半分辨率结果加双线性放大，也不要同时降低 march steps 或修改随机序列。

若以后要统一太阳光、局部灯和有空间变化的雾介质，再考虑 froxel 三维表示；那是更彻底的表示方式变化，不是当前小改动的必需前提。

## 一手资料与适用范围

### 1. NVIDIA NVVL / Fallout 4：降分辨率 + MSAA + 双边重建

来源：

- [NVIDIA Volumetric Lighting 官方页](https://developer.nvidia.com/volumetriclighting)
- [Fast, Flexible, Physically-Based Volumetric Light Scattering，作者幻灯片](https://developer.nvidia.com/sites/default/files/akamai/gameworks/downloads/papers/NVVL/Fast_Flexible_Physically-Based_Volumetric_Light_Scattering.pdf)

官方页确认该技术用于 Fallout 4。幻灯片 45–46 页明确说明：降采样时使用 MSAA 以保留 shading-rate 收益、改善边缘；合成可用 bilateral upsampling，在文档所称 ¼-resolution 有帮助，在 ½-resolution 则视情况而定。后续页面讨论 Fallout 4 集成。

**启示**：遮挡覆盖精度与昂贵的体积光着色精度不必相同。MSAA 保留更多覆盖/深度信息，不等于给每个样本执行完整积分。

**限制**：NVVL 使用的体积几何/光散射管线与我们逐视线 shadow-map marching 不同；不能把 MSAA 直接加到当前单样本 R32F compute target 就声称获得相同效果，也不能把 SDK 的所有可选模式断言为 Fallout 4 每个质量档的具体配置。

### 2. Assassin’s Creed IV：低分辨率三维体积，按全分辨率表面深度读取

来源：[Bartłomiej Wroński, Volumetric Fog: Unified Compute Shader Based Solution to Atmospheric Scattering, SIGGRAPH 2014](https://bartwronski.com/wp-content/uploads/2014/08/bwronski_volumetric_fog_siggraph2014.pdf)，重点 slides 24–29。

作者给出的 volume 为 160×90×64 或 160×90×128，沿相机视锥排列，深度非均匀分布。先计算体积光照/阴影，再沿深度积分并将累计结果保留在三维纹理里；最终每个原生分辨率片元用自己的真实深度取累计值。

slides 28–29 直接对比了问题根源：二维低分辨率 buffer 必须为多个前景/背景片元选择一个深度，邻域/bilateral reconstruction 只能近似；三维纹理保存不同距离的结果，最终片元可以在自身深度读取，而不是借用邻居已截断的积分。

**启示**：这不只是“边缘补算”，而是避免过早丢掉深度维度，是长期很值得考虑的结构。

**限制**：作者明确承认结果很柔和、损失高频几何细节。它解决的是表示中的前景/背景截断冲突，不代表低分辨率体积能保留任意细小阴影光束。不能把幻灯片中的低成本直接换算成我们的 GPU 成本。

### 3. Unreal Engine：低分辨率体积 + 时域重投影，也有明确副作用

来源：[Epic 官方 Volumetric Fog 文档](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-fog-in-unreal-engine)，Temporal Reprojection / Performance。

官方说明 volume texture 相对低分辨率、沿相机视锥排列，通过 sub-voxel jitter 与较重的 temporal reprojection 抗锯齿。快速变化的灯光（手电、枪口闪光）会留下 lighting trails；文档建议对不适用的灯禁用体积散射贡献。

**启示**：复用空间和时间都是常用工程手段，但“工业界在用”不等于零拖影、零误差。我们已经有 STBN 与 temporal resolve，不能把“再加 TAA”当成尚未实现的答案。

补充：[Frostbite SIGGRAPH 2015 官方课程页](https://advances.realtimerendering.com/s2015/)中的 Hillaire 讲座也明确介绍了统一 extinction volume、粒子体素化、volumetric shadow map 的方向。本次仅引用课程页所确认的架构，不把搜索摘要中的具体网格尺寸/权重当作已核实的实现参数。

### 4. NVIDIA GPU Gems：混合分辨率是已发表的策略，但不是无损保证

来源：[GPU Gems 3, Chapter 23, High-Speed, Off-Screen Particles](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-23-high-speed-screen-particles)，§23.3、§23.6–23.8。

该章节先低分辨率渲染粒子，检测低分辨率结果的边缘，以 stencil 限定区域在全分辨率重画。它是**粒子**方案，不是我们的体积光实现；可借鉴的是“不要让低频区域和边界承担相同着色成本”。

重要反例：作者明确报告薄尖端可能在低分辨率中消失，边缘检测因此也漏掉；最大深度降采样只是更好看的 expedient hack，并无物理保证。因此本项目不能仅在低分辨率结果里找边缘，更不能用 max-depth 缩小遮挡轮廓来伪装正确。

### 5. Intel Outdoor Light Scattering：自适应采样不只看深度

来源：[Intel Outdoor Light Scattering Update 技术文档](https://www.intel.com/content/dam/develop/external/us/en/documents/outdoor-light-scattering-update.pdf)，Refinement threshold、Correction at depth breaks、Refinement criterion；[Intel 官方样例仓库](https://github.com/GameTechDev/OutdoorLightScattering)。本次核实的是 PDF 文档，未审计仓库 shader 实现。

文档中的 epipolar sampling 会在光强变化区域增加积分采样；对于重建不准的 depth breaks，另以逐像素 ray marching 修正。它还提供 Depth / Inscattering 两种 refinement criterion，并明确指出：深度不变时散射光仍可能明显变化，后者在这种情况下更合适。

**启示**：几何深度只是采样需求的一项信号，不能是“保证画质”的充分条件。

**限制**：epipolar 坐标、1D min/max 阴影加速及修正 pass 的改动范围较大，不建议在当前约亚毫秒的 pass 上直接整体移植。文档中“典型 terrain scene 很少像素要修正”不能外推到我们的密集树叶。

### 获取证据

网页正文通过抓取读取；PDF 的网页提取仅得到标题，随后直接下载原 PDF 并用 `pdftotext -layout` 提取正文。原 PDF/文本位于 `target/god-ray-industry/{nvvl,ac4,intel}.{pdf,txt}`。不依赖二手搜索总结确认上述实现细节。

## 对 Re: Flora 的建议：保持算法，只改变空间采样密度

### 第一候选的界限

1. 保留全分辨率 reference；Debug 增加保存绑定的实验复选框，未勾选为 reference，勾选为候选。保持当前全分辨率默认，不悄悄替换。
2. 共享同一个 ray-march 函数和同一套参数。高风险像素直接调用 reference 的积分，不另写近似版光照公式。
3. 分类读取**主渲染分辨率**深度，覆盖完整 2×2 footprint 及必要保护邻域，不仅检查被选中的低分辨率中心。
4. 使用重建出的实际射线长度/线性距离，不直接对非线性 device-Z 做统一阈值。当前积分止于 `min(realDepth, maxDepth)`，实际积分终止距离也应纳入分类；更远表面的深度差未必改变当前积分范围。
5. 有明显前景/背景分裂、细枝叶片、邻域不匹配或有效性不明的区域按全密度采样。不要把近远深度平均成一个虚假的表面，也不要通过改近/远深度让边缘“看起来不漏”。
6. 对明显的光照场变化同样提高采样密度。低分辨率梯度可用于提示，但不能证明不存在更窄的光束；必要时保守保留高密度。若为了覆盖这些情况需要过复杂的估计器，宁可先承认候选的适用范围，不叠补丁。
7. 输出和 history 暂时保持全分辨率，减少新增时域/深度错配变量。纯平滑区域复用少量积分结果；重建核不能跨不同深度表面混合。

这是**待实现和测量的设计**。不是仅增加一个深度阈值就能保证等于 reference，也不是已经验证的优化。

### GPU 执行要点

“2×2 只算一次”不自动等于 pass 快四倍。如果只是让一个 wave 中 3/4 lanes 空闲，同一条长循环仍可能执行，分歧还可能抹掉收益。实现时应按区域组织工作，衡量是否需要分开的低密度/高密度 dispatch 或 tile 列表；先用最简单的可测版本确认实际 GPU 节省，不为理论采样数预先承诺速度，也不一开始搭复杂多级调度框架。

密集枝叶可能让很大比例区域选择 full-rate。要同时记录 full-rate 占比、分类/重建耗时与总 GPU 时间；不能只展示开阔天空的最佳结果。

## 性能收益能有多大？

现有 [release ABBA 结果](../god_ray_full_resolution.md)：

| GPU 中位耗时 | 原 half-res | 当前 full-res |
| --- | ---: | ---: |
| god-ray march | 0.178 ms | 0.648 ms |
| god-ray temporal | 0.0775 ms | 0.205 ms |
| 整帧 | 7.3845 ms | 8.043 ms |

当前 march + temporal 的两个中位数之和约 0.853 ms，只用于预算估计，不冒充逐帧相加后的实测中位数。

若第一候选保留 full-res temporal，以旧 half-res march 成本作粗略乐观参照，可回收的 march 成本约 0.47 ms，约当前 GPU 整帧的 5.8%，**还没有扣除分类与重建**。这不是数学上限，因为其他优化也可能改变单样本成本；但它能避免“光效快四倍，整帧也快四倍”的错误预期。

更理想化的采样数模型：若全密度区域比例是 p，其余区域每 2×2 积分一次，积分数量比例约 `0.25 + 0.75p`。p=20% 时是 40%，p=50% 时是 62.5%；实际 GPU 时间不必按这个比例缩放，尤其存在 wave 分歧、带宽及调度成本时。

不在调研阶段设定未经用户确认的硬验收阈值，也不声称已有“快很多”的候选。先以回收额外成本的一部分且不重现晕边为目标，再报告明确的画质/性能曲线。

## 画质保证应如何建立

### 同帧、同输入的 reference 对比

- 固定相机、太阳、阴影、风/树姿态、噪声 frame serial、depth 和其他照明设置。独立启动的动态场景截图只适合初步观察，不能当严格逐像素误差依据。
- 诊断模式在同一帧对相同输入计算 reference/candidate，各自独立 history；记录 raw 和 resolved 光效、主深度、采样密度 mask 与差值图。切换模式不能无条件复用不同估计器留下的 history。
- 性能跑法则只运行选中路径，关闭 reference 双算、差值计算和截图读回，以 release GPU scopes 做顺序反转比较。

### 至少覆盖这些用例

- `glitch` 的树枝/天空边界：两侧误差、晕边带宽。
- 单像素/细枝与缝隙：平移跨过 2×2 网格时不能反复漏检或 popping。
- 同深度背景上的细光束：不能因为 depth 平滑就把光束抹掉。
- 密集树冠：最坏 full-rate 覆盖比例与分类开销。
- 镜头移动、遮挡揭露、树叶运动：重投影拖影与分类切换稳定性。
- 开阔平滑区域：正常区域的实际节省，而非只看理论积分数。

量化记录 raw/resolved error、深度边缘 ROI 的 p95/max error、误差超过选定容差的像素比例和帧间波动；整体 PSNR/平均误差不足以代表细轮廓质量。同时人工查看静态放大和动态原速 A/B。

**能提供的是对明确场景、容差和动态测试的画质保证，不是对所有未知图像的无损保证。** 对不确定区域选择 reference 可以降低风险，但检测器自身也必须被验证。

## 路线选择

- **现在优先**：保守的 2×2 混合采样候选，保留全分辨率输出/时域和直接 reference 入口；改动集中在现有 god-ray 链路，验证每项成本。
- **不优先**：只恢复 half-res + 普通双线性；只依赖深度差的“保证”；用更重 temporal 掩盖空间漏采样；重开已在本项目测过且被拒绝的 per-step jitter 方案。
- **长期可考虑**：froxel 体积累计表示，从数据结构上保留不同距离的光照。若要把体积系统扩展到多灯、局部介质，再单独立项；不能为了一个约 0.66 ms 的增量就未经实验大改管线。
