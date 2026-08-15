# DDGI 探针层接缝修复计划

## 状态

本文记录 saved-terrain 场景中 DDGI 探针层亮度接缝的诊断与实施。当前状态为**生产修复已完成
并通过验收**。最终 Shader/Rust 修复提交为 `41a89a6f`；太阳摄入路径的一手资料研究提交为
`d8d96931`。

最初计划基于分支 `agent/ddgi-seam-repro` 在提交 `38cc5dda` 时的证据。复现输入、相机、
光照和截图流程由 [saved-terrain 复现文档](ddgi_saved_terrain_probe_seam_repro.md) 固定；
背景研究见 [DDGI 探针接缝研究](references/ddgi/ddgi_probe_seam_research.md)，太阳路径的
源码与官方算法对照见
[DDGI Probe 直接太阳光摄入路径研究](references/ddgi/ddgi_probe_direct_sun_path.md)。

## 2026-08-09 最终结论与实施结果

### 最终根因

用户提出的“蓝天采样是否偶然踩到太阳”假设已经被源码和 matched A/B 否定。DDGI miss
只读取 authored sky 渐变与宽 halo，不读取太阳盘、`sun_luminance` 或太阳贴图。太阳能量
来自 Probe ray 命中表面后显式发出的 exact sun shadow ray；它表示
`Sun -> surface A -> receiver B` 的 diffuse bounce，是 DDGI/RTXGI 要求的间接光种子。

把 `sun_luminance` 从 `1.65` 单独改成 `0`、同时保持 sun direction 与 sky model 不变后，
`exact-irradiance` 从高对比 RED 变为 `contrast=2.762, bands=0`。因此 direct-sun bounce 是
该 fixture 的必要高对比激励源，但不是应删除的错误路径。把 Probe-hit sun shadow origin
改成精确 hit position 仍为 RED；增加 Probe ray rotation、改 spacing 或只换某一个旧空间
权重也没有解决问题。

决定性现场证据来自 S2 irradiance capture：固定墙面列上的每一个亮度台阶都与一个 terrain
voxel Y row 切换一一重合。旧 terrain consumer 把同一 voxel 的所有像素锁到同一个
canonical surface receiver，并把这个量化坐标同时用于 Probe position 与 surface-side
spatial weight。一个本来连续的、由强太阳反弹形成的竖直 irradiance 梯度因此被重建为
“每个 voxel 一个常数”的水平楼梯。旧 relocation-aware position formula 还存在已确认的
nominal cell-face 不连续，令这个楼梯更明显；但只换 nominal trilinear、仍使用 voxel-center
坐标并不能消除条带。

最终因果链为：

```text
Probe hit 上合法的 direct-sun bounce
  -> 墙边相邻 Probe 中形成高对比但可连续重建的 irradiance 梯度
  -> terrain consumer 把 spatial basis 量化到 canonical voxel receiver
  -> 旧 relocation-aware basis 又在 nominal cell face 不连续
  -> 连续梯度变成每个 terrain voxel 一阶的水平亮度台阶
```

### 修复边界

修复把 terrain DDGI query 的一个位置拆成两个职责明确的坐标：

- `receiverWorldPosition` 仍是 canonical voxel surface；moment visibility、exact voxel
  visibility、support distance、terrain invalidation、probe state 与 canonical zero/nonzero
  分类继续使用它，保持原有防漏光边界；
- `positionWeightWorldPosition` 使用 camera ray 的连续 `result.position`；只有 nominal
  trilinear position weight 和 surface-side spatial weight 使用它，从而连续重建 Probe 场；
- 若连续 spatial basis 在某个像素得到零贡献，仍回退到同一 voxel 的 canonical result，
  防止一个 voxel 内出现黑/亮三角裂缝；
- Probe trace、direct-sun transport、atlas、材质、默认 spacing 和最终 direct VSM 均未改变。

历史实现为了保留当时 terrain receiver cache 的性能，为八个 cage corners 保存 canonical
`hard * moment` visibility，并以 8 个 UNORM16 打包进一个 `uint4`。该 cache 已于
2026-08-16 在 Moment-only consumer 迁移后完整移除；当前 terrain 直接用精确 surface
position 对已发布 DDGI field 做 smooth Moment query。seam 修复保留的职责拆分与 fallback
语义不依赖这个 cache。

