# Terrain voxel 内 sub-voxel 阴影变化诊断与修复

日期：2026-08-26；叶影 follow-up：2026-08-27

初始基线：`agent/voxel-shadow-subvoxel-diagnosis` / `c1748623`；2026-08-27 先以 merge commit `06949438` 合入本地 `main@4d6a4af2`

任务边界：本文最初只做诊断、测量、根因定位与修复设计；用户随后明确授权运行测试并实施修复。本文末尾记录已实施的 seam 与验收证据。

## 结论摘要

1. **问题可复现，而且变化发生在生产 terrain VSM shadow transmittance（阴影可见度）本身，不只是最终 lighting。** 在固定 `house-overlook`、关闭 flora/cloud/particles 等成分的确定性线性捕获中，同一 receiver voxel、同一存储 normal 的生产 VSM 值最大范围为 `0.673871`；把唯一变量——terrain VSM 的 receiver position——临时改成由 voxel center 派生的 canonical position 后，同样 30,491 个可比较组的 voxel 内范围全部为精确 `0`。
2. **修复前实现本来就会在一个 voxel 内连续变化。** terrain tracer 把连续 ray hit `result.position` 送入 shadow receiver，再连续投影成 shadow UV/depth，线性读取经过空间/时间滤波的 EVSM moments，最后用连续 Chebyshev 函数还原 transmittance。链上没有 voxel identity，也没有“同 voxel 只计算一次”的约束。
3. **hard direct-shadow visibility 与 filtered/soft shadow 必须分开定义。** 同一捕获内，使用 voxel center canonical origin 的 exact binary sun ray 在所有 33,051 个至少覆盖 4 个 internal pixels 的 voxel-face 组中，mixed visibility 数为 `0`。也就是说，hard terrain visibility 已能满足 per-voxel constant；生产 VSM 是另一个、刻意连续且经过滤波的量。
4. **调小 blur、改 nearest、关掉 jitter 或在屏幕末端去噪都不是不变量修复。** 实测 blur=0 + nearest 仍有 102 个组跨越阴影分类，最大 voxel 内范围接近 `1.0`。这些手段只改变边缘宽度/数量，不能让同一 voxel 的所有 receiver 查询共享一个值。
5. **最深且干净的 seam 是 receiver-domain × source policy。** terrain tracer 的 terrain VSM 与 leaf opacity 都使用现有 `terrainRayOriginAlongNormal(center_position, normal, ...)` canonical position；cloud 保持连续 surface position；三项仍在 source 层分别求值后再相乘。这样不改叶片 producer/filter，也不把 cloud 或 raster Flora receiver 一并 voxel 化。
6. **不建议承诺“一个 voxel 的最终颜色完全相同”。** 当前 DDGI 明确让 moment-visibility receiver 固定、但让 probe spatial weighting 跟随连续 `result.position`；local light、tone mapping 和每屏幕像素 dither 也有自己的连续/屏幕空间语义。当前不变量只约束 terrain receiver 上的 terrain/leaf direct-sun terms。
7. **2026-08-27 的叶影 follow-up 扩大了产品不变量。** 用户确认 terrain self-shadow 已正确，但 tree leaf shadow 仍不得在同一个 terrain voxel 内变化。source-separated capture v8 在 51,809 个至少覆盖 4 个 internal pixels 的 receiver voxels 上测得：terrain range max `0.0`、leaf range p99 `0.329216` / max `0.86`、cloud `0.0`。把 terrain receiver 上的 leaf sample 改用同一 canonical voxel-surface position 后，terrain/leaf/combined 的 p99/max 全部为 `0.0`。cloud、DDGI、local light 与最终显示链仍不属于该不变量。第 14 节给出完整证据；它取代本文早期“leaf 保持连续”的旧产品边界。

## 1. 要验证的量及语义边界

“阴影颜色”可能指四个不同层级；不先拆开，会把互不等价的方案混在一起：

| 层级 | 本报告定义 | 是否应 per-voxel constant |
|---|---|---|
| hard terrain visibility | 从 canonical voxel receiver 沿太阳方向做 binary occlusion，值仅为 0/1 | **是；当前 exact capture 已满足** |
| filtered terrain shadow | 生产 1024² VSM/EVSM map 的 transmittance | **建议是，但必须由产品明确接受 voxel 化 penumbra** |
| filtered leaf shadow on terrain | 生产 2048² opacity/history/mask 的 transmittance | **是；softness/opacity 仍存在于相邻 voxels 之间** |
| cloud shadow on terrain | 生产 256² cloud transmittance | 否；本次保持连续 receiver |
| final terrain lighting | direct + DDGI/indirect + emission + preview，之后 tone map/dither | 不建议恒定；DDGI 的连续权重是有意设计 |

本报告的推荐产品不变量是：

> 对一个 terrain receiver voxel 及其存储 normal，**terrain-caster 与 leaf-caster 的 direct-sun transmittance 各自只能有一个值**；值可以随太阳、caster、terrain revision、leaf animation 和各自 history 改变，但不能随该 voxel 内的 camera ray hit position 改变。cloud、DDGI 和 local light 不包含在此不变量中。

如果产品只要求 hard binary visibility 恒定，那么 exact terrain 路径已经满足。当前产品进一步要求 filtered terrain 与 leaf shadow 在 terrain voxel 内恒定，因此不能同时要求单 voxel 内保留连续 penumbra/dapple；软过渡和冠层 opacity 仍保留在相邻 voxels 之间，但过渡会以 voxel 为阶梯。

## 2. 确定性复现

### 2.1 启动前状态

任务开始时先执行：

```text
$ git status --short --branch
## agent/voxel-shadow-subvoxel-diagnosis
$ git rev-parse --short=8 HEAD
c1748623
```

没有文件状态行，worktree clean。随后阅读了仓库根目录的 `AGENTS.md` 和 `CONTEXT.md`。

### 2.2 固定场景与捕获命令

主证据不是经过 tone mapping 的 PNG，而是现有 `.rfi` 线性 float capture：

```bash
cargo run --release -- \
  --hidden --mute --windowed \
  --house-scene --no-flora --no-particles --no-clouds \
  --no-god-rays --no-lens-flare \
  --camera-snapshot house-overlook \
  --environment-irradiance-capture \
    target/voxel-shadow-subvoxel-diagnosis/house-direct.rfi \
  --environment-irradiance-capture-target converged \
  --auto-exit 12
```

