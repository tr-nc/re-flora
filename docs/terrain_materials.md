# Procedural terrain materials

## 当前：已接受的逐 voxel 颜色

用户已认可逐 voxel 效果，要求只保留新逻辑。A/B checkbox 与旧模式运行时分支已删除；
Cool Dark / Warm Light Band 滑杆也已删除，固定为共享 Slang 源中的 `TERRAIN_COLOR_BAND = 0.25`。
该源码仍参与 compiled radiance-model identity，live 与 DDGI 不会各用一份不同常量。

Debug → **Terrain Material** 只保留土壤/沙地强度、岩石强度和 seed。保留用户试用时保存的
土壤强度 **0.135**；岩石 **0.32**，seed **17**。没有开关需要勾选。零强度自然得到基色，
不是保留旧模式。旧 GUI 的开关/冷暖色带字段加载时退休，不覆盖仍有效的用户设置。

palette 现为 **112 字节**（两个强度 float + seed，相对原 main 增加 16 字节），运行时
材质 identity 仅包含三个有效控制。详情与最终验证见
[逐 voxel 迭代记录](terrain_material_per_voxel.md#用户接受后的收尾2026-09-20)。

以下 A/B、五项参数、128 字节布局等描述为接受前的**历史记录**，不代表当前 UI/API。

## 历史：逐 voxel 独立颜色变化

用户视觉复评否定了连续宏观斑驳：即使 scale 最低为 8，邻近 voxel 的渐变也不是所需效果。
现改为 Dirt/Sand/Rock 各体素按 `floor(worldPosition * 256)` 和 seed 哈希取得稳定颜色，
不再插值、缩放噪声或生成岩石层理。同一 voxel 内颜色恒定，邻居独立取值；不是保证所有
邻居肉眼可辨（强度、基色和显示量化仍会影响差异），也不随相机或时间随机变化。

Debug → **Terrain Material** → **A/B: Per-Voxel Color Variation (off = original)**：
默认 off 保留 main 原效果；on 启用逐 voxel 色差。仅保留 **Soil / Sand Color Variation
Strength**、**Rock Color Variation Strength**、冷暖色带、seed。废弃尺度及倾斜控件；
旧 GUI 加载会移除它们并刷新标签，但保留强度/开关等已有值，加载不写回。

五项参数进入不可变 DDGI radiance identity，live/frozen 共用材质求值；湿度与编辑光照
策略不变。GPU palette 现为 128 字节（相对原 main 增加 32，较宏观候选减少 16）。
切换后仍需等待 DDGI 完整场发布才比较稳定间接光。

最新验证见 [逐 voxel 迭代记录](terrain_material_per_voxel.md)。这一迭代没有新的性能
测量或视觉接受；逐 voxel 高频颜色远处可能混叠，需用户复评。下文宏观斑驳设计、尺度、
八项控制、144 字节布局和旧 A/B 标签均为**被本节取代的历史记录**。

## 当前迁移状态（2026-09-19）

在 `agent/terrain-material-refresh` 从 main 基线
`d7bbee241c6880df00d71300bffa77eef1262bce` 合并固定候选
`c47050223d0394f2e36eb3074b8caa646ad5fe7a`，保留两条历史。
旧 worktree `/home/terence/code/re-flora-terrain-material` 未修改。
本阶段是**供用户视觉复评的候选**，不是性能接受、main 集成或发布。
详细迁移与本次验证见 [terrain material refresh](terrain_material_refresh.md)。

Debug → **Terrain Material** → **A/B: Procedural Terrain Material (off = original)**：
默认 unchecked 为当前 main 的原始调色板；checked 为土壤/沙地斑驳及岩石层理。
切换立即影响可见材质，并通过原有 Transport Input Step 更新 DDGI；间接光需要等待
完整 field 发布，不能把切换瞬间的旧场当作稳定 A/B 结果。旧 GUI 配置只补缺失控件，
已有参数与 main 其余默认值保留。没有改动存档、碰撞、地形结构、树木或蝴蝶路径。

以下 2026-09-06 的数值与失败 gate 全部是**历史信息，不是此次测量**。
本次不展开性能优化、不新增阈值、不因旧 +2.17% GPU 记录阻塞视觉复评。

The first implementation is a macro appearance layer for Dirt/Sand and Rock. It uses no image
textures, per-voxel appearance storage, material graph or PBR channels. Existing voxel IDs,
terrain generation, collision, saves, moisture and raster-flora materials are unchanged.

## Tune it

The debug panel's **Terrain Material** section contains:

| Control | Default | Meaning |
| --- | --- | --- |
| A/B: Procedural Terrain Material (off = original) | off | Off restores the current main palette exactly. |
| Soil Mottling Scale (voxels) | 16 | Noise lattice spacing; larger means broader Dirt/Sand patches. |
| Soil Mottling Strength | 0.35 | Maximum signed linear-RGB modulation; 0 restores that material's base. |
| Rock Layer Spacing (voxels) | 12 | Layer repeat distance along the world-space layer normal. |
| Rock Layer Strength | 0.32 | Rock's contrast, independent of the soil contrast. |
| Rock Layer Tilt (deg, world XY) | 15 | 0 gives horizontal layers; rotates their normal from +Y toward +X. |
| Cool Dark / Warm Light Band | 0.25 | 0 is brightness only; 1 adds the full subtle cool-dark/warm-light bias. |
| Terrain Pattern Seed | 17 | Stable integer seed; unchanged seed/position/parameters yield unchanged appearance. |

The existing **Voxel** color controls remain the authored midpoint colors. Scales are bounded
to 8–128 voxels, strengths to 0–0.75, tilt to -75–75 degrees and band to 0–1. Invalid/nonfinite
runtime inputs are normalized before both live and transported use. These controls are GUI
configuration, not new terrain-save fields.

## Evaluation and ownership

`TerrainMaterialParams` is the authored input. `MaterialFrameInput` conveys it into
**Authored Environment Lighting**, whose normalized palette is the single live fact used for
the visible uniform and for immutable DDGI snapshots. Existing uniforms grow by 48 bytes each;
there is no new GPU descriptor, texture, allocation-per-chunk or asynchronous resource owner.

`shader/slang/terrain_material.slang` owns the actual evaluation. Every affected terrain
consumer calls this same implementation using the global voxel center in world units. The
conversion to voxel units is the existing 256 voxels/world-unit convention. No chunk-local
wrap, camera position, frame serial, ray sample seed or time enters the pattern.

- Dirt and Sand use one 3D value-noise field: eight deterministic lattice values blended with
  quintic weights. Adjacent positions query one continuous field; this is not the removed
  four-bucket HSV selection.
- Rock uses one lower-frequency field for both a small phase warp and weak mottling, plus a
  sinusoidal layer band. It deliberately reuses that field rather than adding multiple octaves.
- The result modulates the authored linear-RGB midpoint with a continuous band. The color bias
  fades with strength. Stucco, both woods, emissive and other IDs bypass the effect.
- Each voxel still has a single appearance value. This first stage does not expose sub-voxel
  grain, derivatives, per-hit normal mapping or an extra texture sampling grid.

The earlier HTML explored two noise evaluations for its rock candidate and independent
intra-voxel grain. Neither is the production contract: this implementation uses one noise
evaluation plus `sin` for Rock, samples centers only, and orients layers in world XY.

## Rendering consumers and retained approximations

| Consumer | New procedural pattern | Existing moisture/lighting policy |
| --- | --- | --- |
| Primary terrain, including Glass primary's opaque hits | Shared field at `MarchingResult.center_position` | Existing live atlas moisture applies after the pattern. |
| Path-tracing secondary opaque hits | Same `parseTraceResult` path and center | Same retained moisture response. |
| DDGI probe transport, including Glass transport | Same field and center, from the builder's frozen palette | Existing dry-material approximation remains; live dynamic moisture is intentionally absent. |
| Glass off-screen voxel fallback | Same field at `(event.to_cell + 0.5)/256` | Existing dry, simplified ambient/direct fallback remains; no new wetness or lighting approximation added. |

DDGI deliberately does not read live atlas moisture during a multi-frame field build. Making
wetness part of transport would require a separate immutable data/revision contract; this
feature does not silently introduce that change. There is no new “DDGI uses average material
color” rule: the authored procedural field itself is evaluated identically in all consumers.

All eight controls participate in radiance identity. A change requests a Transport Input Step
and resets the applicable irradiance history; a field in flight retains its original parameters.
Compiled material sources also participate in capture model identity. The RFIRR v10 fixture
changes only its eight model-identity bytes; its format, lineage and sample payload do not change.

Raster grass/leaf material code is untouched. As before, raster consumers may receive changed
indirect illumination when the terrain's reflected radiance changes.

## Stability and verification boundaries

The material cannot animate or swim within a voxel: its inputs are camera-independent, and the
underlying field crosses chunk boundaries continuously. Macro-only scale limits avoid adding the
requested-but-not-approved sub-voxel high-frequency detail. These facts are not a universal
anti-aliasing guarantee: sufficiently distant/grazing views can still undersample spatial detail.
This stage keeps the existing production sampling/temporal pipeline and does not claim a new
ray-footprint filter. Real fixed/moving-camera checks and their limitations are recorded with the
implementation validation artifacts under `target/terrain-material/`.

Fast deterministic checks exercise production functions rather than a copied Rust/JS noise:

```sh
cargo fmt --check
cargo check
python3 scripts/run_slang_tests.py
cargo test
```

The current refresh runs the full suite without a material-specific skip; the old checkpoint's
PATT skip is historical only. The material shader test checks determinism, seed sensitivity, adjacent values, a
world/chunk boundary, live/DDGI equality, eligible IDs, exact disabled/zero-strength baseline,
finite bounded colors and the retained moisture formula. Rust tests cover sanitized GPU inputs,
GUI mapping, and all controls invalidating frozen transport without mutating the old snapshot.

For visual A/B, use the same world and camera; keep a release baseline binary from before the
change, or turn the new section's switch off and restore it after capture. Do not compare HTML
colors to game screenshots as if they included the same illumination. Performance decisions use
the release app, not CPU unit tests or the noise evaluation count.

### Historical measured first-stage result (2026-09-06; not remeasured)

The release fixed-camera comparison used `player-default`, 1600×1000, NVIDIA RTX 3060 Ti,
the unchanged `render-steady` scenario (30 seconds/run, warmup frame 600), and its original
budgets. A retained pre-change `b632b270` release binary was compared against this implementation;
SPIR-V is embedded in each binary, not loaded from the current shader source at runtime.

| Comparison | Samples baseline / candidate | Main tracer median | Whole GPU frame median | Full gate |
| --- | --- | --- | --- | --- |
| Old/new, A B B A | 511 / 510 | 474 → 479 µs (+1.05%) | 1943 → 1996 µs (+2.73%) | Not passed: GPU frame, shadow prepass. |
| Old/new, B A A B | 476 / 471 | 475 → 479 µs (+0.84%) | 2090.5 → 2130 µs (+1.89%) | Not passed: CPU total/path, shadow prepass. |
| Same binary, on/off/off/on (off is baseline) | 482 / 480 | 475 → 481 µs (+1.26%) | 2072.5 → 2117.5 µs (+2.17%) | Not passed: GPU frame and CPU render path. |

The same-binary switch isolates a 6 µs median tracer cost in this scene. It does **not** prove
zero total cost: the complete 2% GPU-frame gate is still exceeded by 0.17 percentage points in
that series. Shadow-prepass median decreased 1.54% in the switch series, so the earlier shadow
increase cannot confidently be assigned to the new noise evaluation. No shadow optimization or
budget relaxation was made. These data are an explicit performance acceptance boundary, not a
GREEN benchmark claim. The two animated research previews were paused before the second series.

Raw reports/logs are under `target/terrain-material/{perf-ab,perf-reverse,toggle}/`. The switch
series temporarily changed only `terrain_material_enabled`; the original config was restored
byte-for-byte. The `off-pair` reports both describe **disabled** runs even though the generic
runner labels them baseline/candidate. Its own off/off comparison is not a feature comparison;
`toggle/comparison.json` is the actual pooled off/on comparison.

For a static temporal isolation, both releases used no flora, particles, leaf shadows or clouds,
600 warmup frames then 64 captured frames / 63 transitions. Mean absolute 8-bit luma delta was
0.013436 → 0.013499 and the noticeable-pixel ratio 0.000056448 → 0.000055853. This did not show
an added static flicker in the sampled scene. A normal-production moving-camera run had mean
delta 0.264606 → 0.338690 and noticeable ratio 0.009075 → 0.008046; changing spatial appearance
changes frame differences, and normal flora animation was not time-locked, so these are not
isolated flicker/anti-aliasing proof. Four unedited paired keyframes are retained alongside the
64-frame report for visual inspection. No claim is made about every distance or grazing angle.

### Historical runtime gates and old candidate status

The candidate is preserved as a **committed checkpoint on `codex/terrain-material-candidate`,
awaiting user acceptance** of the disclosed performance boundary. Repository cleanup isolated
it from `main`; this checkpoint is not integration or performance acceptance. No threshold was
relaxed, no push was performed, and the unrelated acceptance readback issues below were not changed.

Historical validation artifacts below remain in the original integration checkout at
`/home/terence/code/re-flora/target/terrain-material/` and its `target/re-flora-logs/` directory.
They were retained during worktree cleanup, not discarded or claimed as fresh measurements.

- Final `cargo fmt --check` and `cargo check` pass. Full Rust validation: 873 main tests and
  4 collision tests pass, 1 ignored and exactly the documented PATT test filtered out.
  Nine Slang tests and 70 Python capture/performance tests pass.
- The required release hidden/muted 0.5-second smoke exits normally with `failures=0`.
  Log: `target/re-flora-logs/re-flora-20260906-032553.912-416467.log`.
- The Glass isolated scene passes its existing runtime checks: actual coverage 50.480%,
  50,749 fallback pixels, zero nonfinite/exhaustion pixels, GPU/reference scene-query and
  transport match. `target/terrain-material/glass.png` is a real 1600×1000 screenshot.
  Log: `target/re-flora-logs/re-flora-20260906-032655.852-416971.log`.
- The independent lighting-mode production acceptance is **RED**, not skipped or counted as
  successful secondary-path acceptance. Its readback images lack `TRANSFER_SRC` usage, and
  its analyzer rejects `raster_rgba byte length mismatch`. A fresh run of the retained
  pre-change binary reproduces both failures. The A–D captures do not override these failures.
  Candidate artifact/log: `target/terrain-material/lighting-modes.rflma{,.app.log}` and
  `target/re-flora-logs/re-flora-20260906-032627.158-416823.log`.
  Baseline artifact/log: `target/terrain-material/baseline-lighting-modes.rflma` and
  `target/re-flora-logs/re-flora-20260906-032723.922-417063.log`.

The shared production material functions and parameter/history contracts are verified; a clean
end-to-end lighting-mode acceptance still needs its separate pre-existing readback issues fixed.
