# Terrain 与自定义 SDF 的有机融合研究

> 日期：2026-08-22
>
> 范围：确认用户记忆中的 Inigo Quilez 方法，整理 smooth minimum / maximum
> 的公式、布尔语义、变体性质，以及它与 Re: Flora 当前程序化 voxel terrain 的接入关系。
> 本文只做研究和设计判断，不修改 shader 或 Rust。
>
> 证据标记：**【事实】**来自当前仓库或作者一手资料；**【推导】**由已列公式直接得到；
> **【建议】**是面向 Re: Flora 的下一步选择，仍需实现和固定场景验证。

## 结论先行

**【结论】用户记忆中的名字大概率是 `smooth min` / `smooth max`，不是 `near max`。**
IQ 的原文叫 *A Study on Smooth-Minimums*：普通 `min()` 做隐式形体 union 时会在相交处留下导数不连续，
而 smooth minimum 只在两输入值接近时改写它们，使交界成为圆滑的、有机过渡。
[Inigo Quilez, *A Study on Smooth-Minimums*](https://iquilezles.org/articles/smin/)

**【建议】霍比特山丘应该进入现有 terrain atlas 的生成/编辑链路，而不是成为只参与绘制的独立
render shape。** 这样山丘天然参与现有 Surface/Contree、碰撞、水体 terrain dependency、
DDGI、种植和后续 terrain edit。对目前这个不需要 overhang 的土丘，最小的第一版是在初始化
高度场旁加入一个局部 hill field，并在写 atlas 前做有界 polynomial smooth max；房屋内部空腔和
圆门则在合并实体之后做 subtraction。不要先另建一套永久 analytic-SDF renderer。

这里必须先处理一个符号差异：

- IQ 的标准 SDF 约定是 **内部负、外部正**；union 是 `min(dA, dB)`，所以 smooth union 是
  `smin(dA, dB, k)`。[IQ 3D distance functions](https://iquilezles.org/articles/distfunctions/)
- Re: Flora 当前初始化 shader 用 `qTerrain = surfaceHeight - y`，并在 `qTerrain >= 0` 时写实体；
  它是 **实体正、空气负** 的 occupancy/density field。因此同一 union 在当前符号下是
  `max(qTerrain, qHill)`，平滑版是 `smax(qTerrain, qHill, k)`。

所以用户说“near max”很可能正好混合了名称记忆与本项目所需的实际运算：IQ 原函数叫
smooth minimum，但在 Re: Flora 当前的 solid-positive 场上应使用它的对偶 **smooth maximum**。

## 1. IQ 的 polynomial smooth minimum

### 1.1 推荐实现

**【事实】IQ 当前最常用的有界 quadratic polynomial 形式是：**

```slang
float sminQuadratic(float a, float b, float k)
{
    k *= 4.0;
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}
```

这里 IQ 当前把 `k` 归一化成 `a == b` 处相对 hard `min` 的最大偏移。函数内部乘 4 后，实际
受影响的 field-difference band 是 `abs(a-b) < 4k`。网上大量文章和旧 shader 使用下面未归一化
的 mix 写法：

```slang
float sminQuadraticLegacy(float a, float b, float bandK)
{
    float h = clamp(0.5 + 0.5 * (b - a) / bandK, 0.0, 1.0);
    return lerp(b, a, h) - bandK * h * (1.0 - h);
}
```

两者只有在 `bandK = 4 * k` 时等价，不能把同一个数值直接带入两个版本。`k > 0` 与输入 field
使用同一距离单位；band 外精确退回 `min(a,b)`。quadratic 版本是 C1 连续。
IQ 的推导还给出 C2 cubic 版本；若同样把参数归一化成相等处最大偏移为 `k`，写法是：

```slang
float sminCubic(float a, float b, float k)
{
    k *= 6.0;
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * h * k / 6.0;
}
```

cubic 有二阶连续性；内部 `6k` 是其 field-difference band。上述公式与 C1/C2 说明来自 IQ
[“A list of Smooth-minimums”与“The DD Family”](https://iquilezles.org/articles/smin/)。

### 1.2 为什么它适合 terrain + hill

**【事实】polynomial `smin` 满足 `smin(a,b,k) <= min(a,b)`，因此对 negative-inside SDF
做 union 时是保守的；IQ 指出这一性质不会让 sphere tracer 越过原来的尖锐交界。当前归一化
代码中 caller-facing `k` 是形体的最大 inflation / bounding-box expansion；quadratic kernel
实际只在 `abs(a-b) < 4k` 时改变 field。**
[IQ “Normalization, thickness and bounds”与“Properties”](https://iquilezles.org/articles/smin/)

**【推导】smooth max 只需要利用符号对偶：**

```slang
float smaxQuadratic(float a, float b, float k)
{
    return -sminQuadratic(-a, -b, k);
}
```

它满足 `smax(a,b,k) >= max(a,b)`。因此对 Re: Flora 的 solid-positive field，山丘与自然
terrain 的 smooth union 可写成：

```slang
float qTerrain = surfaceHeight - worldPosition.y;
float qHill = -sdHill(worldPosition); // sdHill 内负；换成实体正
float qSolid = smaxQuadratic(qTerrain, qHill, blendWidth);
bool solid = qSolid >= 0.0;
```

这会在 terrain 与 hill 两个实体接近的窄带内补出圆滑肩部，而不是简单 `max()` 留下硬折线。

## 2. Union、intersection 与 subtraction 的符号表

设标准 SDF `d < 0` 表示形体内部，`A \\ B` 表示从 A 中挖掉 B：

| 运算 | sharp，negative-inside | smooth，negative-inside |
|---|---|---|
| union `A ∪ B` | `min(dA, dB)` | `smin(dA, dB, k)` |
| intersection `A ∩ B` | `max(dA, dB)` | `smax(dA, dB, k)` |
| subtraction `A \\ B` | `max(dA, -dB)` | `smax(dA, -dB, k)` |

这些 sharp CSG 规则与 IQ 的 distance operation 表一致；这里把 subtraction 的参数明确写成
`A \\ B`，避免 IQ 示例函数 `max(-d1,d2)` 的调用顺序产生歧义。
[IQ 3D distance functions, “distance operations”](https://iquilezles.org/articles/distfunctions/)

令 Re: Flora 风格的 solid-positive field `q = -d`，同一张表整体翻转：

| 运算 | sharp，solid-positive | smooth，solid-positive |
|---|---|---|
| union `A ∪ B` | `max(qA, qB)` | `smax(qA, qB, k)` |
| intersection `A ∩ B` | `min(qA, qB)` | `smin(qA, qB, k)` |
| subtraction `A \\ B` | `min(qA, -qB)` | `smin(qA, -qB, k)` |

**【建议】霍比特房结构的 field 顺序应是“先并、后挖”：**

```text
qCoveredMass = smax(qNaturalTerrain, qHill, kGround)
qWithInterior = smin(qCoveredMass, -qInteriorCavity, kInterior)
qWithDoor     = smin(qWithInterior, -qRoundDoor, kDoor)
```

门口和室内不一定都要平滑：圆门边框希望清晰时，`kDoor` 可以为 0 并退回 hard `min`；土丘
接自然地面才是最需要 smooth blend 的 seam。必须显式 guard `k <= 0`，不要让 polynomial
公式除以零。

## 3. IQ 当前列出的八种 smin：性质和取舍

IQ 当前的 “A list of Smooth-minimums” 给出 exponential、root、sigmoid、quadratic、cubic、
quartic、circular 与 circular geometrical 八个例子；随后把多数例子归纳为 Direct Difference
（DD）family，并把具有有限 difference support 的 rigid kernel 归为 Clamped Differences
（CD）family。[IQ *A Study on Smooth-Minimums*](https://iquilezles.org/articles/smin/)

下面只展开影响本项目选择的性质；公式都采用 IQ 当前归一化的 caller-facing `k`：

| 变体 | IQ 当前公式 / kernel | 关键性质 | 对本项目的判断 |
|---|---|---|---|
| exponential | `-k*log2(exp2(-a/k)+exp2(-b/k))` | non-rigid、field 影响无界；可结合，顺序无关 | 多输入有优势；单 hill 不值得承担全局 distortion 和数值稳定处理 |
| root | 令 `r=2k`，`0.5*(a+b-sqrt((b-a)^2+r^2))` | DD、non-rigid、非结合 | 不选：不满足局部 terrain modifier 的 rigidity |
| sigmoid | 令 `r=k*log(2)`，`a+(b-a)/(1-exp2((b-a)/r))` | DD、non-rigid、非结合；`a≈b` 时要处理数值极限 | 不选：比 polynomial 难稳定，且无本场景收益 |
| quadratic polynomial | 令 `r=4k`，`h=max(r-abs(a-b),0)/r`，`min(a,b)-h²r/4` | CD、rigid、保守、非结合；`abs(a-b)>=4k` 精确退回 min | **首选**：便宜、有限 difference support、`k` 是明确的最大 inflation |
| cubic polynomial | 令 `r=6k`，`min(a,b)-h³r/6` | CD、rigid、保守、非结合；C2 | 若 quadratic 的二阶变化产生可见 lighting artifact，再 A/B 它 |
| quartic polynomial | 令 `r=16k/3`，`h=max(r-abs(a-b),0)/r`，`min(a,b)-h³(4-h)r/16` | CD、rigid、保守、非结合 | 更高阶候选；第一版没有证据值得增加复杂度 |
| circular（CD kernel） | 令 `r=k/(1-sqrt(1/2))`、`h=max(r-abs(a-b),0)/r`，再从 `min(a,b)` 减 `r*(1+h-sqrt(1-h*(h-2)))/2` | CD、rigid、保守、非结合；垂直形体可产生准确圆弧连接 | 明确追求圆弧截面时再比较，不作为默认首选 |
| circular geometrical | `max(r,min(a,b))-length(max(r-vec2(a,b),0))`，`r=k/(1-sqrt(1/2))` | local support、可结合，但会在部分区域高估距离，非保守 | **不要**用于要求稳健 raymarch/collision 的 canonical field |

上述 rigidity、locality、conservative 与 associativity 分类来自 IQ 的 “The DD Family”、
“The CD Family”、“Regions of distance under/over-estimation”与“Properties”。要特别区分：CD
kernel 会在输入距离差足够大时恢复原形，但其低估距离的 Voronoi band 仍可延伸很远；这不等于
整个组合 distance estimator 具有严格空间 local support。circular geometrical 才有 local
support，但它以失去 conservative safety 为代价。

**【历史注意】IQ 旧版文章还列过 power 形式：**

```slang
float pa = pow(a, n);
float pb = pow(b, n);
return pow((pa * pb) / (pa + pb), 1.0 / n);
```

它对一般 signed field 跨 0 并不稳妥：非整数幂遇负值没有实数结果，偶数幂会丢符号。这个限制
由公式直接得到；它也已不在当前八个主实现中，因此不作为 Re: Flora terrain boolean 候选。

**【事实】IQ 当前 “Properties” 指出 exponential 与 circular geometrical 可结合，CD/DD 的
其他成员一般不满足 associativity。** 因而如果以后一次合并大量相互重叠的 procedural blobs，
不应假设逐项 polynomial fold 与 primitive 顺序无关。
[IQ “Properties”](https://iquilezles.org/articles/smin/)

**【建议】当前只有 `natural terrain + one hill`，polynomial 的非结合性没有实际障碍。** 若以后
一座土丘由许多 metaball/blobs 组成，要么固定并记录组合树顺序，要么评估 N-ary exponential，
不能靠容器遍历顺序决定地形。

## 4. 梯度、法线与材质不是自动解决的

### 4.1 smooth result 通常不再是精确 SDF

**【事实】对 quadratic polynomial smin，IQ 推导出的梯度是输入梯度的线性混合。即便两个输入
都是精确 SDF、梯度长度都为 1，混合梯度通常小于 1，所以结果不再是精确 signed distance。
它仍是保守 field，但等距层不再保持单位间隔。**
[IQ “Gradients”与“Gradients of the DD and CD family”](https://iquilezles.org/articles/smin/)

**【事实】Re: Flora 当前 `qTerrain = surfaceHeight(x,z)-y` 本身也通常不是精确 Euclidean SDF：**

```text
grad(qTerrain) = (dh/dx, -1, dh/dz)
|grad(qTerrain)| = sqrt(1 + |grad(h)|²)
```

除平地外其梯度长度大于 1。当前 `chunk_init.slang` 只检查 field 的符号来生成 voxel occupancy，
所以这不妨碍 atlas 初始化；但不能把这个 field 无条件交给 sphere tracing 并声称它给出安全的
真实距离。

**【建议】第一版应把 smooth field 当作“用于确定 voxel 内外的隐式标量场”，采样后仍由现有
voxel/Surface/Contree 管线负责可见几何和法线。** 不要同时把它升级成新的 runtime collider
SDF 或 raymarch distance estimator；后者需要单独验证 Lipschitz bound、步长和梯度。

### 4.2 材质必须随几何一起定义

**【事实】IQ 给出的 polynomial smin 可以同时返回 mix factor；这个 factor 用来让两个形体的
材质在同一 transition band 内混合。对当前归一化的 quadratic 版本，原文权重等价于：**

```slang
float h = 1.0 - min(abs(a - b) / (4.0 * k), 1.0);
float w = h * h;
float materialBWeight = (a < b) ? 0.5 * w : 1.0 - 0.5 * w;
```

[IQ “Mix factor”](https://iquilezles.org/articles/smin/)

**【建议】Re: Flora 第一版不要把连续 mix factor 硬塞进现有 `R8_UINT` voxel type atlas。** 当前
atlas 的核心输出是离散 voxel type；土丘可以统一写 Dirt/rock depth policy，stucco house shell
再由现有 authored edit 覆盖。若 future visual 要在 grass/soil/stucco 间连续混合，应先设计
独立权重/材质层及其 persistence/edit 语义，而不是用距离较小者直接抢 material ID。直接选择
winner 会在几何已平滑处留下材质硬 seam。

### 4.3 noise 只提供细节，smooth boolean 才负责接缝

**【事实】当前自然 terrain 的高度来自三层 classic gradient-noise fBm：base、detail、fine；
它不是 `fastnoise_lite::Perlin` 路径。** 见
[`chunk_heightmap.slang`](../shader/slang/chunk_heightmap.slang) 与
[`gradient_noise.slang`](../shader/slang/gradient_noise.slang)。

**【建议】山丘形体先用可控的 ellipsoid/capsule/rounded profile 决定大轮廓，再用低振幅、低频
noise 对 hill field 或 hill height 做 displacement。** 仅把一张 noise 相加不会自动消除
terrain/hill 的 CSG 折线；反过来，smooth max 本身只生成 fillet，也不会凭空产生侵蚀、土层或
植被的自然细节。两者职责应分开：

```text
authored hill macro shape
    -> bounded noise displacement
    -> smooth union with natural terrain
    -> cavity / door subtraction
    -> voxel material classification
```

## 5. 当前 Re: Flora 地形链路的接入点

### 5.1 已确认的现状

**【事实】启动地形现在是二阶段 GPU 初始化：**

```text
chunk_heightmap.slang
    computeSurfaceHeight2D(worldXZ)
    -> surface_height / base_shape
chunk_init.slang
    density = surfaceHeight - worldVoxelPosition.y
    -> density < 0 ? Empty : Dirt/Rock
    -> chunk_atlas
```

相关 source 是 [`chunk_heightmap.slang`](../shader/slang/chunk_heightmap.slang)、
[`chunk_init.slang`](../shader/slang/chunk_init.slang) 与记录 dispatch 的
[`plain/mod.rs`](../src/builder/plain/mod.rs)。`computeIslandShape2D()` 当前虽然计算出来，
`computeSurfaceHeight2D()` 最终只返回 `baseShape`；实现时不应误以为 island mask 已参与高度。

**【事实】房屋场景目前不是 terrain generator 的一部分。** 它先读取 footprint 下自然 terrain
高度，再以 cuboid voxel edits 写 floor、stepped gables、two roof panels，最后以 Empty cuboids
挖圆门。见 [`house_scene.rs`](../src/app/core/house_scene.rs)。这解释了为什么现有 A-frame 屋顶是
独立“积木形状”，而不是山体的一部分。

**【事实】可见 terrain 的 canonical representation 是 voxel atlas；edit 后 Surface -> Contree
同步重建并在下一帧整体可见。碰撞/水体 SDF 等 non-visual consumer 从 atlas 派生并可延后，
但不能以独立 render mesh 取代 atlas。** 见
[`terrain_visual_rebuild_pipeline.md`](terrain_visual_rebuild_pipeline.md) 与
[`voxel_collision_architecture.md`](voxel_collision_architecture.md)。

### 5.2 最小可行方案

**【建议】先做一个有界 `HobbitHillDesc`，在 house scene 启动时将 hill field 采样/写入 atlas，
然后复用现有房屋 shell 与 Empty carve。** 边界至少包含 center、ellipsoid radii/profile、
blend width、noise seed/frequency/amplitude 和材质 policy。它必须只影响 house 周围的 bounded
AABB，避免为一个局部房子重写整个地图生成架构。

若只需要图片中“地面隆起后覆盖房屋”的单值曲面，最窄实现甚至不必先引入通用 3D CSG graph：

```slang
float naturalHeight = computeSurfaceHeight2D(xz);
float hillHeight = computeHobbitHillHeight2D(xz); // footprint 外返回很低值
float coveredHeight = smaxQuadratic(naturalHeight, hillHeight, blendWidth);
float qCovered = coveredHeight - y;
```

因为两项都减同一个 `y`，这等价于对两个 solid-positive height fields 做 smooth max。优点是改动
最小、土丘天然没有 overhang；缺点是无法表达真正 3D 的洞穴、回卷屋檐或垂直入口，内部空腔
仍要通过后续 voxel carve。

如果目标很快会扩展到多个洞室、烟囱孔、门廊 tunnel 或 overhang，则直接构造 3D field 更稳妥：

```text
natural height field -> qTerrain(x,y,z)
ellipsoid/capsule     -> qHill(x,y,z)
interior primitive    -> qInterior(x,y,z)
round-door/tunnel     -> qDoor(x,y,z)
```

两条路径最终都应采样进同一个 atlas，再走现有 publication/rebuild。选择标准不是“哪种数学更酷”，
而是所需轮廓是否仍为单值 height：只要每个 `(x,z)` 只有一个顶面，高度场足够；出现 overhang
或内部由 field 直接表达时，才需要完整 3D SDF composition。

### 5.3 单位、离散化与 seam 验收

**【事实】现有 terrain shader 以 `1 / 256` 把 atlas voxel coordinate 转换成 world unit；房屋
代码也注明一个 world unit 等于 256 terrain voxels。** 所以 `k` 必须与被混合 field 同单位。
例如希望当前 normalized-polynomial 的最大界面偏移约为 8 voxels，若 field 是 world unit，
则 `k = 8/256 = 0.03125`；若 field 直接用 voxel coordinate，则 `k = 8`。对应的完整
field-difference blend band 是 `4k`。不能把两种单位的 distance/density 直接做 smin/smax。

**【建议】normalized caller-facing `k` 的第一轮探索用 4、8、16 voxel 三档（quadratic 的
field-difference support 分别为 16、32、64 voxels），固定 seed、camera 和 house transform，
同时检查：**

- 土丘与原地面的 silhouette/normal 是否无硬折线；
- 门洞、室内净空是否没有被 smooth union 回填；
- Dirt/rock/stucco 边界是否符合 material precedence；
- 受影响 chunks 是否在同一 Visible Terrain Publication 中发布；
- atlas 派生 collision、水体 terrain dependency 与 DDGI 是否在对应 revision 后更新；
- release hidden run 的日志无 shader、rebuild、water 或 DDGI error。

由于最终几何是 voxelized 的，`k` 明显小于 1 voxel 时几乎不会产生稳定可见差别；`k` 很大则会
让山丘膨胀并回填入口。blend width 应以“可观察到的 seam 消失但门廊仍保持净空”为选择条件，
而不是只凭公式名决定。

## 6. 尚未确定、需要下一阶段回答的问题

1. **最终轮廓是否需要 overhang？** 参考图的草坡本身可用 height field；深门廊或洞穴式入口
   是否要由 terrain field 直接承担，会决定 2D height blend 还是 3D composition。
2. **原 A-frame 的哪些 stucco/wood 构件保留？** “去掉三角房梁”可能只删除可见 gable/roof，
   也可能保留隐藏承重 shell。terrain union 不能替代室内防漏与材质设计。
3. **土丘应该仅属于 `--house-scene`，还是进入默认世界？** 前者适合有界 scene edit；后者才值得
   把 descriptor 接到通用 startup generator。
4. **连续材质 blend 是否真有需求？** 当前离散 voxel type 足以让 dirt 覆盖 stucco；若希望
   moss/grass/soil 连续过渡，需要新增材质 ownership，不是 smin 一个函数能解决。
5. **入口 carve 的平滑度。** terrain/山丘接缝适合较大的 `kGround`，室内和圆门应独立使用更小
   `k` 或 hard subtraction，不能共享一个全局 smoothing 参数。

## Sources

- Inigo Quilez, [*A Study on Smooth-Minimums*](https://iquilezles.org/articles/smin/) — 八个
  当前示例、归一化、DD/CD families、mix factor、解析梯度、保守性与结合性。
- Inigo Quilez, [*3D distance functions*](https://iquilezles.org/articles/distfunctions/) —
  negative-inside primitive SDF 与 union/intersection/subtraction 运算约定。
- Inigo Quilez, [smooth-min implementations demo](https://www.shadertoy.com/view/DlVcW1)、
  [material blending demo](https://www.shadertoy.com/view/MXfXzM)、
  [analytical-gradient demo](https://www.shadertoy.com/view/tdGBDt) 与
  [snow/bridge organic blend](https://www.shadertoy.com/view/Mds3z2) — 作者本人的可运行代码与效果。
- Inigo Quilez, [*Rendering Worlds with Two Triangles*](https://iquilezles.org/articles/nvscene2008/rwwtt.pdf) —
  distance-field rendering、有限差分梯度法线以及将 noise gradient 用作表面细节的作者资料。
- Re: Flora [`chunk_heightmap.slang`](../shader/slang/chunk_heightmap.slang)、
  [`chunk_init.slang`](../shader/slang/chunk_init.slang)、
  [`house_scene.rs`](../src/app/core/house_scene.rs)、
  [`terrain_visual_rebuild_pipeline.md`](terrain_visual_rebuild_pipeline.md) 与
  [`voxel_collision_architecture.md`](voxel_collision_architecture.md) — 当前项目事实。