同一命令重复一次生成 `house-direct-repeat.rfi`。运行保持 hidden/muted，没有可见启动。日志确认：

- hidden surface：1920×1080；tracer internal extent：960×540；
- terrain VSM：1024×1024；leaf opacity：2048×2048；leaf mask：256×256；cloud map：256×256；
- camera snapshot：`house-overlook`，position `(0.65234375, 0.72, 1.84)`；
- sun direction：`(0.8163623, 0.5273512, 0.23548566)`；
- local light count：0；
- `.rfi`：V6、518,400 samples、3 个 float4 planes：pre-albedo DDGI irradiance、world xyz + exact direct-sun visibility、production direct-light RGB；
- capture target 为 converged，nonfinite count 为 0。

重复捕获比较：

```bash
python3 scripts/analyze_environment_irradiance_capture.py \
  target/voxel-shadow-subvoxel-diagnosis/house-direct.rfi \
  --compare target/voxel-shadow-subvoxel-diagnosis/house-direct-repeat.rfi \
  --compare-direct-light
```

结果为 `compatible=true`、`environment_bit_exact=true`、`direct_light_bit_exact=true`、总 `bit_exact=true`；direct-light SHA-256 均为 `e5cde60df7315b69c99fee77f61dc6007f8255536bd99d1de8492c9a58544814`。因此后续差异不是跨帧噪声。

`--no-shadows` PNG 没有用作定量基线：`tracer.slang:137-141` 的 terrain receiver 目前把 `available_mask` 固定为 `DIRECT_SUN_SHADOW_SOURCE_ALL`，而 `--no-shadows` 会停止 shadow update；这不是一个能证明“全亮参考”的干净隔离开关。所有结论都来自 shadow-on 的线性 planes 和 source-specific probes。

### 2.3 分组方法

现有 analyzer 的 `quantized_voxel_face_key` / `receiver_voxel_key` 位于 `scripts/analyze_environment_irradiance_capture.py:301-337`，使用 256 voxels/world-unit。测量采用：

- 只统计 terrain-hit pixels；
- baseline 以 world-position plane 量化为 voxel face；probe 以沿 camera→surface 向 voxel 内轻推后的 receiver voxel key 分组；
- probe 额外把存储 normal 的 x/y 编入 key，避免把不同 surface identity 混为一组；
- 每组至少 4 个 internal pixels；
- 组内 range 定义为 `max(value) - min(value)`；
- 所有 probe 只临时重定向 capture plane，没有提交或保留诊断代码。

## 3. 测量结果

### 3.1 baseline：同 voxel 覆盖与最终 direct-light 变化

固定 capture 中识别到 52,154 个可见 voxel-face keys，其中 33,051 个至少覆盖 4 个 internal pixels。

| 指标 | internal pixels/face | 1920×1080 screen 的近似面积（nearest 2×2 展开） |
|---|---:|---:|
| p50 | 5 | 20 pixels |
| p95 | 22 | 88 pixels |
| p99 | 33 | 132 pixels |
| max | 53 | 212 pixels |

33,051 个可比较 face 中，exact hard visibility mixed 组为 **0**。其中 31,131 个组 exact visibility 全为 1 且 direct-light 为正；这些组的 production direct-light luma 仍出现：

| voxel 内 luma range 门槛 | 组数 |
|---|---:|
| `> 1e-6` | 371 |
| `> 1e-4` | 368 |
| `> 0.001` | 328 |
| `> 0.01` | 185 |
| 相对组内中位数 `> 10%` | 160 |
| 相对组内中位数 `> 25%` | 73 |

最强组有 17 个 internal pixels，exact visibility 全为 1，但 production direct luma 从 `0.127422` 到 `0.526733`，range/median 为 `3.1338`。这先证明“exact hard visibility 恒定”并不推出“生产 direct light 恒定”；下面的 source probe 进一步定位到 VSM transmittance，而不是 material 或 DDGI。

### 3.2 source probe：变化就在 terrain VSM visibility

临时 capture-only probe 把第三 plane 写为 `(terrain_vsm_transmittance, normal.x, normal.y, terrain_hit)`；保持同一场景、camera、sun 和 converged capture。比较 30,491 个至少有 4 pixels 的 `(receiver voxel, stored normal)` 组：

| Probe | receiver | VSM blur / sampler | exact hard mixed | range > 0.001 | range > 0.01 | range > 0.1 | max range |
|---|---|---|---:|---:|---:|---:|---:|
| 生产默认 | 连续 `result.position` | radius 3 / linear | 0 | 456 | 324 | 158 | 0.673871 |
| canonical A/B | `center_position`→voxel surface | radius 3 / linear | 0 | 0 | 0 | 0 | **0.0** |
| 无空间 blur | 连续 `result.position` | radius 0 / linear | 0 | 202 | 114 | 89 | 0.998447 |
| 最近邻诊断 | 连续 `result.position` | radius 0 / nearest | 0 | 102 | 102 | 102 | ~1.0 |

补充门槛：生产默认有 1,100 组 range `>1e-5`、56 组 `>0.25`；canonical A/B 在所有门槛均为 0。

这组对照可排除两种常见误诊：

- blur radius 3 会把受影响的 voxel 数从 blur=0/linear 的 788（`>1e-5`）扩到 1,100，并降低最尖锐边缘的最大幅度；它是影响范围和形状的贡献者，但不是产生“同 voxel 不同查询”的必要条件。
- nearest 会把受影响的组缩到 102，却让这些组几乎完整跨过 0/1 分类。只要 receiver UV/depth 仍由连续 surface hit 决定，改 sampler 不可能建立 per-voxel invariant。

canonical A/B 只改 receiver identity，不改 producer、moments、blur、history、normal、material 或 DDGI，却把全部组内变化降为精确 0；这是根因定位的最小充分 probe。probe 完成后所有 shader/config 改动均已撤销，并以 `cargo check` 验证恢复。

## 4. 实际调用链：CPU → uniform → producer → filter/history → receiver

### 4.1 CPU、uniform 与 light-space identity