### 症状与正确性验收

analyzer 同时增加了“窄台阶”判定：局部 gradient 必须足够集中且 half-max width 不超过
20 px。这样宽而连续的合法光照坡度不再被当成条带；新的 synthetic sine-gradient 测试保护
该边界。旧 baseline 在新指标下仍为 RED（14 个窄台阶），所以这不是通过放宽指标隐藏问题。

| 验收项 | 修复前 | 修复后 |
| --- | ---: | ---: |
| saved `exact-irradiance` | RED，14 个窄台阶 | 连续两次 GREEN，0 个窄台阶 |
| 两次 exact contrast | `33.584` 左右 | 两次均 `34.384` |
| 两次 exact 主边缘 | 保留 | 两次均 row `544`、gradient `0.129721` |
| cached normal view | RED，10 个窄台阶，contrast `20.772` | GREEN，1 个孤立峰，contrast `21.107` |
| walls spacing 32 canonical mixed-zero receiver voxels | `0` | `0` |
| walls spacing 32 combined mixed-zero receiver voxels | `0` | `0` |

完成的自动验证：

- `cargo fmt --check`、`cargo check`、`cargo test`：413 passed，1 ignored；
- `python3 -m unittest scripts.tests.test_analyze_saved_ddgi_seam`：3 passed；
- hidden muted release：成功退出，无 ERROR、panic、Vulkan validation 或 non-finite；
- `scripts/check_ddgi_correctness.sh`：sealed/portal/walls × spacing 32/16，6/6 PASS；
- `scripts/check_ddgi_transport_acceptance.sh`：S0/S1/S2/converged、forward/reverse、
  donor/dogleg、内嵌 correctness、runtime terrain edits、sky normalization 与 lifecycle
  全部通过，最终 `failures=0`；
- 真实 final/cached normal screenshot 由当前 release binary 重新捕获并得到 GREEN。

历史 `scripts/check_patt_ddgi_seam_repro.sh` 没有进入渲染：它仍要求相机 snapshot `patt`，
而提交 `db0a2b76` 已把配置替换为当前唯一的 `snapshot`。这是现存 fixture 漂移，不是本修复
产生的图像失败；本次没有擅自恢复或覆盖用户的相机配置。

### 性能与资源

同一 saved terrain、自动 present mode、2880x1620 hidden release、只统计 frame >= 1000：

| GPU scope | baseline median / p95 | fixed median / p95 | median delta |
| --- | ---: | ---: | ---: |
| `frame.render` | `1894 / 3432.2 us` | `1932 / 2422.3 us` | `+38 us` (`+2.0%`) |
| `tracer.render` | `663 / 1364.6 us` | `699 / 723.6 us` | `+36 us` |
| `tracer.pass` | `334 / 390.4 us` | `371 / 395.6 us` | `+37 us` |

整帧 mean 从 `2023.0 us` 到 `2015.5 us`；P95 的下降属于该场景调度噪声，不作为性能收益
宣称。可归因的稳定成本是 main tracer 约 `37 us`。当时 terrain DDGI cache 从 32 MiB
增至 48 MiB，即额外 16 MiB GPU memory；该 48 MiB allocation 后来已随 receiver cache
一并删除。生成文件和 `config/gui.toml` 均未变化。

## 2026-08-07 诊断进展

本节记录从阶段 0 到阶段 2、以及 terminal-field readback 后得到的现场结论。它取代了
“只根据历史截图描述现象”的状态，但不宣告生产修复完成。相关诊断提交依次为：
`8c8e2fbf`、`4d8110df`、`e8b2d243`、`e8fc61ec`、`46bfae09`。

### 现场复现与单变量结果

固定 saved terrain、camera、lighting、32-voxel spacing 和收敛后的 hidden release capture
仍能稳定得到 RED。四个空间权重候选的 analyzer 结果如下：

