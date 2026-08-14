# God ray 固定步距随机相位采样研究

> 日期：2026-08-15
>
> 范围：比较两种屏幕空间 God ray 积分采样：每个 pixel ray 每帧只随机一次起始相位、随后固定步距，与每个 march step 各自独立 jitter。本文不修改 shader，也不把输出 dithering 当作积分质量修复。
>
> 证据标记：**【事实】**来自当前仓库、一手论文或作者源码；**【推导】**可由文中公式直接复算；**【实验】**来自本文记录的确定性 CPU 小实验；**【建议】**仍需在 release-mode 实际 GPU 场景中验证。

## 结论先行

**【建议】保留“每条 ray 每帧一个随机相位，ray 内保持固定 `stepSize`”作为默认方案，不要先改成每 step 独立 jitter。** 用户提出的方案是一个随机平移的规则格点：只要相位在 `[0, 1)` 均匀分布，它对一维积分无偏；它以一次随机数读取换取一整条 ray 的采样位置变化，并能与 temporal accumulation 配合。随机平移格点是有正式数值积分依据的方法，不是临时画质技巧。[Cranley and Patterson, *Randomization of Number Theoretic Methods for Multiple Integration*, 1976](https://doi.org/10.1137/0713071)

**【事实】当前 Re: Flora 已经精确实现了这个方案。** `randomValue` 在 march loop 外按 pixel coordinate 和 `frame_serial_idx` 从 STBN 读取一次，loop 内的位置是 `(randomValue + i) * stepSize`；步距没有逐 step 改变。见 [`god_ray.slang`](../shader/slang/god_ray.slang) 与 [`noise_tex.slang`](../shader/slang/noise_tex.slang)。因此，“改成每 ray 一个 preset jitter”不是尚未实现的修复，而是当前基线。

**【事实】当前也已有独立的 temporal resolve。** raw、history 与 output 都由同一个 half-resolution `R32_SFLOAT` 工厂创建；有效 history 会先做重投影和 3×3 neighborhood clamp，再以静态区域 `10% current + 90% history`、变化区域最高 `65% current` 的权重解析。见 [`extent_dependent_resources.rs`](../src/tracer/extent_dependent_resources.rs)、[`god_ray_temporal.slang`](../shader/slang/god_ray_temporal.slang) 与 [`composition.slang`](../shader/slang/composition.slang)。所以静止相机下仍持续出现固定台阶或不收敛噪声时，第一嫌疑不应再是“缺少 per-ray jitter”或“没有 temporal pass”，而应是 temporal history 是否持续有效、clamp/自适应权重是否拒绝了历史、以及 STBN 相位是否真的在目标 pixel 上随帧覆盖区间。

**【实验】每 step 独立 jitter 不具有普遍优势，而本项目的实际候选也已被 A/B 拒绝。** 32-step CPU 小实验中，两种方案对单个 shadow boundary 的误差几乎相同；固定步距方案对一个与格点互质的周期信号恰好精确，而独立 jitter 产生噪声。进一步的固定场景 release ABBA 中，per-step golden-ratio decorrelation 没有移除 contour，却让 `god_ray.pass` median 从 `118 µs` 增至 `127 µs`（`+7.63%`）。因此当前实现应继续选择 A：每 ray 一个 STBN phase、固定步距。

## 当前实现的实际采样与时域链路

**【事实】采样器使用 NVIDIA STBN 资产的 `128 × 128 × 64` scalar sequence。** `getBlueNoiseSeed()` 以屏幕坐标选空间 texel、以 `frame_serial_idx % 64` 选时间 slice；仓库资产 README 指向 NVIDIA 的原始方法和资产来源。见 [`assets/texture/noise/stbn/README.md`](../assets/texture/noise/stbn/README.md)、[`noise_tex.slang`](../shader/slang/noise_tex.slang) 和 [NVIDIA STBN 官方源码与预生成纹理](https://github.com/NVIDIA-RTX/STBN)。NVIDIA 的原论文与作者说明指出，STBN 的目的正是在每帧保留空间 blue-noise，同时让同一 pixel 的时间序列分布良好，以改善 temporal filtering 的收敛与稳定性；论文还明确展示了 volumetric rendering 用例。[Wolfe et al., *Spatiotemporal Blue Noise Masks*, EGSR 2022](https://research.nvidia.com/publication/2022-07_spatiotemporal-blue-noise-masks)、[NVIDIA 作者技术说明](https://developer.nvidia.com/blog/rendering-in-real-time-with-spatiotemporal-blue-noise-textures-part-1/)

当前一条 ray 的采样位置可写成：

```text
h   = usedDepth / N
x_i = (i + u[pixel, frame]) * h,  i = 0 ... N-1
```

其中默认 `N = 32`，GUI 允许 `1 ... 64`。见 [`config/gui.toml`](../config/gui.toml) 与 [`god_ray.slang`](../shader/slang/god_ray.slang)。这是“随机起点 + 固定间距”，不是随机 ray direction，也不是每一步重新取随机数。

**【事实】生产 volumetric renderer 采用 temporal jitter + reprojection 来处理欠采样并有直接先例。** Assassin's Creed IV 的 volumetric fog 方案把规则 grid aliasing 转换为较高频噪声，再在空间与时间上过滤；其最简单的一次 temporal jitter + reprojection 实验已经去除了绝大多数 edge artifact。[Wroński, *Volumetric Fog: Unified Compute Shader Based Solution to Atmospheric Scattering*, SIGGRAPH 2014，slides 56–60](https://www.advances.realtimerendering.com/s2014/wronski/bwronski_volumetric_fog_siggraph2014.pdf) 这支持“让 sample pattern 随帧变化，再 temporal accumulate”的总体方向，但没有指定必须采用 per-ray phase，更没有证明任何特定 history clamp 或权重必然适合 Re: Flora。

## 本项目的固定场景 A/B 证据

**【实验】A 是当前实现：** 每 pixel/frame 在 loop 外读取一个 STBN `randomValue`，所有 step 使用 `(randomValue + i) * stepSize`。**B 是临时实验候选：** 不增加 texture load，而是在每个 stratum 内使用 `frac(randomValue + i * 0.61803398875)`，即由同一个 STBN phase 生成 golden-ratio decorrelation。A 的代码见 [`god_ray.slang`](../shader/slang/god_ray.slang)；B 只用于实验，未保留在此 research branch。

性能比较使用固定 `2400 × 1350`、相同 camera/scene/options 的 release-mode `A,B,B,A`，每轮从 frame 300 后取样；每个 variant 合并 289 个 GPU scope samples：

| GPU scope | A：per-ray phase | B：per-step sequence | delta |
|---|---:|---:|---:|
| `god_ray.pass` median | 118 µs | 127 µs | **+7.63%** |
| `god_ray.pass` p95 | 119 µs | 127 µs | **+6.72%** |
| `god_ray_temporal.pass` median / p95 | 37 / 38 µs | 37 / 38 µs | 0% / 0% |
| `tracer.render` median | 1,414 µs | 1,421 µs | +0.50% |

**【实验】B 没有带来对应的画质收益。** 对隔离后的 resolved God-ray field 做相同 `×32` 显示和固定 crop，B 没有移除 contour topology；归一化 Sobel mean edge energy 反而从 A 的 `0.000199394` 增到 B 的 `0.000244533`。这个 Sobel 数值只适合作为该固定 ROI 的诊断信号，不能外推成一般画质 benchmark。

**【实验】另外两项 discriminator 也削弱了“可见 contour 就是 32-step lattice”的假说。** 把 `max_checks` 从 32 加倍到 64，没有让 contour 数量加倍、间距减半或位置重排；临时绕过 temporal resolve 时，raw God-ray field 会随帧更新并显示噪声，而启用 temporal 后噪声会被解析。前者说明当前 contour 至少不按简单 `usedDepth / max_checks` 规律缩放，后者直接反驳“God ray raw 不更新”与“完全没有 temporal blend”。它们仍不能单独定位 contour 来自 shadow-map signal、half-resolution reconstruction 还是后续 composition。

**【实验结论】A 在当前实现中占优：** 它保留更低的 God-ray pass 成本，resolved contour 不比 B 差，且现有 temporal pass 确实在工作。B 应回滚；除非后续出现一个能随 step count 定量缩放、并被 per-step sequence 显著降低的固定场景指标，否则不应重开这一方向。

## 两种 jitter 的数学差别

### A. 每 ray 一个相位，固定步距

估计量为：

```text
I_A = h * sum(f((i + u)h)),  u ~ Uniform[0, 1)
```

**【推导】它无偏。** 对随机相位取期望，格点第 `i` 项正好覆盖区间 `[ih, (i+1)h)`：

```text
E[I_A]
= sum(integral from 0 to 1 of h * f((i + u)h) du)
= sum(integral from ih to (i+1)h of f(x) dx)
= integral from 0 to usedDepth of f(x) dx
```

它的关键性质是所有 strata 共用同一个 `u`。误差会相关：有时边界误差互相抵消，效果极好；当信号频率与格点锁定时，也可能同向叠加，形成 resonance。

### B. 每 step 独立 jitter

估计量为：

```text
I_B = h * sum(f((i + u_i)h)),  u_i independently ~ Uniform[0, 1)
```

**【推导】它同样无偏。** 它切断不同 strata 的误差相关性，能把规则采样 aliasing 转成高频噪声。PBRT 对 stratified jitter 的一手说明也是“每个样本在自己的 cell 内随机”，并指出它能把规则 pattern 的 aliasing 转为高频噪声，但仍可能出现 clump 与 undersampling；这不是对所有被积函数都更低方差的保证。[Pharr, Jakob and Humphreys, *Physically Based Rendering*, 4th ed., Stratified Sampler](https://pbr-book.org/4ed/Sampling_and_Reconstruction/Stratified_Sampler)

**【事实】NVIDIA 对 STBN 的使用边界也不支持把同一 scalar mask 盲目扩成 32 个随机维度。** 作者说明 STBN 最适合低 sample count、低维算法；高 sample count 或高维算法通常应转向 low-discrepancy sequence。[NVIDIA STBN 作者技术说明](https://developer.nvidia.com/blog/rendering-in-real-time-with-spatiotemporal-blue-noise-textures-part-1/) 因而若以后确实需要 per-step decorrelation，应单独设计一维 low-discrepancy sequence 或轻量 PRNG，而不是在 loop 内再做 32 次 STBN texture lookup。

## 确定性小实验：反驳“独立 step jitter 总是更好”

**【实验】方法。** 固定 `N = 32`、seed `0x52464C4F5241`，用 Python 标准库的 Mersenne Twister 生成 65,536 个 IID frame。方案 A 每帧生成一个 `u`，方案 B 每帧生成 32 个 `u_i`。对每帧估计值及连续 8/64 帧算术平均计算 RMSE。四个 `[0,1)` 信号分别模拟平滑 mist、单一光影边界、非共振的多次光影切换，以及采样格点锁定的最坏情况：

```text
smooth_exp(t)        = exp(-2t)
one_boundary(t)      = 1 when t < 0.473, else 0
eleven_cycles(t)     = 1 when frac(11t) < 0.5, else 0
step_locked(t)       = 1 when frac(32t) < 0.5, else 0
```

| signal | scheme | bias | RMSE 1 frame | RMSE 8 frames | RMSE 64 frames |
|---|---|---:|---:|---:|---:|
| smooth exponential | ray phase | -0.000059 | 0.007820 | 0.002807 | 0.000988 |
| smooth exponential | independent step | -0.000000 | 0.001580 | 0.000557 | 0.000194 |
| one boundary | ray phase | -0.000005 | 0.010707 | 0.003804 | 0.001360 |
| one boundary | independent step | +0.000033 | 0.010748 | 0.003796 | 0.001413 |
| eleven cycles | ray phase | 0 | 0 | 0 | 0 |
| eleven cycles | independent step | +0.000035 | 0.059477 | 0.021016 | 0.007285 |
| step-locked cycles | ray phase | -0.002884 | 0.500000 | 0.179215 | 0.064629 |
| step-locked cycles | independent step | -0.000209 | 0.088397 | 0.031403 | 0.010507 |

**【实验结论】两种无偏策略没有统一胜者。**

- 单一 binary shadow boundary 只有一个 stratum 不确定，两种方案基本等价。
- 独立 jitter 对平滑非周期信号降低了本实验的方差。
- 固定步距对非共振周期信号保留了格点覆盖优势；`gcd(11, 32) = 1` 时 32 个点刚好均匀遍历所有相位，单帧即精确。
- 固定步距的真实风险是 resonance：当 signal period 恰好等于 `stepSize`，整条 ray 的 32 个样本会同时落在 light 或 shadow，误差完全相关。独立 jitter 能打散它，但 64-frame RMSE 仍不为零。

这个实验只用于判定采样策略的支配关系，不是 Re: Flora 画质或 GPU 性能 benchmark：它没有模拟实际 STBN sequence、half-resolution reconstruction、shadow-map filtering、camera reprojection、neighborhood clamp 或指数时域权重。

## GPU 成本判断

**【事实】当前每个 output pixel 在 march loop 外做 1 次 scalar STBN `Load`，每个有效 step 做 1 次 shadow-map `SampleLevel`。** 默认 32 steps 时，采样相关指令上限是 1 次 noise load + 32 次 shadow sample。见 [`god_ray.slang`](../shader/slang/god_ray.slang)。

**【推导】最直接的 per-step STBN 写法会把 noise load 从 1 次增加到 32 次，即额外 31 次纹理读取；它不会改变 loop 内最多 32 次 shadow test 的数量。** cache 命中、occupancy 与隐藏延迟决定真实 wall-time，因此不能把“采样相关纹理指令从最多 33 次增加到 64 次”直接写成“pass 时间翻倍”。用整数 hash/PRNG 可以避免额外 texture load，但会增加 loop ALU 和随机状态，并且不自动继承当前 STBN 的空间/时间频谱保证。

**【建议】用户的性能判断成立：如果 per-ray phase 已能满足画质，它明显是更小、更可预测的实现。** 只有 God-ray-only 的固定相机 A/B 证明存在格点 resonance，才值得用 hash/low-discrepancy per-step sequence 做候选，并以 release-mode `god_ray.pass` GPU scope 决定是否接受。

## 对当前问题的验证顺序

**【建议】不要再把最终 output dithering 纳入 God ray 积分验收。** 先从 `R32F` raw/resolved God ray field 取证；输出 dithering 只能掩蔽最终量化，无法补回 march 漏掉的光影区间。

本轮已经完成 A、B 和 per-step candidate 的主要 discriminator；剩余调查仍应保持同一 camera、sun、shadow map 和 God ray 参数：

| Case | ray sampling | history | 要回答的问题 |
|---|---|---|---|
| A | 当前 STBN per-ray phase | off | raw field 是否每帧变化；单帧误差形状是否为 blue-noise，而非固定 band |
| B | 当前 STBN per-ray phase | on | 静止相机 8/16/32/64 帧是否单调收敛；history 是否被 reset/clamp/adaptive weight 持续拒绝 |
| C | 64-step 或离线更高步数参考 | on/off | 可见层级究竟来自 32-step 积分，还是 shadow map/half-res reconstruction |
| D（已完成并拒绝） | per-step decorrelated candidate | 同 B | contour 未改善，`god_ray.pass` median 回退 7.63% |

后续验收仍应记录：raw-field 与 resolved-field 的固定 ROI RMSE/temporal variance、64 帧后的残余 contour 强度、history acceptance/reset 计数，以及 release-mode `god_ray.pass` median/p95。若 temporal 输出不随时间收敛，先查 history validity/resolve；当前 D 已经失败，只有新的 reference 证明 C 明显更好、且新的 D 同时接近 C，才有理由重做 per-step decorrelation。

## 最终判断

**【结论】用户的经验判断成立，而且本项目 A/B 已支持它：一条 ray 一个随 pixel/frame 变化的 scalar phase，ray 内固定步距，再由 temporal resolve 累积。** 当前代码已经采用这条路线；per-step sequence 既没有改变 contour topology，又让 `god_ray.pass` median 回退 `7.63%`，所以 A 是当前明确选择。现象不能用“尚未加 per-ray jitter”或“没有 temporal blend”解释；下一轮应把 contour 继续拆分到 shadow-map signal、half-resolution reconstruction 与 composition，而不是继续增加 march 随机维度。