1. `src/app/core/mod.rs:3461-3495` 决定 shadow update，并在当前 checkout 把 cloud 和 cloud shadow 明确硬关闭；`src/app/core/mod.rs:4084-4119` 把 frame-rate-adjusted VSM alpha、blur radius 和 render flags 传给 `Tracer::record_shadow_prepass`。
2. `src/gameplay/camera/shadow.rs:5-63::calculate_directional_light_matrices` 对整个 world bound 做稳定 sphere-fit，计算 world-units/texel，将 light-space center snap 到 texel grid，再生成 orthographic projection。它稳定 shadow grid，但不会把 terrain receiver snap 到 voxel identity。
3. `src/tracer/mod.rs:3056-3087::update_buffers` 观察 sun direction；光空间改变时使 history 失效，然后用实际 1024² extent 计算 matrix。`src/tracer/direct_sun_shadow_runtime.rs:57-145` 管理 light-space revision、terrain/leaf/cloud history validity 和 available mask。
4. `src/tracer/buffer_updater.rs:19-36::update_camera_info` 写入 view/projection 及其逆矩阵；对应 shader uniform 是 `shader/slang/tracer_types.slang:149-158::U_ShadowCameraInfo`。这里没有丢失/重新量化位置精度的 CPU 中间层。

### 4.2 shadow producer

1. 固定分辨率位于 `src/tracer/mod.rs:211-215`：terrain 1024²、cloud 256²、leaf opacity 2048²（leaf mask 由资源层降到 1/8，即 256²）。
2. `src/tracer/mod.rs:3982-4088::record_shadow_prepass` 的顺序是 leaf opacity→leaf temporal→leaf mask，然后 dynamic fruit depth→depth copy→terrain compute trace→VSM filtering/history，最后（若启用）cloud。
3. `shader/slang/tracer_shadow.slang:39-80::tracePixelDepth/main` 从每个 shadow texel center 生成 light ray，march 到连续 caster hit；terrain caster 沿 light direction 加 `terrain_self_shadow_tolerance_voxels × 1/256` 后写最小 depth。默认 tolerance 是 1 voxel（`config/gui.toml:1016-1024`）。
4. `src/tracer/mod.rs:4518-4574` 在更新时把 depth 和 raw R32 shadow target 清为 1，之后 raster fruit depth 和 compute terrain depth 共享 producer。

### 4.3 VSM filter/history

1. `shader/slang/vsm_creation.slang:17-38` 把 raw depth 转成正/负 exponential moments；指数 `(16, 5)` 和 variance floor `1e-5` 位于 `shader/slang/vsm.slang:3-20`。
2. `src/tracer/mod.rs:6240-6273::record_vsm_filtering_pass` 依次执行 creation、horizontal blur、vertical blur + temporal blend，再存 history。
3. `shader/slang/vsm_filtering.slang:12-85` 使用 separable Gaussian，radius=3 时每轴 7 taps、sigma=1.5；`shader/slang/vsm_filtering.slang:88-100` 按 alpha blend history。默认 radius=3、alpha-at-60fps=0.5，位于 `config/gui.toml:1060-1079`；实际 alpha 由 `src/app/core/mod.rs:2011-2018` 按 frame delta 调整。
4. filtered moments 是 RGBA32F，sampler 有意设为 linear：`src/tracer/resources.rs:1799-1823`。因此 blur 之后还有 bilinear footprint。

### 4.4 terrain receiver 与最终 lighting

1. `shader/slang/scene_marching.slang:33-40` 同时返回连续 `result.position` 与 voxel `center_position`；normal/type/hash 从 voxel data 读取。`shader/slang/voxel_data.slang:76-97` 证明 normal 和 hash 是 voxel 存储字段，不是逐屏幕像素 normal reconstruction。
2. 修复前，`shader/slang/tracer.slang::directLighting` 对 terrain 使用 canonical `center_position`，但对 leaf/cloud 调用 `terrainShadowReceiverPositionFromSurface(result.position, normal)`；`shader/slang/terrain_ray_origin.slang::terrainRayOriginFromSurface` 只沿 normal 加固定 world offset，仍保留全部 sub-voxel position 差异。默认 offset 0.0065 world units，约 1.664 voxels（`config/gui.toml:1026-1035`）。
3. `shader/slang/tracer_shadowing.slang::DirectSunShadowReceiver` 已在 terrain 第一阶段修复中深化为 terrain/leaf/cloud 三个 source-specific position。叶影 follow-up 修复位于 `shader/slang/tracer.slang::shadowRayColor` 的 receiver 构造点：terrain 与 leaf 传同一个 canonical `terrainLeafReceiverPosition`，cloud 单独传连续 `cloudReceiverPosition`。
4. `shader/slang/tracer_shadowing.slang:210-220::vsmTransmittance` 连续投影 world position 得到 UV/depth，线性取 moments，再做 EVSM/Chebyshev；`shader/slang/tracer_shadowing.slang:286-325` 把 terrain、leaf、cloud 三个 transmittance 相乘。
5. `shader/slang/tracer.slang:487-523` 把 production direct light 与 `DDGI irradiance × albedo`、emission、preview 合成最终线性颜色。

根因链可压缩为：

```text
同一 voxel 的不同 camera rays
  → 不同 result.position（center/normal/type/hash 相同）
  → FromSurface receiver 保留位置差
  → 不同 shadow UV 与 receiver depth
  → 不同 bilinear EVSM moments / Chebyshev transmittance
  → 同 voxel 的 terrain shadow factor 不同
```

producer blur/history 会改变差异的空间分布，但它们看不到 receiver voxel identity，无法恢复该不变量。

## 5. 尺度量化

terrain voxel size 是 `1/256 = 0.00390625` world units（`shader/slang/tracer_shadow.slang:9`）。固定场景 world bound 为 2×2×2 world units；按 `calculate_directional_light_matrices`、1024² map 和捕获的 sun direction 计算：

| 量 | 数值 | 含义 |
|---|---:|---|
| sphere-fit 初始 radius | 1.832051 world | `sqrt(3)+0.1` |
| 最终 world/shadow texel | 0.00358521 | 一个 scalar voxel width 为 1.0895 shadow texels |
| voxel X 轴在 light map 的投影 | (0.302, 0.552) texels | 取绝对分量 |
| voxel Y 轴投影 | (0.000, 0.926) texels | 同上 |
| voxel Z 轴投影 | (1.047, 0.159) texels | 同上 |
| 整个 voxel cube 的 light-map AABB | 1.349×1.637 texels | 一个 voxel 完全可能跨 texel/阴影边界 |
| radius=3 的空间 blur 半径 | 0.010756 world = 2.753 voxels | 直径约 5.507 voxels，尚未计 bilinear footprint |