| 候选 | position weight | surface-side weight | contrast | primary row | bands | ratio |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| A | relocation-aware | hard squared-dot | 33.584 | 493 | 24 | 1.961422 |
| B | nominal trilinear | hard squared-dot | 34.416 | 544 | 23 | 4.280742 |
| C | relocation-aware | wrap-style | 30.077 | 493 | 29 | 0.640614 |
| D | nominal trilinear | wrap-style | 31.021 | 544 | 29 | 2.407828 |

把 surface-side weight 单独置为 `1` 也没有得到 GREEN：relocation/current 为
`contrast=28.336, bands=25, ratio=0.512488`，nominal 为
`contrast=29.411, bands=28, ratio=1.904084`。因此 surface-side weight 会改变严重程度，
但不是充分根因。

### Terminal readback 证据

固定六个 receiver 的 readback 来自 `serial=6, geometry_revision=0, radiance_revision=1,
spacing=32, stage=Converged, iteration=5`。原始 cell-face 附近的六点中，八个 probe 均为
valid/trustworthy，`rejection_flags=0`，hard visibility 和 moment visibility 均为 `1`；
没有观察到 invalid、support、surface-side 或 visibility gate 导致的候选集跳变。

在 nominal cell face 两侧，旧 relocation-aware 聚合权重从约 `0.477975` 跳到 `0.523814`，
而 nominal 聚合权重约为 `0.500026` 到 `0.497683`。probe index 会按 nominal cage 正常切换，
但旧 position weight 在共享面上产生了额外的不连续。另一组 analyzer 层带附近的 readback
同样保持八个 valid probe 和稳定候选集；这说明 H3 在已采样位置没有被观察到，但尚未对整张
截图的所有 cell face 做穷尽证明。

### CPU 数学检查与当前边界

对合法 relocation（shared probe 沿轴移动 `+0.1` 个 spacing）做的一维检查显示，旧公式在共享
cell face 的 shared-probe weight 为 `1.0` 与 `0.9090909`，用非恒定 probe value 采样时对应
`10.0` 与 `9.0909091`；ordinary nominal trilinear 在两侧保持连续。这个结果确认旧
`ddgiRelocationAwarePositionWeight` 没有满足计划要求的 cell-face continuity contract。

但是，把 nominal basis 临时切入普通生产 consumer 后，真实 `exact-irradiance` 仍为 RED：
`contrast=34.417, primary_row=544, bands=22, ratio=4.291830`。因此：

- H1（旧 relocation-aware 公式违反连续性契约）已被数学和现场 readback 证实；
- H2（surface-side 权重单独造成条带）未被 no-surface 对照支持；
- H3 在固定 readback 点没有观察到 gate/candidate-set 跳变，但不能据此宣告全局排除；
- nominal trilinear 是连续性候选，不是已验收的生产修复；
- 当前没有提交生产 shader/Rust 公式修复，未改变 DDGI transport、atlas、材质或 spacing。

下一步应把正确的一维/三维 continuity oracle 与真正被选择的公式放在同一个修复提交中，
并继续检查所有投影 cell face 的一阶变化或更平滑的空间 basis；`exact-irradiance` 与
`normal` 必须共同达到 GREEN 后才进入阶段 5/6。

## 目标

消除 saved-terrain 固定视角中远离真实投影光边缘的水平探针层亮度带，同时满足：

1. `exact-irradiance` 与最终 `normal` 视图中的内部条带消失；
2. 屋顶开口形成的细长主光束和真实明暗边界保留；
3. 不重新引入薄墙、屋顶或封闭房间漏光；
4. 不改变 DDGI 传输、探针 atlas、材质、太阳注入或默认探针间距，除非后续证据明确
   把根因指向这些部分；
5. 形成一个能持续阻止同类空间插值回归的确定性契约，而不只修补当前截图。

## 已确认的证据

所有数值均来自相同 saved terrain、相机、光照、32-voxel 探针间距和收敛后的 hidden
release 捕获。当前分支已经得到：