因此“shadow map 比 voxel 更细/更粗”都不是充分描述：沿不同 world axes 投影，一个 voxel 的 footprint 约跨 1–2 shadow texels；默认 filter 又在数个 voxel 的尺度上混合 moments。连续 receiver position 在这个尺度上产生不同查询，是预期数学结果。

screen 侧，1920×1080 输出通过固定 `scaling_factor=0.5`（`src/app/core/mod.rs:1194-1208`）渲染为 960×540 internal pixels；`src/tracer/mod.rs:2820-2826` 直接截取缩放尺寸。`shader/slang/post_processing.slang:36-51` 用整数 mapped coordinate 做 nearest 2× upsample，不是 bilinear；所以一个 internal sample 复制为 2×2 screen block。screen dither 在之后逐像素执行。

## 6. 各候选成分的贡献与排除

| 成分 | 当前复现中的状态/证据 | 对 observed terrain VSM 变化的贡献 |
|---|---|---|
| direct terrain hard visibility | `.rfi` exact plane；canonical center ray；33,051 组 mixed=0 | 已恒定；不是变化来源 |
| production terrain VSM/EVSM | 默认 probe 最大 range 0.673871；anchor A/B 为 0 | **主根因所在** |
| world-position sampling | 修复前 `directLighting` 的 shadow query 使用连续 `result.position` | **建立变化的必要条件** |
| bilinear VSM lookup | filtered moments sampler 为 linear | 扩展/平滑变化；不是必要条件，nearest probe 仍失败 |
| Gaussian blur | 默认 radius 3，覆盖约 2.753-voxel 半径 | 扩大受影响 voxel 数，降低部分最尖峰；不是必要条件 |
| EVSM/Chebyshev | 连续 moments + receiver depth → 连续概率上界 | 把位置/depth 差变成连续 transmittance |
| temporal history | 默认 alpha 0.5@60fps，light-space change 时 reset | 沿时间平滑已有 map；固定场景重复捕获 bit-exact，不是随机来源 |
| PCF/PCSS | helper 有 16 Poisson taps，另有 0.35-texel blue-noise UV jitter（`tracer_shadowing.slang:123-207`） | 当前 terrain caller 明确选择 VSM，所以本复现贡献为 0；若改用 PCSS，连续 receiver 仍会变化 |
| camera/subpixel jitter | primary ray 用 `(dispatch+0.5)/extent`；无 camera jitter（`tracer.slang:526-538`） | 本复现为 0；blue-noise seed 虽逐帧生成，VSM 路径不使用 |
| leaf shadow | 最初 terrain-only 复现使用 `--no-flora`；follow-up 为固定 tree、2048² linear opacity + temporal + 256² mask，并在 receiver 做 3×3 max/depth gate | follow-up 的唯一非零 source；连续 receiver 产生 max `0.86`，canonical receiver 后为 `0.0` |
| cloud shadow | CLI 关闭；当前 app 还在 `core/mod.rs:3465-3488` 硬关闭；一般路径 256² linear transmittance（`tracer_shadowing.slang:267-277`） | 本复现为 0；未来启用时应保持 source-specific policy |
| DDGI/indirect | `tracer.slang:493-502` 明确固定 moment-visibility receiver，但让 spatial position basis 跟随 `result.position` | 不进入 terrain VSM probe；可以让 final lighting 在 voxel 内平滑变化 |
| material/normal | type/hash/normal 来自 voxel data；probe key 还包含 normal | 不是 VSM probe 差异来源；没有 screen-space normal reconstruction |
| denoiser/TAA | terrain production tracer→postprocess 路径没有 denoiser/TAA resolve | 本复现为 0 |
| upsampling | integer nearest 2×，不会插值相邻 internal samples | 不创造线性 shadow 梯度，只放大为 2×2 block |
| dither | 默认 1 LSB；`dither.slang:3-18` 是 deterministic screen-space hash | 可在最终显示上增加最多约 ±1 LSB 的细微 pixel 差；`.rfi` 线性 probe 在其之前，故不影响根因结论 |

## 7. 可证伪结论

以下不是仅凭代码形状的推测，而是可由 capture 推翻的结论：

1. **C1：当前 hard terrain visibility 对同一 voxel 恒定。** 证据：baseline 和四个 probe 的 exact mixed 都为 0。反证：固定 scene/sun 下，同一 receiver voxel + normal 的 exact plane 同时出现 0 和 1。
2. **C2：修复前 production terrain VSM 对同一 voxel 不恒定。** 证据：默认有 456 组 range `>0.001`，最大 0.673871。反证：独立 source plane 在足够多的 shadow-edge voxel 上范围均落在 float tolerance 内。
3. **C3：连续 receiver position 是本问题的最小充分控制点。** 证据：仅改成 canonical voxel receiver 后 30,491 组 range 全为精确 0。反证：保持 canonical identity 后仍能在同一 source/history revision 的同组中测到非零 range。
4. **C4：sampler/filter 调参不能建立不变量。** 证据：blur=0 + nearest 仍有 102 组、最大约 1.0。反证：在不引入 voxel identity/caching 的前提下，某种 filter 参数能对所有视角、sun angle、shadow-map resolution 保证同组恒定。
5. **C5：最终屏幕颜色恒定是更强且不同的合同。** 证据：DDGI continuous spatial weighting 与 screen dither 明确位于 terrain shadow 之外。反证：逐项禁用/隔离后证明这些路径也共享 canonical voxel identity。

## 8. 修复方案比较（诊断阶段设计；后续已实施 B）

| 方案 | 能否严格满足 terrain VSM per-voxel | 主要代价/artifact | 评价 |
|---|---|---|---|
| A. 把所有 consumer/source 的共享 `world_position` 全局改成 canonical voxel position | 能 | terrain、leaf、cloud 与 raster Flora 一起 voxel 化；污染其他 receiver 语义 | 不推荐 |
| B. terrain-receiver policy：terrain+leaf canonical，cloud continuous | **能** | terrain penumbra和 leaf dapple 以 voxel 为阶梯；需逐 consumer 验证 | **已实施，推荐** |
| C. 单独的 per-voxel terrain visibility/cache | 能 | 内存、更新调度、sun/caster/terrain revision invalidation；dense 512³ R8 已约 128 MiB | 仅当多个消费者/性能数据证明值得 |
| D. shadow UV snap、nearest、blur=0、调 EVSM 参数 | 不能 | aliasing、硬跳边、漏光/acne trade-off；实测仍失败 | 拒绝作为修复 |
| E. 屏幕空间按 voxel ID 平均/denoise | 可能强行得到屏幕内恒定 | camera-dependent；需 ID buffer；边界泄漏；不同分辨率不一致；修错层 | 拒绝 |
| F. 每像素重跑 canonical exact shadow ray | 能得到 hard constant | 重复计算昂贵；不保留 filtered VSM 语义；应至少按 voxel 缓存 | 仅 correctness/reference |

### 8.1 推荐 seam 的形状

现有最深可复用 primitive 已存在：`shader/slang/terrain_ray_origin.slang:7-39` 能从 `center_position + stored normal` 计算 canonical voxel surface，再加统一 receiver offset；exact capture 和 terrain moisture dry 也已使用这个语义（`ddgi_exact_sun_visibility.slang:8-22`、`terrain_moisture_dry.slang:168-206`）。

诊断时识别出的最深 seam 是 `DirectSunShadowReceiver`。它当时只存一个共享 `world_position`，强迫 terrain/leaf/cloud 共用；terrain 第一阶段修复已把它深化为 source-specific sample positions，叶影 follow-up 只调整 terrain renderer 的 leaf policy：

```text
DirectSunShadowReceiverPositions
  terrain_world_position = canonical voxel surface + terrain receiver offset
  leaf_world_position    = canonical voxel surface + terrain receiver offset
  cloud_world_position   = continuous surface hit + appropriate offset
```

或者把三个 source evaluator 拆成独立调用，再在 caller 组合。接口名并不重要，关键不变量是：**receiver identity 由 receiver domain 与 source 的产品语义共同决定；terrain receiver 上 terrain/leaf 来源于 voxel，cloud 来源于连续 surface；producer/filter 不需要知道 terrain voxel。**

`tracer.slang::directLighting` 同时拿到 `result.center_position` 与 `result.position`；terrain/leaf terms 用前者，cloud 用后者。`terrain_moisture_dry.slang:139-206` 是同一 shadowing contract 的另一个 consumer，但它已经从 voxel center 构造 canonical sample；已实施的 API 深化保持了该行为，没有让 gameplay exposure 随视觉 caller 漂移。

### 8.2 hard、soft 与 DDGI 的明确边界

- **Hard terrain shadow**：binary canonical ray 应严格 per-voxel constant；现有 exact capture 可作为 oracle。
- **Filtered terrain VSM**：若纳入不变量，就在 canonical point 取一次连续 VSM transmittance。softness 仍存在于相邻 voxels 之间，但单 voxel 内不再连续；代价是 penumbra/接触边的 voxel stair-step 和随移动逐 voxel 跳变。
- **PCF/PCSS/area shadow**：即使将来替换 VSM，也必须使用同一个 terrain canonical receiver/seed 才能保持合同；Poisson taps 可产生 soft value，但同 voxel 必须共享 query identity。若要求单 voxel 内平滑，则应明确放弃 constant 合同。
- **Leaf shadow on terrain**：保留高分辨率 opacity、temporal/filter、depth gate、canopy transmittance 与 wind motion，只把 terrain receiver query 改成 canonical。soft/dapple 语义存在于 voxel 之间，不再存在于同 voxel 内。
- **Cloud shadow**：当前关闭；若未来恢复，保留独立连续 receiver，避免 terrain voxel stair-step 放大到大尺度云影。
- **DDGI/indirect**：维持现有“visibility receiver canonical、irradiance spatial weight continuous”的语义。它不属于 direct terrain shadow invariant，验收时必须看独立 source plane，不能用 final RGB 误判。
- **Local lights/material/display**：不由本设计承诺恒定；如产品以后要求最终 voxel-flat lighting，应另立任务逐项定义，而不是扩张本修复。

## 9. 推荐实施阶段

以下是诊断阶段提出的实施顺序；后续授权实现已经完成阶段 0～2。

### 阶段 0：先固化合同与诊断面

- 明文确定 invariant 是 terrain-caster hard only，还是包括 filtered terrain VSM；推荐包括后者。
- 给现有 `.rfi` 增加正式、非临时的 source planes 或 shadow-source capture mode：terrain/leaf/cloud transmittance 分开，不再借用 direct-light plane。
- analyzer 以 `(receiver voxel identity, stored normal, light-space/history revision)` 分组，输出 range/max/offender 数；默认验收 tolerance 建议 `1e-6`，同时保留 exact hard oracle。
- 先记录当前 red baseline（默认 456 组 `>0.001`），避免靠截图主观判断。

### 阶段 1：深化 receiver API，但保持行为不变

- 把 shared position 改为 source-specific positions 或独立 evaluators。
- renderer 与 moisture-dry consumer 先都传入与当前等价的位置；验证 source masks、available masks、history reset 和 generated reflection 无回归。
- 这是独立可提交、可回滚的一步。

### 阶段 2：只切 terrain renderer receiver policy

- `directLighting` 同时传 surface/center identity；terrain VSM 与 leaf opacity 改用 `terrainRayOriginAlongNormal(center, normal, offset)`；cloud 保持连续 surface policy。
- 不改 VSM producer、resolution、blur、temporal alpha、EVSM 参数或 DDGI。
- 用阶段 0 capture 证明 terrain/leaf/combined range 收敛到 tolerance，同时 cloud plane 的连续 policy 不因修复改变。

### 阶段 3：只在测量需要时考虑 cache/perf

- 先做 release hidden benchmark，比较 tracer/shadow/VSM GPU scopes 和 frame p50/p95。
- 如果重复 canonical VSM query 仍是热点，再评估 surface-only sparse cache；不要预先引入 dense per-voxel volume。

## 10. 验收矩阵