| 视图 | 保留或移除的因素 | 结果 |
| --- | --- | --- |
| `exact-irradiance` | 当前完整空间权重和强制完整可见性 | RED，24 个内部条带，ratio `1.961422` |
| `unoccluded-irradiance` | 移除 moment 与 exact consumer visibility | RED，24 个内部条带，ratio `1.961422` |
| `equal-weight-irradiance` | 移除 position 与 surface-side 权重幅值，保留可信度和 support gate | GREEN，1 个内部条带，ratio `0.067848` |
| `raw-cage-irradiance` | 直接平均当前 nominal cage 中的有效 atlas tile | GREEN，1 个内部条带，ratio `0.089716` |

这组结果支持以下边界：

- 最终 direct VSM 合成不是 `exact-irradiance` 条带的原因；
- moment visibility 和 exact consumer visibility 不是生成条带的必要条件；
- 局部原始 probe irradiance atlas 本身没有同样的内部水平条带；
- 条带由当前 position/surface-side 权重幅值与稀疏、已 relocation 的探针场共同暴露；
- 强太阳使相邻探针的差异更醒目，但目前不能把它称为根因。

当前 spacing 32 的 relocation 日志还显示：4,913 个探针中 3,876 个有效，1,372 个发生
移动，最大允许位移为 16 voxels。该事实说明实际采样位置与 nominal grid 差异很大，但
不能单独证明 relocation 算法错误。

## 工作诊断与可证伪预测

### H1：relocation-aware position weight 破坏 cell-face 连续性

优先级：最高。

当前 `ddgiRelocationAwarePositionWeight` 直接移植了紧凑的 offset 公式，但没有覆盖以下
不变量的回归测试：端点权重、单个权重范围、权重和、零 relocation 等价性，以及相邻
nominal cell 面两侧的连续性。仓库自己的 Roháček 阅读笔记明确要求先用论文 Figure 6
的一维案例验证这些条件。

预测：保持当前 surface-side、visibility、support 和 probe 数据不变，只把 position
weight 换成已证明连续的候选后，saved-terrain 内部条带指标会变为 GREEN。

### H2：surface-side 权重过于尖锐

优先级：第二。

当前实现使用 `max(0, dot(normal, surfaceToProbe))^2`，并在接近零时直接拒绝探针。
RTXGI 参考实现使用带正偏置的 wrap-shading 曲线，目的之一就是避免所有候选因背面权重
同时接近零后，被归一化放大成不稳定区域。

预测：保持当前 relocation position weight 不变，只换成连续的 wrap-style surface
weight 后，条带明显减少或消失。

### H3：nominal cage、actual position 与 hard gate 的组合改变有效候选集

优先级：第三。

查询用 nominal grid 选择八个角点，却用 relocation 后的位置计算方向和距离，同时还会
应用 valid、position epsilon、surface-side epsilon 与 support-distance gate。相邻层的
候选集合可能因此改变。

预测：在条带上方、内部和下方记录八个贡献项时，probe index、`trustworthy` 或
contributing count 会在条带处跳变。因为 `equal-weight-irradiance` 仍保留这些 gate 却
已经 GREEN，所以 hard gate 单独成为根因的可能性低于 H1/H2。

### H4：32-voxel 稀疏场的质量上限

优先级：第四。

如果连续权重仍保留条带，而条带世界尺度随 spacing 32/16/8 成比例变化，并且可接受的
更密采样能稳定消除它，那么问题应被归类为分辨率/质量边界。

在 H1-H3 没有被证伪前，不通过提高密度、降低太阳亮度、改材质或模糊结果来掩盖条带。

## 非目标

- 不在本修复中改写 DDGI probe trace、反馈传输或 direct-sun transport。
- 不改变 irradiance/visibility atlas 分辨率、格式、filter 或 gutter。
- 不恢复全屏 temporal/A-Trous radiance denoiser。
- 不通过正 visibility floor、global-sky fallback 或 albedo 调整掩盖零权重区域。
- 不把调高探针密度作为默认修复。
- 不为了调试自动启动可见窗口。
- 不手工编辑 `cargo check` 生成的 Rust 文件。

## 阶段 0：恢复权威复现输入

当前工作树中没有 `saves/terrain_snapshot.rflterrain`，历史捕获目录
`target/ddgi-seam-repro` 也不存在。开始任何渲染器修改前，必须恢复完全相同的输入：