| 维度 | 场景/操作 | 必须观察的量 | 通过标准 |
|---|---|---|---|
| 默认 terrain VSM | `house-overlook`、flora/cloud off、blur 3 | terrain source plane 按 voxel+normal range | 每组 `<=1e-6`；exact mixed=0 |
| 无 blur/不同 sampler | radius 0；linear/nearest 仅诊断 | 同上 | invariant 不依赖 filter 参数 |
| hard oracle | exact canonical ray | 0/1 visibility | 同 voxel 不 mixed；与 terrain VSM 的语义差异有记录 |
| leaf 静态/运动 | flora/leaves on，固定与风动序列 | leaf source plane、foliage 专项稳定性指标 | terrain receiver 上 leaf 恒定；producer/filter/wind 保留；不替代 foliage-shadow 验收 |
| cloud | 当前关闭；未来启用后 | cloud source plane | terrain policy 不改变 cloud 连续性/history |
| DDGI | final 与 DDGI debug/capture planes | direct terrain 与 indirect 分列 | direct terrain 恒定；允许 indirect 平滑变化；无把 DDGI 当失败的误报 |
| material/normal | 同 voxel 多 pixels | stored normal/type/hash | identity 一致；没有因屏幕 normal 重建导致的假分组 |
| camera | 固定 snapshot、亚像素平移、跨 voxel 边界 | terrain source range 与 transition location | voxel 内稳定；仅 identity 切换时变；无 camera-dependent swimming |
| resolution | 至少两种 window/internal extent；未来 shadow resolution 变化 | 同一 source invariant | 不依赖 screen/shadow texel ratio |
| sun/history | 改 sun direction，再等待 history convergence | light-space revision、reset、source range | 不跨 light space 混 history；收敛后 per-voxel 恒定 |
| caster/terrain edit | fruit/contact caster、terrain edit | contact shadow、acne/peter-panning、source range | invariant 成立且接触关系没有不可接受退化 |
| moisture gameplay | 干燥 surface exposure | gameplay shadow sample/revision | API refactor保持现有 canonical 行为与 source availability |
| 显示链 | 线性 capture 与最终 screenshot 分看 | source plane、nearest blocks、dither | 线性 invariant 先通过；显示差异仅为已预算的 dither/其他 lighting |
| 性能 | release hidden 固定 camera/workload | frame p50/p95、`tracer.shadow_prepass`、VSM/tracer GPU scopes | 以既定预算为准；debug/unit test 不作为性能证据 |
| E2E | `cargo run --release -- --hidden --mute --auto-exit 0.5` | latest worktree log | 无 Vulkan/shader/error/panic；严禁用可见启动替代 |

## 11. 风险与未验证项

1. **主要产品代价是可见的 voxel stair-step，而不是算力。** canonical VSM 会让整 voxel 同时跨入/跨出 penumbra；斜向边缘、移动太阳或 caster 可能出现更明显的块状跳变。生产修复已实现，但尚未做可见/manual artifact 接受判断。
2. **接触阴影/self-shadow 需专门回归。** canonical surface 是沿存储 normal 与 voxel AABB 求交，再加 0.0065 world offset；它比实际 ray hit 更稳定，但可能让小 caster/contact 在整 voxel 上同时出现/消失，也可能改变 peter-panning。不能用增加 bias 掩盖。
3. **共享 API 是回归面。** `terrain_moisture_dry.slang` 复用 direct-sun shadowing；深化 seam 时必须保持其 canonical exposure 和 source availability，不能只验证画面。
4. **不要把 terrain-receiver policy 扩散到其他 receiver。** leaf 在 terrain 上按 voxel 恒定，不等于 raster Flora、grass、fruit 或 particles 的 receiver 也应量化；cloud 也继续使用连续位置。source-specific API seam 使这些边界可独立验证。
5. **baseline 场景不是全部几何。** terrain-only 阶段验证了 house terrain、固定 sun、无 flora/cloud；leaf follow-up 验证了固定 tree、cloud off、RTX 3060 Ti、960×540 internal/1024² terrain shadow/2048² leaf opacity。未验证不同 sun angle、极端 normal、fruit contact、terrain edit、其他 GPU/driver、不同 resolution。
6. **没有做可见/manual try-out。** 本轮只运行 hidden/muted；因此不把线性数值验收冒充主观视觉接受。
7. **没有新增 lookup，但仍缺正式性能 A/B。** capture v8 的第五 plane 只在启用 capture 时按完整 extent 分配，普通运行只保留最小占位 buffer；production fix 只是选择已有 position。仍未做固定 workload 的 frame/GPU scope benchmark。
8. **按要求 merge，但没有 push 或移除 worktree。** 2026-08-27 已先将本地 `main@4d6a4af2` 合入当前分支；临时 shader probe 不进入提交，提交不包含截图或生成物；正常 per-worktree run log 与 capture 位于 ignored `target/`。

## 12. 最终建议

把缺陷定义为 **terrain receiver 上的 terrain/leaf filtered direct-sun transmittance 缺少统一 voxel identity**，而不是“shadow map 太糊”“分辨率不足”或“屏幕需要 denoiser”。已实施的修复复用 source-specific receiver seam，把 terrain renderer 的 terrain/leaf query 切到已有 canonical voxel-surface helper；cloud 连续位置与 DDGI 现有 hybrid 语义保持不变。

这在最深的正确层建立了可测试不变量，也把代价说清楚：换来的不是更平滑的阴影，而是**同 voxel 一致、跨 voxel 阶梯化的 filtered terrain shadow**。线性 contract 已通过；是否接受跨 voxel 阶梯的视觉取舍，仍需用户可见/manual 体验。

## 13. 授权后的生产修复与验收

### 13.1 实施内容

- `DirectSunShadowReceiver` 现在分别携带 `terrain_world_position`、`leaf_world_position` 与 `cloud_world_position`。先以三个相同位置完成 behavior-neutral seam，并用 capture 证明环境、world、direct-light 三个既有 plane bit-exact。
- `tracer.slang::directLighting` 同时接收 `result.center_position` 和连续 `result.position`：terrain VSM 与 terrain 上的 leaf shadow 使用 `terrainShadowReceiverPosition(voxelCenter, normal)`；cloud 使用 `terrainShadowReceiverPositionFromSurface(surfacePosition, normal)`；local light 仍使用连续 surface position。
- `DirectSunShadowTransmittances` 暴露 terrain/leaf/cloud 分项与组合值，只做一次既有 source sampling。普通最终着色仍使用组合值；capture 单独记录 terrain 分项，避免拿 final RGB 或 direct RGB 猜测 shadow visibility。
- `.rfirr` 当前为 v8：第四个 float4 plane 为 `marcher receiver center XYZ + terrain VSM transmittance`，第五个为 `terrain + leaf + cloud + combined transmittance`。capture buffer 的 plane count 由 Rust 单一常量驱动；未启用 capture 时只分配一个 pixel 的最小占位。
- analyzer 的永久门槛按捕获的真实 marcher center 分组，不再从连续 world hit 与 camera 方向反推 voxel。后者在 voxel face/edge/corner 上会误分组，已由临时 center probe 证伪。

### 13.2 RED→GREEN 证据

修复前，固定 `house-overlook`、flora/cloud/local-light 等干扰关闭的 release capture 中，旧的连续 terrain VSM receiver 在同一 voxel 内产生非零变化；诊断 source-plane probe 的最大范围为 `0.673871`。仅把 terrain receiver 换成 canonical center 后，同样分组的范围为精确 `0`。

最初 v7 terrain-only 黑盒验收使用 800×450 internal、1024² VSM、302,214 个 terrain-hit pixels。按真实 marcher center 分组后，有 25,889 个 receiver voxels 至少覆盖 4 个 internal pixels：

| 指标 | 结果 |
|---|---:|
| `terrain_shadow_receiver_voxel_transmittance_range_p99` | `0.0` |
| `terrain_shadow_receiver_voxel_transmittance_range_max` | `0.0` |
| threshold | `1e-6` |
| validation failures | `[]` |

第二次独立 release capture 与第一次在 environment/world/receiver、direct-light plane 上全部 bit-exact，包含 receiver plane SHA-256 `82a489440954b2f2f01ae345a49f8e40534d042b3c29105ef81bfb18a0f03289`。

### 13.3 当前语义保证

当前修复保证的是：**同一 terrain marcher voxel 的 filtered terrain VSM 与 leaf shadow transmittance 不随 voxel 内 surface hit 改变。** 它不保证 cloud shadow、local light、DDGI spatial weighting、material/emission、tone mapping 或 dither 生成相同 final RGB。hard exact terrain visibility 的 canonical 合同没有改变；foliage producer 的 opacity、filter/history、adaptive proxy 与 wind motion 也没有改变。

### 13.4 验证记录与未验证项

- `cargo fmt --check`、`cargo check` 通过；100 个 native Slang shader 全部可编译。
- `python3 -m unittest discover -s scripts/tests -p 'test_*.py'`：83/83 通过；相关 Python compile 与三个改动过的 DDGI shell script syntax 也通过。
- 完整 `cargo test` 唯一失败是任务开始前已知的独立 PATT fixture：`patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof` 为 expected 1 / observed 2。只排除这一个已知 fixture 后，主 binary 为 644 passed、0 failed、1 ignored、1 filtered out；collision bench 另有 4 passed。
- `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过；日志 `target/re-flora-logs/re-flora-20260827-005714.845-61725.log` 正常完成 shutdown 与 `Application exited successfully`，无 error、panic、validation failure、device lost 或 VUID。唯一启动 warning 是已有的 multiple butterfly atlases 选择提示，与本修复无关。
- 未验证：可见/manual artifact 接受、不同 sun angle 与 shadow resolution、terrain edit/fruit contact、其他 GPU/driver，以及固定 workload 性能数据。没有运行可见窗口、push 或删除 worktree；本地 `main` 已按用户要求先行 merge。

## 14. 2026-08-27 follow-up：tree leaf shadow 在 terrain voxel 内不恒定

### 14.1 复现与 source 分离

用户截图显示：terrain self-shadow 已经按 voxel 恒定，但种树后，树冠投影仍在单个 terrain voxel 内形成不同蓝色深浅。按用户要求，先将本地 `main@4d6a4af2` 以 `--no-ff` 合入当前分支，merge commit 为 `06949438`。合并内容包括 adaptive 4→2 voxel shadow-only proxies、optical-area preservation、spray-level shadow motion 和 foliage benchmark；这些 producer 优化被完整保留。

固定复现使用 `src/app/core/foliage_shadow_bench.rs` 的 tree seed 122、time of day 0.47、固定 camera，并关闭 cloud、particles、god rays 与 lens flare。leaf ON/OFF 的短基线在裸 terrain 区域产生 `3.957566` 的 8-bit mean darkening，证明现象由 leaf source 单独触发。真正的可证伪门槛使用 capture v8：第五个 float4 plane 直接记录 terrain/leaf/cloud/combined shadow transmittance，再按第四 plane 中 GPU 实际写出的 `result.center_position` 分组。

```sh
cargo run --release -- \
  --hidden --mute --windowed \
  --no-clouds --no-particles --no-god-rays --no-lens-flare \
  --foliage-shadow-bench target/leaf-subvoxel.toml \
  --foliage-shadow-bench-warmup-frames 10000 \
  --foliage-shadow-bench-frames 2 \
  --environment-irradiance-capture target/leaf-subvoxel-v8.rfirr \
  --environment-irradiance-capture-target e0

python3 scripts/analyze_environment_irradiance_capture.py \
  target/leaf-subvoxel-v8.rfirr \
  --expect-version 8 \
  --max-terrain-shadow-receiver-voxel-transmittance-range 0.000001 \
  --max-leaf-shadow-receiver-voxel-transmittance-range 0.000001 \
  --max-combined-shadow-receiver-voxel-transmittance-range 0.000001