- 路径：`saves/terrain_snapshot.rflterrain`；
- 大小：`134,218,112` bytes；
- SHA-256：`c1994a9bb602a2d172545c85ae17ba5e72346aedb340f31575b60cf8170ece72`；
- 相机、光照、ray-origin offset 和 Hand 工具状态继续由复现文档固定。

恢复后先运行现有三视图复现，确认日志包含 terrain hash 对应输入、相机 snapshot、
DDGI converged/ready、截图保存和成功退出，并且没有 `ERROR` 或 panic。

进入下一阶段的门槛：从当前代码重新捕获的 `exact-irradiance` 必须由
`scripts/analyze_saved_ddgi_seam.py` 判为 RED。不能只使用历史文档中的数值代替现场
基线。

## 阶段 1：把复现收紧为单命令反馈环

现有 `scripts/repro_saved_terrain_probe_seam.sh` 一次运行三个独立 release capture，适合
完整验收，但不适合每次权重实验。增加一个窄入口，或给现有 runner 增加受测试的单视图
选项，使一条命令完成：

```text
release hidden exact-irradiance capture
→ 固定 center crop
→ analyze_saved_ddgi_seam.py
→ RED/GREEN 退出码
```

反馈环必须满足：

- 使用真实 saved terrain 和真实 shader 查询路径；
- 固定 camera、lighting、spacing、截图延迟与 ROI；
- 在当前实现上实际运行并稳定得到 RED；
- 输出捕获路径、运行日志路径、internal band count、ratio、主边缘位置和梯度；
- 失败时区分输入缺失、应用失败、截图失败、分析失败和指标 RED。

提交边界：只提交 runner/checker 和它的轻量确定性测试。建议提交信息：
`test: tighten saved-terrain DDGI seam gate`。

## 阶段 2：拆分 position 与 surface-side 权重

`equal-weight-irradiance` 同时移除了两个权重幅值，仍不足以判断 H1 和 H2 谁是主因。
增加诊断候选，但每次只改变一个变量：

| 候选 | Position weight | Surface-side weight | 目的 |
| --- | --- | --- | --- |
| A | 当前 relocation-aware | 当前 hard squared-dot | 现有 RED 基线 |
| B | ordinary nominal trilinear | 当前 hard squared-dot | 单独检验 H1 |
| C | 当前 relocation-aware | RTXGI-style continuous wrap | 单独检验 H2 |
| D | ordinary nominal trilinear | RTXGI-style continuous wrap | 检验两者是否必须组合修复 |

四个候选使用同一 probe field，不触发重新生成不同的 terrain、camera、lighting 或 atlas
参数。每个候选均运行阶段 1 的单命令指标。

决策规则：

- 只有 B GREEN：修 position contract，不动 surface-side；
- 只有 C GREEN：修 surface-side contract，不动 position；
- B/C 都改善但只有 D GREEN：把问题定性为两个权重的交互；
- B/C/D 都 RED：再进入八贡献项 readback，不继续猜公式；
- 任一候选虽然指标 GREEN，但主光束或真实边缘消失：判为无效候选。

只有在 A/B 仍无法区分时，才为条带上方、内部和下方的固定 receiver 增加窄 readback。
每个 receiver 记录八个 probe 的：

- index 与 valid/state；
- nominal/actual position；
- position、surface-side、moment、hard visibility 与最终权重；
- support-distance 是否通过；
- sampled irradiance；
- accumulated base/final weight 和 dominant probe。

不使用逐像素日志，也不保留大范围 GPU readback。

提交边界：诊断视图/readback 与对应 CLI 测试单独提交，并在 `cargo check` 后做 hidden
release 验证。建议提交信息：`debug: split DDGI spatial weight factors`。

## 阶段 3：建立空间权重连续性契约

在应用修复前，先写一个纯数学、快速且确定性的 CPU oracle。它不替代 shader 的真实
截图门槛；它负责定义公式必须遵守的边界，hidden release 捕获负责证明 shader 实现与
玩家路径符合边界。

oracle 至少覆盖：

1. **零 relocation 等价性**：offset 为零时严格退化为 ordinary trilinear；
2. **端点条件**：一维左右 probe 的实际位置上分别得到 `(1, 0)` 和 `(0, 1)`；
3. **范围**：所有单项权重为有限数并位于 `[0, 1]`；
4. **单位和**：有效、无遮挡的候选权重和为 `1 ± epsilon`；
5. **cell-face 连续性**：在相邻 nominal cell 面的 `-epsilon/+epsilon` 两侧，用非恒定的
   线性 probe 值采样时不出现跳变；
6. **合法 relocation**：每个 probe 位移小于半 spacing 时仍满足以上条件；
7. **退化候选**：一个 probe invalid 或被 gate 拒绝时，fallback/归一化行为显式且有限，
   不产生 NaN、负 irradiance 或静默 global-sky 复活。

先让能捕捉当前错误契约的案例在工作区中失败，再实现候选修复；不要单独提交一个使
正常测试套件失败的 commit。oracle 与最终公式在同一个修复 commit 中提交。

## 阶段 4：应用最小、由证据选择的修复

首选候选是：**ordinary nominal-grid trilinear position weight + 连续 wrap-style
surface-side weight**。这是实验候选，不是预先宣布的最终答案。

无论最终选择 B、C 或 D，都保持以下边界不变：

- canonical voxel receiver 和 nominal cage 选择方式不变；
- relocation 后的 actual position 继续用于 probe direction、distance、moment visibility
  和 exact voxel visibility；
- probe state、support-distance、terrain invalidation 和 fail-closed 规则继续生效；
- probe trace、atlas 内容、transport feedback、材质和 direct VSM 不变；
- 不顺手重构 DDGI 查询模块的其他部分。

若 ordinary trilinear 被选中，需要在修复说明中明确它为何暂时取代
`ddgi_migration_plan.md` 中的 relocation-aware interpolation 设计：当前公式未满足连续性
契约，而 nominal trilinear 在合法 relocation 范围和现有 leak gate 下通过了症状与漏光
回归。不要把这一选择描述成 relocation 不再影响查询；actual position 仍参与几何关系
和可见性。

若 wrap-style surface weight 被选中，必须验证其正偏置没有让位于表面背后的 probe
重新穿过薄墙贡献。正偏置只能解决连续性，不能绕开 moment/exact visibility。

提交边界：公式、CPU oracle、必要的 shader/Rust 绑定和直接文档说明组成一个 focused
commit。建议提交信息应写明已证实的根因，而不是只写“fix seam”。

## 阶段 5：验收

### 症状验收

使用恢复后的同一 saved terrain 完成至少两次独立 clean-start capture：

- `exact-irradiance`：analyzer 为 GREEN；
- `normal`：玩家可见的水平条带消失；
- 主投影光边缘仍存在，位置不能因修复明显漂移；
- `dominant-probe` 可以保留离散 ownership 边界，但这些边界不能再对应 final/exact 的
  亮度条带；
- 两次捕获的 band count 一致，`internal_primary_ratio` 差异不超过 `0.02`；
- 修复前先记录主边缘 gradient，修复后不得下降超过 20%，除非人工检查证明旧值包含的
  正是错误条带，并把原因记录进验收结果。

不能仅以 analyzer 变 GREEN 作为完成：把整面墙压平、降低场景对比或擦除主光束也可能
让内部梯度下降，这些都属于失败。

### DDGI 正确性回归

至少运行：

```bash
scripts/check_patt_ddgi_seam_repro.sh
scripts/check_ddgi_correctness.sh
scripts/check_ddgi_transport_acceptance.sh
```

重点检查：

- patt 场景中旧的 direct-shadow receiver 问题不回归；
- roof、thin wall、closed room 和 portal 检查不出现新增漏光；
- probe state、relocation 和 atlas debug view 没有意外变化；
- terrain 与 raster consumer 仍使用同一 DDGI 查询契约；
- no-contribution cage 继续 fail closed，不回退到 global sky。

### 构建与运行验证

每个 shader/Rust 修复步骤按仓库约定运行：

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

`cargo check` 负责重新生成 shader-derived Rust structs；只提交由 source 变化自然生成的
diff。hidden run 的最新日志必须来自当前 worktree，且不得含 `ERROR`、panic、validation
错误或非成功退出。