```

capture 为 960×540 internal pixels、490,667 个 terrain hits。120,252 个可见 receiver voxels 的 screen coverage 为 min/p50/p90/p99/max `1/3/8/16/63` internal pixels，平均 `4.08`；门槛使用的 51,809 个至少覆盖 4 pixels 的 voxels 平均覆盖 `6.89` pixels，p50/p99/max 为 `6/23/63`。2× nearest upsample 后，每个 internal pixel 复制为 2×2 screen pixels。

### 14.2 RED 与根因链

修复前同一 capture 的 source-separated 结果：

| source | voxel groups | range p99 | range max |
|---|---:|---:|---:|
| terrain VSM | 51,809 | 0.0 | 0.0 |
| leaf opacity | 51,809 | 0.329216 | 0.86 |
| cloud（关闭） | 51,809 | 0.0 | 0.0 |
| combined | 51,809 | 0.249785 | 0.86 |

因此变化已经出现在 leaf shadow visibility；DDGI、material/normal、denoiser、upsampling、tone mapping 与 screenshot 只能影响最终表现，不能解释 source plane 的 `0.86` range。实际调用链是：

1. `src/tracer/leaf_shadow_proxy.rs::build_leaf_shadow_proxies` 生成主分支的 adaptive shadow-only proxy，`src/tracer/mod.rs` 在 `leaf_shadow_opacity.pass` 中提交 caster。
2. `shader/slang/leaves_shadow.vert.slang` 应用 spray-level shadow motion 与 `opacity_layer_count`；opacity pass 写 2048² `R8G8B8A8_UNORM`。
3. `src/tracer/mod.rs::record_leaf_shadow_temporal_pass` / `record_leaf_shadow_mask_pass` 对 opacity 做 history blending，并生成 256² influence mask。
4. `shader/slang/scene_marching.slang` 同时给出连续 `result.position` 和 canonical `result.center_position`。旧 `tracer.slang::directLighting` 对 terrain VSM 使用后者，却通过 `terrainShadowReceiverPositionFromSurface(result.position, normal)` 把前者传给 leaf。
5. `tracer_shadowing.slang::leafTransmittance` 把该连续位置投影为 UV/depth；`leafOpacity` 以默认 radius 2 做 3×3 max filter，并执行 leaf depth gate。只要连续 world position 不同，就可能查询不同 opacity/depth。
6. `DirectSunShadowTransmittances.combined = terrain × leaf × cloud`，所以 terrain range 为零时，leaf 的 `0.86` range原样进入 combined 与 direct light。

producer、filter/history 和 depth gate 都不携带 receiver voxel identity。它们可以塑造跨 voxel 的冠层孔洞、软度和运动，但不可能让同一 voxel 的多个连续 query 自动相等。固定场景中一个 voxel 在 1024² terrain map 上投影约 1.349×1.637 texels，在 2048² leaf map 上约 2.698×3.274 texels；默认 3×3 taps 又以 2 leaf texels 为间距。因此旧实现的同 voxel 连续采样本来就会产生变化。

### 14.3 最深修复 seam 与语义

修复位于 terrain tracer 构造 `DirectSunShadowReceiver` 的地方，而不是 leaf producer、sampler 或 post-process：

- terrain VSM：`terrainLeafReceiverPosition = terrainShadowReceiverPosition(result.center_position, normal)`；
- leaf opacity on terrain：复用同一个 `terrainLeafReceiverPosition`；
- cloud：继续用由连续 `result.position` 派生的 `cloudReceiverPosition`；
- local light 与 DDGI spatial weighting：保持连续 surface position；
- raster Flora/grass、fruit、particles 等非 terrain receiver：不受本次 policy 影响。

这不是把叶影改成 binary。2048² opacity、canopy transmittance、temporal history、mask、3×3 filter、depth gate、adaptive proxies 与 wind motion 都保留；它只规定同一 terrain voxel 在同一帧/同一 history revision 查询同一个 filtered leaf 值。软阴影和 dapple 仍存在于相邻 voxels 之间，代价是 leaf edge 随 caster/太阳移动时以 voxel 为台阶变化。

### 14.4 GREEN、foliage A/B 与可证伪结论

同一固定场景、相同 960×540 capture 在修复后得到 51,809 个门槛 voxels；terrain、leaf、cloud、combined 的 range p99/max 全部为精确 `0.0`，threshold `1e-6`，validation failures 为空。由于 canonical position 相同时 3×3 filter、depth gate 与 history 的结果也相同，这同时反驳了“这些阶段会在同点采样时继续制造 voxel 内空间差异”的假设。

为确认主分支 producer 优化没有被替换，另做 1920×1080、warmup 90、capture 64 的临时连续-receiver / 修复后 A/B，两者复用相同 leaf-OFF control：

| foliage metric | 连续 leaf receiver | canonical terrain leaf receiver | 变化 |
|---|---:|---:|---:|
| midband spatial contrast | 4.728728 | 4.697288 | -0.66% |
| significant hole fraction | 0.214598 | 0.210418 | -1.95% |
| silhouette edge density | 0.200771 | 0.201487 | +0.36% |
| midband temporal delta | 0.036830 | 0.037769 | +2.55% |
| highband temporal delta | 0.050969 | 0.050094 | -1.72% |
| mean leaf darkening | 4.061294 | 4.060734 | -0.01% |

旧 official thresholds 在当前 1920×1080 环境下对临时连续 baseline 本身就会因 hole `<0.22`、highband `>0.045` 失败；修复后也同样失败。因此不能把这两项说成本修复引入的 regression。修复对结构指标有小幅重新采样（最大相对变化约 2.55%），仍需用户用实际截图场景检查 voxel stair-step、移动树影和远距离稳定性。

当前可证伪结论是：**terrain receiver 上的 terrain 与 leaf direct-shadow visibility 已满足 per-marcher-voxel constant；cloud、DDGI/local light 和最终 RGB 没有被声明为 constant。** 反证方式是在固定 source/history revision 的 v8 capture 中，再找到同一 recorded center、至少 4 pixels、leaf 或 combined range 大于 `1e-6` 的组。

### 14.5 验收矩阵与剩余风险

| 条件 | 自动验收 | 当前结果 |
|---|---|---|
| fixed tree、leaf ON、cloud OFF | terrain/leaf/combined voxel range ≤ 1e-6 | 通过，三者 max 0.0 |
| receiver policy source contract | terrain 与 leaf canonical；cloud continuous | Rust source regression 通过 |
| capture contract | v8、5 planes、finite、combined product 有效 | analyzer tests 与 GPU capture 通过 |
| foliage producer structure | adaptive proxy 日志仍为 13,749 sources / 8,063 proxies | 通过 |
| shader/Rust build | `cargo check`、100 native Slang shaders | 通过 |
| hidden renderer smoke | release hidden/muted 与 latest log | 通过；正常 shutdown / application exit，无 renderer error |
| visible/manual | 用户截图视角、静止/移动树影、不同 sun angle | 未验证 |

剩余风险主要是跨 voxel 的块状跳变、leaf contact/depth-gate 边界、不同 shadow resolution/GPU、terrain edit 后 identity，以及当前 foliage benchmark 阈值在 1920×1080 的基线漂移。修复没有增加 normal-run shadow lookup；capture v8 仅在显式 capture 时增加第五 plane 的存储/读回成本。