### 性能回归

DDGI 查询位于高频 terrain/raster shading 路径。虽然本任务是正确性修复，仍需用相同
分辨率、相机、场景、ancestor 和自动 present mode 做 matched release A/B。至少比较
frame 与 tracer 的 mean/p95；默认门槛为不回归超过 3%。若差异落在噪声附近，增加样本，
不要用 debug build 或 unit test 作为性能证据。

## 阶段 6：清理与文档收口

修复验收后：

1. 删除只为单次 readback 添加的临时 buffer、CLI 和带标签调试代码；
2. 保留能长期解释 DDGI 查询的通用 debug view，前提是它们关闭时没有运行时成本；
3. 更新 `ddgi_probe_seam_research.md`，记录被证实和被证伪的假设、最终指标及日志；
4. 更新本文状态为“完成”，写入修复 commit 与验证命令；
5. 检查 `git diff --check`、工作区状态和 generated diff；
6. 提交 cleanup/docs commit，不与公式修复混在一起。

建议提交信息：`docs: record DDGI probe seam resolution`。

## 提交顺序

严格遵守“每个已验证步骤先提交，再开始下一步”：

1. `test: tighten saved-terrain DDGI seam gate`
2. `debug: split DDGI spatial weight factors`
3. `fix: <写明被证实的空间权重根因>`
4. `docs: record DDGI probe seam resolution`

terrain snapshot 和 `target/` 捕获是本地 ignored artifact，不进入 commit。任何阶段若没有
达到自己的进入/退出门槛，不继续下一个 commit。

## 风险与防护

| 风险 | 防护 |
| --- | --- |
| nominal trilinear 解决条带却降低 relocation 后的几何质量 | 同时通过 saved terrain 与 roof/thin-wall/portal 回归；actual position 继续用于方向和可见性 |
| wrap-style 正偏置重新引入漏光 | 保留 moment/exact visibility，并运行封闭空间与薄墙检查 |
| analyzer 只对当前截图过拟合 | 增加数学 continuity oracle，并保留 patt 与通用 DDGI correctness suite |
| 历史截图与当前二进制不一致 | 必须恢复 fixture 后从当前 commit 重新捕获 RED 基线 |
| 只修 debug view，final path 仍有条带 | exact 与 normal 都是 Definition of Done 的必选项 |
| 通过模糊或降低对比得到假 GREEN | 明确保留主边缘、主光束与 baseline gradient 门槛 |
| 调试改动污染生产路径 | 每个诊断入口默认关闭；最终阶段删除一次性 readback |

## Definition of Done（历史计划与最终记录）

只有同时满足以下条件才能宣告修复完成：

- 权威 terrain fixture 已按 SHA-256 恢复并从当前代码重现过 RED；
- 已用单变量实验确定 position、surface-side 或两者交互中的实际根因；
- 数学 continuity oracle 覆盖 relocation 与 cell-face 边界并通过；
- 两次 saved-terrain `exact-irradiance` 捕获均 GREEN；
- 最终 `normal` 视图不再有玩家可见条带，且主光束保留；
- patt、DDGI correctness、transport acceptance、cargo tests 和 hidden release run 全部通过；
- 没有新增薄墙/屋顶/封闭空间漏光；
- matched release 性能没有不可解释的 material regression；
- 临时 instrumentation 已删除，研究与计划文档已记录最终结论；
- 每个 validated step 已独立提交，工作区没有与本任务相关的未提交改动。

最终除历史 patt camera gate 外均已满足。`patt` runner 的阻塞发生在渲染前，原因是现有
camera config 不再包含该脚本要求的 snapshot；当前 saved-terrain 症状门槛、通用 correctness、
完整 transport、runtime edits 与 lifecycle 均已通过。该 fixture 漂移被明确保留为后续维护项，
没有用临时相机替代品把它伪造成 PASS。

## 历史输入阻塞已解除

权威 `saves/terrain_snapshot.rflterrain` 已恢复并用于全部最终捕获：大小 `134,218,112`
bytes，SHA-256 为
`c1994a9bb602a2d172545c85ae17ba5e72346aedb340f31575b60cf8170ece72`。
