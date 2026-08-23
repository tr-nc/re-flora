# Re: Flora 树叶环境声源遮挡不稳定诊断与方案调研

> 日期：2026-08-23
>
> 基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`（`agent/leaf-audio-occlusion-research`）
>
> PetalSonic：crate `0.7.0`，对应不可变 tag commit `06d992f755fdc17a26b52a4eef97341ebe8d6e12`
>
> 初始范围：下文保留以 `0b607897...` 为基线的诊断与设计记录；“当前 0.7.0”“尚未实施”等措辞描述的是该历史切片。
>
> 证据标记：**当前代码**表示从上述基线及 PetalSonic 固定提交直接核对；**当前观测**表示本 worktree 的 release 隐藏静音运行日志；**一手资料**表示项目官方文档或作者论文；**建议**表示仍需测量和听感校准的设计起点。

## 实施闭环更新（2026-08-24）

本节记录研究之后完成的正式实现与验收，覆盖并取代下文的“尚未实施”状态，但保留原始诊断，便于审计问题是怎样从假设闭合为观测的。

- Re: Flora 集成起点：`c88a2a03ef87ef109b81f016e082a0a442fe9f1d`。
- 最终验证锁定的 PetalSonic producer：`b65ef9b56f29466dfaafb875793e04d91bf49e2a`；精确 contract 见该提交的 `docs/extended-source-routing.md`、`docs/adr/0002-capture-source-extent-per-voice.md`、`petalsonic/src/source_extent.rs`、`petalsonic/src/acoustic_propagation.rs` 和 `petalsonic/src/events.rs`。
- 验证期间只通过 Cargo 命令行临时 patch 使用 producer；仓库清单和最终 `Cargo.lock` 仍指向 crates.io `petalsonic 0.7.0`，没有提交机器路径。正式合入需要先发布或以仓库可复现方式引入上述 producer API。

### 已实现的长期模型

答案已经从“架构有解”推进到“consumer 端已实现并闭环”：**一棵树的一个 active generation 只有一个 emitter、一个 looping Voice 和一个 PCM cursor；树冠是该 Voice 捕获的、不可变的 `WeightedSamples` extent，不再是 8 个各自播放 rustle PCM 的点声源。** Re: Flora 只构造领域 descriptor，PetalSonic 负责多采样声学聚合、方向 lobes、预算保留和 DSP 平滑。

模块边界如下：

- [`src/geom/round_cone_clearance.rs`](../src/geom/round_cone_clearance.rs) 只提供 wood primitive 的 signed clearance 真值；[`src/geom/shape/round_cone.rs`](../src/geom/shape/round_cone.rs) 暴露所需几何数据。
- [`src/audio/canopy_acoustics.rs`](../src/audio/canopy_acoustics.rs) 从真实 leaf placements/sprays 构建 `CanopyAcousticDescriptor`。候选按树局部稳定八分体选择，最多 8 个；sample ID、relative power、content/phase seed 与 generation 都是确定性的，总 power 归一。输入遍历顺序不会改变布局。
- [`src/audio/canopy_audio_lifecycle.rs`](../src/audio/canopy_audio_lifecycle.rs) 拥有 generation、旧/新 layout 的有界 crossfade 和删除语义；旧 generation 的 descriptor 不因重建而移动，完成过渡后 registry 回到一个 active generation。
- [`src/audio/canopy_distributed_emitter_adapter.rs`](../src/audio/canopy_distributed_emitter_adapter.rs) 是唯一 Petal realization 边界：把 descriptor 转成 `SourceExtent::WeightedSamples` 和 `OcclusionProfile::AmbientDistributed`，每 generation 创建一个 looping Voice。Petal 类型没有泄漏到 canopy descriptor。
- [`src/audio/spatial_sound_manager.rs`](../src/audio/spatial_sound_manager.rs) 保留完整 `SpatialFrame` 的 extent/profile 语义，并以约 30 Hz 合并仅 listener pose 改变的 frame；结构性 dirty frame 立即发布。它没有树冠特例，也没有复制 Petal solver。
- [`src/audio/canopy_audio_telemetry.rs`](../src/audio/canopy_audio_telemetry.rs) 把 producer 的 voice/extent/sample/ray/cache/revision telemetry 映射为 opt-in tree/generation/sample 观测；[`src/audio/canopy_audio_diagnostics.rs`](../src/audio/canopy_audio_diagnostics.rs) 与 [`scripts/analyze_canopy_audio_diagnostic.py`](../scripts/analyze_canopy_audio_diagnostic.py) 提供固定轨迹和机器可解析验收。

每个发布 sample 均先通过 wood clearance：阈值大于 Petal source endpoint epsilon，并另加几何/体素 safety margin。固定 seed 122 的单树布局发布 8 个真实 leaf samples，最小 clearance 为 `7.598783 voxel`，没有 fallback。多树压力场景的 5 棵树共 38 个 samples，每棵仍不超过 8 个，最小 clearance 为 `2.538373 voxel`。没有 sample 朝 listener 移动，也没有通过关闭整树自遮挡来规避问题。

### Producer contract 与 telemetry 映射

PetalSonic `b65ef9b...` 的 `SourceExtent::WeightedSamples` 在构造时按 stable ID 排序并归一，最多 8 个 sample；完整 `SpatialFrame` 原子更新 emitter pose 与 extent，而 play 接受时把 extent/profile 捕获进 immutable Voice。`AmbientDistributed` 按

```text
gain[band] = sqrt(sum(normalized_power_weight * transmission[band]^2))
```

聚合直接声能量，并提供 gain floor、attack/release、Schmitt enter/exit、minimum dwell、最大 last-good age 和最多 4 个 decorrelated lobes。[PetalSonic `b65ef9b...`：`docs/extended-source-routing.md`、`petalsonic/src/source_extent.rs`、`petalsonic/src/occlusion.rs`、`petalsonic/src/spatial/processor.rs`]

consumer 现在逐 sample 记录 stable ID、归一功率、descriptor/producer world position、hit、三频 transmission 和由 Re: Flora 权威 transmission catalog 反查的 material label；逐 route 记录 visible fraction、raw/filtered 三频 gain、classification、dwell、transition、response/cache age、rays/cache hits、lobes、solve status 与 revisions。producer observation 没有材质名称，因此 material label 是基于项目材质 transmission 的确定性解释，不伪装成 producer 字段。[当前映射](../src/audio/canopy_audio_telemetry.rs) [当前材质真值](../src/builder/contree/mod.rs)

`hit=false` 必须对应 `[1,1,1]`；`Solved` 必须是当前 revision；`Retained` 可复用原 observations 与原 response revision；`Deferred` 没有新 sample response，但 renderer 保持有界 last-good，不回 unity；superseded solve 只产生 discard。analyzer 分别验证这些状态，不把 Retained 的历史 response revision 误报为 rollback。[PetalSonic `b65ef9b...`：`docs/extended-source-routing.md`、`petalsonic/src/acoustic_propagation.rs`、`petalsonic/src/events.rs`] [consumer invariants](../src/audio/canopy_audio_telemetry.rs)

### 确定性与生命周期验证

纯逻辑与几何 fixtures 已证明：

- 相同 seed、不同 leaf 输入遍历顺序得到相同 sample IDs、权重和 positions；sample 数有界、总 power 为 1；
- 所有发布 sample 均在 wood 外且达到 clearance 阈值；没有合格 leaf candidate 时使用确定性 fallback；
- generation 切换期间旧/新总功率有界，旧 descriptor 不变，crossfade 完成后旧 source 被回收，registry 回到基线；
- consumer 可从 8 个 sample observations 重建 producer aggregate，误差为 0；其中 1/8 遮挡为约 `-0.574 dB`，半遮挡为约 `-2.967 dB`，对应 weighted-energy contract，而不是 point-source 的二元 transmission。

这些不变量分别由 [`canopy_acoustics.rs`](../src/audio/canopy_acoustics.rs)、[`round_cone_clearance.rs`](../src/geom/round_cone_clearance.rs)、[`canopy_audio_lifecycle.rs`](../src/audio/canopy_audio_lifecycle.rs)、[`canopy_distributed_emitter_adapter.rs`](../src/audio/canopy_distributed_emitter_adapter.rs) 和 [`canopy_audio_telemetry.rs`](../src/audio/canopy_audio_telemetry.rs) 的单元测试覆盖。

### 10 秒因果验收

诊断固定 tree seed `122`、wind、clip phase 与 camera trajectory：forward orbit → 1 秒 hold → reverse orbit。`--mute` 只关闭物理输出；这里的行为证据来自逐 solve/sample telemetry，而不是“没有报错”或听感猜测。

单树日志 `target/re-flora-logs/re-flora-20260824-014725.048-736200.log` 的 analyzer 结果：

```text
[CANOPY_AUDIO_ACCEPTANCE] verdict=PASS mode=single trees=1 emitters=1 voices=1
samples=8 total_power=1.000000001 min_clearance_voxels=7.598783
step_domain=raw max_step_db=2.926 hold_step_db=0.000
raw_symmetry_db=2.020 filtered_symmetry_db=1.170
extent_responses=283 processed=283 retained=0 deferred=0 rays=4528
```

这条轨迹证明一个 tree/generation 在 Re: Flora registry 与 Petal runtime 中都是一个 emitter/Voice；无 voice identity、sample contract、aggregate 或 revision 违规，telemetry drop 与 render rollback 均为 0。最大 raw 单步 `2.926 dB`，hold 段为 `0 dB`，不再出现整树在 unity 与 wood transmission 间的 `20–36 dB` 二元跳变。startup 几何收敛期间有 11 个 superseded solves，但它们只被 discard，轨迹期间没有 revision 回灌。该 run 的 Petal stop summary 为 `solves=294`、`published=283`、`solve_us_max=5969`、`response_age_ms=17`。

多树预算日志 `target/re-flora-logs/re-flora-20260824-014745.263-736572.log` 的 analyzer 结果：

```text
[CANOPY_AUDIO_ACCEPTANCE] verdict=PASS mode=budget trees=5 emitters=5 voices=5
samples=38 total_power=1.000000005 min_clearance_voxels=2.538373
step_domain=filtered max_step_db=8.574 hold_step_db=0.932
raw_symmetry_db=0.000 filtered_symmetry_db=0.000
extent_responses=1340 processed=536 retained=74 deferred=730 rays=8124
```

压力场景把预算限制为 2 个 extents/solve、32 direct rays，明确观测到 `Retained` 与 `Deferred`，但没有回 unity、stale revision、telemetry drop 或 render rejected rollback。长时间 deferred 的 source 再次求解时，raw target 可因 camera 已移动而大幅变化；有声意义上的 filtered route 最大单步为 `8.574 dB`，仍有界并低于禁止的 `20–36 dB` 跳变，hold 段为 `0.932 dB`。该 run 的 stop summary 为 `solves=281`、`published=268`、`solve_us_max=9649`、`response_age_ms=27`。

producer 在同一精确 SHA 提供的 release fixture 证据为：8×8 worker p99 `40 us`；8 Voice × 3 lobe renderer p99 `1185 us`（约实时预算 `4.15%`）；32 Voice p99 `54–60 us`。这些是 producer 自己的固定 fixture 数据，并非本次 Re: Flora 机器上的重测；Re: Flora 的权威性能观测是上面两个 release diagnostic 的 solve max/response age。

### 最终判断与剩余限制

**现有这套不是没法解：这个问题已经由真实树叶采样的分布式树冠 extent、单 Voice 生命周期、Petal 聚合与有界时间响应解决。** 旧 0.7 point API 确实无法直接表达长期模型，但这只是 API 能力边界，不是声学架构死路。衍射和完整波动声学仍未实现；它们可提升硬边界与低频绕射质量，却不是修复错误 branch endpoint、自遮挡二态和 Voice 复制的前置条件。

当前剩余的交付约束是依赖发布：本分支需要 `b65ef9b...` 的 extended-source API 才能编译正式接入，而仓库不能提交本机 path dependency。producer 发布或以可复现 Git/registry 依赖进入集成分支后，应在同一 SHA 重新运行本文命令与日志 analyzer。多树 budget 场景允许真实大面积遮挡产生连续、有界变化；验收不应把所有遮挡“抹平”，也不应把故意延迟后的 raw target 差异误判为 renderer 跳变。

## 结论先行

“树干遮挡导致树叶声在玩家移动时不稳定”有一条很强、且大部分已经由代码闭合的机制链，但当前不能声称已经在运行日志中录到了音量跳变：

1. **源位置错误是已证实的。** 当前所谓 leaf audio position 不是叶片、树冠表面或树冠质心，而是生成枝干骨架的 `segment.end`。同一个 `segment` 随即生成包含该端点的 `RoundCone` 木质几何；半径还被钳到至少 `1.05 voxel`。换言之，声源按构造位于自身枝端木质几何内，而不是“偶尔被别的树干挡住”。[当前代码：树叶锚点和枝干圆台](../src/tree_gen/tree.rs#L377-L410)
2. **聚类没有修正位置。** 世界坐标只做 `/ 256 + tree_pos`；贪心聚类把第一个成员保留为 `cluster.pos`，后续归入成员只增加计数，不更新质心。因此当前 startup tree 的 7 个音频簇仍是 7 个首入簇枝端点，不是 7 个树冠代表点。[当前代码：世界坐标与聚类调用](../src/app/core/vegetation.rs#L591-L595) [当前代码：聚类位置不更新](../src/util/clustering.rs#L46-L82)
3. **当前直接声模型会把几何边界变化放大成大幅频谱/电平变化。** PetalSonic 0.7.0 对每个候选点源只发一条 listener→source closest-hit ray；有首个命中便直接使用其材质三频段 transmission，无命中则是 `[1, 1, 1]`。[PetalSonic 固定提交：direct solve](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L486-L533) Re: Flora 木材 transmission 为 `[0.08, 0.035, 0.015]`，即约 `-21.9 / -29.1 / -36.5 dB`；因此一次 hit/no-hit 或首材质分类改变远大于普通环境声应有的连续变化。[当前代码：声学材质映射](../src/builder/contree/mod.rs#L2485-L2516)
4. **源端 epsilon 不足以稳定跳出自身枝干。** PetalSonic 的 `RAY_EPSILON_METERS = 0.05`，Re: Flora 的 `distance_scaler = 15`，所以 source 端停止偏移为 `0.00333 world unit = 0.853 voxel`，小于 `1.05 voxel` 最小枝半径。[PetalSonic 固定提交：epsilon 与换算](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L11-L17) [当前代码：比例](../src/audio/spatial_sound_manager.rs#L85-L100) [当前代码：最小枝半径](../src/tree_gen/tree.rs#L11)
5. **时间处理只能软化，不能消除二元跳变。** solver 约每 33 ms 求解；direct 三频段分频点为 400/4000 Hz，目标增益只有 50 ms 对称一阶平滑。它没有遮挡分类、Schmitt hysteresis、驻留时间、衰减上限、路径缓存或环境声专用策略。[PetalSonic 固定提交：solver 周期](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L11-L17) [PetalSonic 固定提交：direct DSP](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L21-L29)

因此，**“源在枝干内 + 单射线首命中 + 极大的 hit/no-hit transmission 差”是当前最可信的主因；玩家移动使射线方向和体素命中边界变化，是不稳定的合理触发器。** 这也与环境声学一手研究的结论一致：大面积/体积内许多不相干事件叠加而成的 ambient source 被少量点源代替，会在 listener 移动时产生不真实的 loudness wobble，并错误表现阴影软化与整体衰减。[Zhang 等，*Ambient Sound Propagation*](https://www.cs.cornell.edu/projects/ambientsound/SAsia-2018-ambient2.pdf)

但要区分两种证据强度：

- **已经证实：** 锚点位于自身木几何；当前声学路径已启用；direct 模型是单射线的首命中材质透射；当前没有足以稳定这一类环境源的分类/多采样/迟滞。
- **尚待因果验证：** 在一条固定玩家轨迹上，某个具体 leaf emitter 的 raw direct gain 是否确实在 `[1,1,1]` 与木材 transmission 之间跳变，并且这是否对应用户听到的那次不稳定。当前日志没有逐 emitter 的 ray、hit/material 或 raw/filtered gain telemetry，不能用“应用无报错”代替这一步。

**推荐最小方案不是把一个点源朝 listener 拖动。** 先在 Re: Flora 把音频锚点改成稳定、listener-independent 的真实叶簇/树冠表面代表点，并用固定轨迹和新 telemetry 做 A/B；若要把这类问题从“多数情况下减轻”提升为有边界的稳定行为，则再给 PetalSonic 增加一个很小的、per-emitter 的 `AmbientDistributed` 遮挡 profile：有限个 extent/surface samples 聚合 visibility，三频段衰减设上限，带 attack/release、Schmitt 阈值和驻留时间，并丢弃已 superseded 的旧求解结果。

**长期方案**是让 PetalSonic 原生表达扩展源，而不是永久堆叠点源补丁：Re: Flora 发布不可变的 canopy extent/weighted surface samples；PetalSonic 在 listener 侧采样并聚合 direct energy 与方向分布，路径/响应按 emitter + spatial revision + geometry version 缓存和时间更新，再逐步加入物理透射厚度与衍射。现有 native environmental acoustics 已经有异步快照、三频段透射、早期反射和 late reverb 的正确骨架，**绝不是“现有这套真的没法解”；只是 0.7.0 的 point-source API 还不能完整表达这类分布式环境声。**

## 当前路径：从树生成到听者

```text
branch skeleton segment.end
        │  同一端点属于 RoundCone 木几何，radius >= 1.05 voxel
        v
Tree.relative_leaf_positions()            真正 render leaf sprays 在另一份 placements 中
        │ /256 + tree_pos
        v
cluster_positions(distance=0.08 world)    cluster.pos = 首个成员，不更新质心
        │ 取最大簇，最多 8 个
        v
looping spatial point emitters             同一 12 s rustle clip，随机 loop phase
        │
        ├── 每帧 publish complete listener + point poses
        │
        v
PetalSonic NativeAsync (33 ms solve)
        │ 每点源 1 条 closest-hit ray
        ├── no hit: direct gain = [1, 1, 1]
        └── first hit: direct gain = material.transmission
                    │ 400 / 4000 Hz 三频段，50 ms 平滑
                    ├── 最多 2 个 early reflection taps
                    └── listener-centric 3-band FDN late reverb
```

### 1. 生成与位置更新

**当前代码。** `Tree::build` 把符合 leaf level 的骨架段终点存为 `leaf_anchors`，`leaf_positions` 只是取其中的位置；真正用于画叶片的 `leaf_placements` 则由 `generate_leaf_sprays` 另行生成。随后同一批可见骨架段变成 `RoundCone`，起终点半径均不小于 `TREE_MIN_TRUNK_THICKNESS = 1.05`。[树生成](../src/tree_gen/tree.rs#L370-L415)

这一区分很关键：**渲染看到的 14572 片叶子并不代表音频锚点来自这些叶子。** startup 日志中的编译数据是 29 个 leaf anchors、15196 个生成 leaf instances，后续实际加入渲染/粒子源的是 14572 个，而音频只对 29 个枝端锚点聚类成 7 个点源。

**当前代码。** 音源空间布局在创建树时完成。`relative_leaf_positions` 只被换算成 world position，之后以 `LEAF_CLUSTER_DISTANCE = 0.08` 聚类，`add_tree_sources_from_clusters(..., false, true)` 表示不用单一 per-tree source、为每个已选簇创建 source，并随机 loop phase。[换算与聚类](../src/app/core/vegetation.rs#L591-L595) [创建音频簇](../src/app/core/vegetation.rs#L2056-L2073) [聚类距离](../src/app/core/mod.rs#L136)

**当前代码。** `cluster_positions` 的注释称代表点为 center，但实现从未在添加成员时更新 `pos`；`items_count` 只用于后续排序和音量权重。`TreeAudioManager` 最多保留 8 个最大簇，并在 `cluster.pos` 创建点源。[聚类实现](../src/util/clustering.rs#L4-L86) [选择与创建](../src/audio/tree_audio_manager.rs#L286-L313)

**当前代码。** 每个 source 的物理位置之后保持不变；风采样使用该位置，player listener 的 position/orientation 每帧更新并和全部点源一起发布为完整 `SpatialFrame`。`SpatialSoundManager::update_source_pos` 能改 point pose，但当前是 dead code。[完整 spatial frame](../src/audio/spatial_sound_manager.rs#L222-L270) [未使用的位置更新接口](../src/audio/spatial_sound_manager.rs#L353-L359)

### 2. 声源、listener 与混音语义

**当前代码。** Re: Flora 只构造 `EmitterDesc::spatial(Pose)` 或 `EmitterDesc::non_spatial()`；PetalSonic 公开的 `EmitterSpatialState` 也只有 emitter、point `Pose` 和 acoustic priority，没有 radius、shape、surface samples 或 per-emitter acoustics policy。[Re: Flora emitter adapter](../src/audio/spatial_sound_manager.rs#L118-L165) [PetalSonic 固定提交：point spatial state](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/domain.rs#L356-L378)

树叶 source 共用**同一段** 48 kHz、12 秒、固定 seed 的 resident procedural rustle clip；每个 source 只是随机 seek 到不同 loop phase。`clustered_volume_db` 又以 `10 log10(cluster_size)` 放大簇，隐含簇内事件以不相关功率相加的模型。[当前代码：clip 与 source 上限](../src/audio/tree_audio_manager.rs#L14-L23) [当前代码：clip 生成与簇增益](../src/audio/tree_audio_manager.rs#L337-L352) 不同相移可避免全部 emitter 完全同相，却不等于生成统计独立的内容；同一有限周期信号经过移动中的不同 HRTF、距离和遮挡滤波后，相关叠加仍可能使 summed level 波动。这是与位置/遮挡主因不同的**次要风险**，当前没有相关性或输出录音证据，不能将它写成已证实根因。Cornell 的 ambient/texture 模型强调 source extent 上的时空不相干，正好给出未来应验证的目标语义，而不是为当前实现背书。[Ambient Sound Propagation](https://www.cs.cornell.edu/projects/ambientsound/) [Acoustic Texture](https://research.cs.cornell.edu/ambientsound/acoustictexture/)

`TreeAudioSource` 的 wind response 自身有 attack/release 指数响应，但最终 target volume 是“达到 base wind 则 full wind volume，否则 silent”的二态门控；而 player 移动并不改变其固定位置处采样的 wind。它可能产生另一类开关，但不是几何遮挡的平滑器，也不解释“绕树移动时”的直接声频谱跳变。[当前代码：wind response 与音量门控](../src/audio/tree_audio_source.rs#L77-L152)

全局 environmental acoustics toggle 会统一开启/关闭 geometry-driven direct transmission、reflections 和 reverb；HRTF、距离、空气吸收和播放仍在。0.7.0 没有 per-emitter 或 per-bus bypass，所以不能在保留全局声学的同时只让 leaf bed 绕过 direct occlusion。[PetalSonic 固定提交：全局控制](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs#L793-L824)

### 3. native environmental acoustics 实际能力

| 能力 | PetalSonic 0.7.0 实际行为 | 对此问题的意义 |
|---|---|---|
| Direct transmission | listener→point source 一条 closest-hit ray；取第一个材质的三频段 transmission；无命中为 unity | 自身枝干边界会产生巨大状态差；没有厚度、多表面或可见比例 |
| Direct DSP | 400/4000 Hz 分为三段；50 ms 一阶对称平滑 | 能消除 sample click，但不能抑制连续 hit/no-hit chatter |
| Early reflections | listener-centric 采样，最多 2 个 taps；高质量下 8 个 source、256 rays | 能提供少量方向反射，不是遮挡替代路径或衍射 |
| Late reverb | listener-centric 三频段 8-delay FDN；高质量 1024 rays、12 bounces | 提供空间尾响，不会修正某个叶源 direct 跳变 |
| 调度 | 异步 bounded worker，33 ms interval；quality 100 最多 64 个 direct source；未入选 emitter 的 direct gain 回退 unity | 正确地避开 audio thread；多采样会线性增加 direct ray 预算；大树林 rank churn 还会造成 processed↔unity 变化 |
| Area/extended source | 无 | 不能原生表达树冠面积/体积与方向分布 |
| Diffraction | 无 | 几何阴影边界没有绕射场保证连续性 |
| Path/IR temporal cache | 无传播路径缓存；只有目标 gain/tap/FDN 参数的 DSP 平滑 | 不能用上帧路径置信度稳定几何分类 |
| Occlusion class/hysteresis/cap | 无 | leaf ambient 与精确点声源被同一 first-hit 策略处理 |
| Per-emitter acoustics bypass | 无，只有 world 全局 toggle/quality | “unoccluded bed + occluded details”的干净实现需要 API 扩展 |

Direct、early 和 late 的预算可在固定提交中直接核对。[PetalSonic：missing direct response 回退 unity](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L79-L95) [PetalSonic：solve plan](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L19-L41) [PetalSonic：early reflections](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L535-L677) [PetalSonic：late solve](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L691-L797) [PetalSonic：late FDN](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/late_reverb.rs#L1-L17)

还有一个与主因独立、但可能放大时间不稳定的实现事实：worker 在求解完成后检测到 input generation 已 superseded，只增加计数，仍把旧 response 写入 `latest_response`。因此“superseded”目前不是 discard。[PetalSonic 固定提交：检测后仍发布](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L420-L449) 这不能证明用户听到的问题由旧帧倒灌造成，但在实现更强的 temporal cache/hysteresis 前，应先建立 `response.spatial_revision/geometry_version` 单调发布不变量。

### 4. 当前 release 观测证明了什么

**当前观测。** 在本基线执行 `cargo run --release -- --hidden --mute --auto-exit 2`，本地日志 `target/re-flora-logs/re-flora-20260823-224232.617-411502.log` 显示：

- `acoustics_backend=NativeAsync`、`environmental_acoustics_enabled=true`；
- startup tree：1941 个 trunk cones、29 个 leaf anchors、14572 个实际 render leaves、7 个 audio clusters；
- 2 秒退出时 `solves=61`、`published=61`、`superseded=10`、`response_age_ms=19`；
- 正常退出，无 error/panic。

这证明“当前代码路径实际上启用并求解”，也验证了 29→7 的稀疏点源布局；它**没有**逐源 direct raw gain、ray hit、材质、filtered gain 或最终输出电平，且 `--mute` 不适合做听感结论。所以当前最严谨的说法是：**机制与启用链已复现，听感因果尚缺可观测量。**

## 根因分层与可证伪预测

| 假设 | 当前证据 | 置信度 | 可证伪观测 |
|---|---|---:|---|
| 音源误置在自身枝干内 | 生成和几何共享 `segment.end`，epsilon 小于最小半径 | 高，已证实结构 | 显示每个 source 到 wood surface 的 signed clearance；预期多数 ≤ 0 |
| 单射线 first-hit 在移动时切换，造成大幅增益/频谱变化 | direct code 和材质差已证实；未记录实际沿轨迹切换 | 高，尚缺运行时因果闭环 | 固定轨迹记录 emitter raw gain，应出现 unity↔wood transmission 或 first-material 切换 |
| 少量点代理不能稳定表达分布式树冠 | 当前仅 7 点；环境声论文明确指出 few proxies 会 wobble | 高，模型错配 | 真实 surface samples / volumetric occlusion A/B 应降低总树冠能量的 dB 导数和 toggle rate |
| 33 ms solve + 50 ms smoothing 不足 | 参数已证实；是否可闻取决于切换频率/素材 | 中高 | 延长/非对称平滑或加入 hysteresis 后，raw 不变但 filtered/output discontinuity 下降 |
| superseded 旧 response 倒灌 | 旧 solve 仍发布且当前有 10 次 superseded | 中，可能放大 | 强制 discard 后 response revision 单调；同轨迹指标改善则成立 |
| direct source 64 上限导致 rank churn | quality 100 上限已证实 | 场景相关；单 startup tree 7 点不成立 | 大树林记录 candidate membership/toggles；单树 A/B 不应受影响 |
| wind 门控造成问题 | 有独立二态音量门控 | 低到中，需隔离 | 固定 wind/volume，几何 raw gain 仍跳则排除；acoustics off 后仍跳才指向 wind |
| 相同 12 秒 loop 的相移源相关叠加 | clip/seed 相同且簇增益按不相关功率模型已证实；实际相关度未测 | 低到中，次要风险 | 对比每 emitter 独立 seed/去相关 grains，测 summed 三频段 RMS；若 acoustics off 仍显著改善则成立 |

最关键的因果闭环是：固定 source gain/wind/clip 和固定 camera path，看到某一 emitter 的 direct state 随移动在 `unity` 与 `wood transmission` 间切换；关掉 environmental acoustics 后该变化消失，而 HRTF、距离和播放仍相同；换成 canopy surface/multi-sample 后变化显著下降。只要其中任一环不成立，就应回到上表重新排序，而不是继续猜参数。

## 方案比较

### 总表

| 方案 | 是否忠实于“树叶是分布式环境声” | 当前 API 直接支持 | 稳定性与声音代价 | 结论 |
|---|---|---|---|---|
| 把单点移到树冠外的固定位置 | 部分；至少不在木头内，但仍把体积压成一点 | 是 | 极便宜；近距离方位与距离衰减仍可能错误 | 可做紧急止血/A-B，不足以成为最终模型 |
| 每帧朝 listener 偏移点源 | 否；声源随听者移动，制造虚假位置 | 是，已有 point pose update | 可避开自遮挡，但 HRTF/距离和声像会漂移；多人 listener 更无定义 | 仅诊断，不推荐生产 |
| 多个分布式点源 | 比单点更接近；取决于是否采真实叶簇/表面、是否稳定加权 | 是；当前就是最多 8 点，但点选错了 | 成本随 voice、HRTF、direct rays 增长；太稀疏仍会 wobble | **最小可行路径**：修正 samples，不增加 listener 依赖 |
| 树冠表面采样，仍发布点源 | 较忠实；固定 surface representatives 是分布式源的离散近似 | 是 | 避免自身木内命中；仍有离散声像/独立遮挡，需要稳定选择与交叉淡化 | **推荐 Re: Flora 侧最小改动** |
| 真正 area/extended/volume source | 是 | 否 | 能对 extent、方向分布和软阴影整体求解；需要 API/DSP 扩展 | **推荐长期模型** |
| unoccluded ambient bed + occluded detail | 语义上合理的分层模型；bed 表示大量不相干叶事件，detail 给近场定位 | 只能粗糙模拟；无 per-emitter bypass | 能防整棵树被压暗；纯 non-spatial bed 会失去距离/方位和室内外传播 | 需要 per-emitter/bus policy 后才是干净方案 |
| 遮挡分类 + LPF/衰减上限 + attack/release hysteresis | 是心理声学/产品策略，不改变几何位置 | 否 | 有界、便宜、易测；不提供物理绕射方向 | **推荐最小 PetalSonic 扩展** |
| 衍射/透射 + 路径缓存/时间平滑 | 物理上正确 | 仅有 first-hit transmission 与 DSP smoothing；其他无 | 阴影边界连续、遮挡后仍有路径；实现和场景预算显著更大 | transmission thickness/cache 中期，edge diffraction 长期 |
| listener-dependent surface virtual source/proxy | 若从真实 extent 选择并平滑切换，比“朝 listener 推一点”合理；仍是渲染代理 | point update 可做；引擎无 proxy contract | 单声道/HRTF点成本低，但候选切换需 crossfade，不能代表全部树冠 | 受限 fallback，不作为权威源位置 |

### A. 把点源移到树冠外 / 朝 listener 偏移

静态地把点从枝端沿枝方向推到木几何外，能直接消除“source endpoint 仍落在自身 RoundCone”这一首要错误。当前 point API 完全能做，成本几乎不变；但一个外移点仍不等于树冠，且“沿枝方向 + 固定距离”未必落在叶片实际密集区域。比固定魔数更好的做法是从真实 `leaf_placements` 选一个有明确 wood clearance 的叶簇表面代表点。

朝 listener 偏移或把 proxy 放在 listener-facing crown surface，通常能减少 direct 自遮挡，却改变了物理世界里的 source pose。若每帧追 listener，距离衰减、HRTF azimuth/elevation 和 wind sample 都会被一个渲染技巧拖动；它会把一次遮挡问题换成声像游移。若只为验证“自遮挡是否主因”，这是很有价值的 A/B；若用于生产，应至少把权威 canopy extent 与 listener-dependent rendering proxy 分开，proxy 在稳定候选间带滞后和交叉淡化，且不能写回 wind/gameplay source position。

### B. 多个分布式点源 / 树冠表面采样

这是当前 API 能做且最符合产品语义的最小方案。关键不在“再多几个点”，而在以下不变量：

1. samples 来自真实 leaf sprays/placements 或其包络表面，不能来自 branch endpoints；
2. 每个 sample 有大于 `ray_epsilon + geometry/voxel safety margin` 的 wood clearance；
3. 选择在树重建前后尽量稳定，例如以固定局部方位扇区/八分体选最外且 leaf weight 最大的代表点，而不是随输入顺序的贪心首点；
4. 每个 representative 的 ID、loop phase 和 gain weight 稳定；切换布局时 crossfade，不能 remove/recreate 全部 source 后瞬变；
5. 所有 sample 的总 power 归一化，避免 source 数量改变导致整棵树变响。

当前已经有最多 8 点和随机 loop phase 的框架，因此可先不增加 source 数量，只把 29 个 branch endpoint 候选换成 render leaf placements 的稳定 surface representatives。这样修的是事实错误，不是通过加重预算掩盖它。

不过，离散点依然只能近似扩展源。Zhang 等把 ambient sound 定义为分布在大面积/体积内的大量混沌事件叠加，并直接指出少量点代理会随 listener 移动产生不真实的 loudness wobble；他们用预计算 FDTD power field 和方向编码获得平滑、位置相关的扩展源响应。[项目与摘要](https://www.cs.cornell.edu/projects/ambientsound/) [原论文](https://www.cs.cornell.edu/projects/ambientsound/SAsia-2018-ambient2.pdf) Re: Flora 场景可编辑且树会生成/删除，不能直接照搬静态烘焙，但这说明“有限 surface samples 是最小近似，不是最终真值”。

### C. Area / extended source

真正的扩展源应有一个权威 extent（如 canopy sphere/ellipsoid、weighted surface sample set 或稀疏 density field），并在 listener 位置求整体 direct energy、方向分布和遮挡软化。Cornell 的 acoustic texture 工作把随机过程、扩展源的空间支持与 listener-dependent 传播纹理解耦，目标正是让雨、溪流、树叶等 extended ambient 随位置产生可信的 diffraction/reverberation 变化。[Zhang 等，*Acoustic Texture Rendering for Extended Sources*](https://research.cs.cornell.edu/ambientsound/acoustictexture/)

PetalSonic 0.7.0 没有 source extent。若扩展 API，最近的实用先例是 Steam Audio 的 volumetric occlusion：把 source 建模为给定半径的球，在其内取多个点做 occlusion sampling；官方文档明确说明增加 samples 会让 occlusion transition 更平滑，但增加 CPU。[Steam Audio 官方 simulation API：`maxNumOcclusionSamples`](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html) 同一接口还把 per-source `occlusionRadius`、`numOcclusionSamples` 暴露为求解输入。这比“移动一个虚点”更符合树冠的物理意义，也可以在 PetalSonic 现有 batched closest-hit query 上做有限扩展。

更完整的 area/volume HRTF 可以使用 listener-centered spherical projection 与低阶方向表示，使成本不直接随源网格复杂度增长；Schissler 等也指出稠密点采样昂贵，而粗点采样在 listener 接近或进入 extent 时会暴露为离散声源。[Schissler、Nicholls、Mehra，*Efficient HRTF-based Spatial Audio for Area and Volumetric Sources*](https://www.carlschissler.com/downloads/publications/ieeevr2016.pdf) 这适合作为长期 PetalSonic `EmitterShape`/direction-lobe 设计依据，不适合作为本问题第一步。

### D. Direct/unoccluded ambient layer + occluded detail layer

分层在感知语义上合理：

- **ambient bed：** 代表大量、相位不相关的叶片事件，不应因一根枝干的单射线命中而整体衰减 20–36 dB；应保留有界的距离、室内外/门户与总遮挡变化。
- **localized detail：** 少量靠近 listener 或显著风区的叶簇，可保留 HRTF、direct occlusion、反射与更快动态。

但当前最直接的 `non_spatial` bed 会同时丢掉树的位置、距离和 HRTF；全局关闭 environmental acoustics 又会影响所有 emitter。因此“一个完全 unoccluded 的 2D bed + 若干 3D details”只能作为产品原型，不能声称是正确传播。干净的实现需要 PetalSonic 提供 per-emitter `AcousticRouting`/profile，例如 bed 使用 distributed direct policy 或仅绕过 first-hit direct attenuation，仍进入 shared late reverb；detail 使用 point policy。

### E. 遮挡分类、低通/衰减上限、attack/release hysteresis

这是最小且高收益的 PetalSonic 扩展。不要把所有 emitter 硬编码成相同策略；让 `EmitterSpatialState` 或 emitter desc 声明用途，例如：

```rust
enum OcclusionProfile {
    PointExact,
    AmbientDistributed {
        radius_or_extent: f32,
        samples: u8,
        gain_floor: [f32; 3],
        attack_seconds: f32,
        release_seconds: f32,
    },
}
```

对 `AmbientDistributed`，direct solver 对 3–5 个稳定 surface/extent samples 求三频段 transmission/visibility，再以 power/energy 而不是任意首命中二态值聚合。分类状态用两个阈值和最短驻留时间：只有可见比例低于较低阈值一段时间才进入 occluded，高于较高阈值一段时间才退出；连续增益再用独立 attack/release 追踪。`gain_floor`/最大低通强度防止一根细枝把整片树叶床完全闷掉。

以下仅是**建议起始搜索区间，不是声学事实或验收结论**：

- 3–5 samples/source；先保持 7–8 source/tree，总 direct rays 从约 7–8 增为 21–40；
- enter-occluded visibility `< 0.25` 持续 80–150 ms，exit `> 0.55` 持续 120–250 ms；
- into-occlusion attack 150–300 ms，out-of-occlusion release 100–200 ms；实际方向应通过盲听确认，不应用名称决定；
- ambient 三频段最大衰减先搜索 `[-3,-6,-9] dB` 到 `[-4,-8,-12] dB`，高频等效低通最低截止频率搜索 1.5–3 kHz；
- point-exact profile 继续允许材质 transmission，但也应有可配置 smoothing，避免策略污染其他精确点源。

Steam Audio 的 volumetric occlusion 是“多点能平滑 transition、代价是 CPU”的官方工程先例；具体阈值和上限则必须由本游戏录音、固定轨迹指标和盲听选择，不能从其他引擎照抄。[Steam Audio 官方文档](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)

### F. 衍射、透射、路径缓存与时间平滑

当前 first-hit transmission 是有用基础，但它不累计材质厚度或多表面：ray 只取离 listener 最近的一个命中；一根细枝与一堵厚木墙可得到同一系数。Steam Audio 的 per-source `numTransmissionRays`/多 surface transmission 是可参考的逐步扩展，但树叶环境声首先需要 extent visibility，不必一开始做通用厚度积分。[Steam Audio 官方 transmission 参数](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)

衍射从物理上解决“直达路径进入几何阴影后仍有绕边传播”，也天然帮助阴影边界连续。Calamia 与 Svensson 明确指出：几何声学分量在 source/listener 穿过 specular/shadow boundary 时会不连续，而加入 edge diffraction 后总声场连续；只有几何声学时，这种不连续可成为 click 等可闻伪影。[Calamia、Svensson，*Fast Time-Domain Edge-Diffraction Calculations for Interactive Acoustic Simulations*](https://link.springer.com/article/10.1155/2007/63560) 但 Contree 是体素/隐式复杂几何，稳定提取可用 diffraction edges 和控制路径数并不小；它是长期物理质量项，不应阻塞锚点纠错和有限多采样。

“时间平滑”应分成三层，不能只把 50 ms 常数调大：

1. **publication correctness：** superseded/旧 revision response 不得覆盖更新 response；
2. **path/classification persistence：** 缓存上次 extent sample 的 visibility/material/path 置信度，稳定 sample 序列，以 geometry/spatial revision 决定失效；
3. **DSP parameter interpolation：** raw energy/classification 确定后，再以 attack/release 在 audio thread 追踪目标。

自适应 impulse-response 研究也把 temporal coherence、IR cache 和指数平滑作为动态声学响应的核心，并明确呈现 rays/响应速度的取舍。[Schissler 等，*Adaptive Impulse Response Modeling for Interactive Sound Propagation*](https://gamma-web.iacs.umd.edu/ADAPTIVEIR/paper.pdf) PetalSonic 长期可以缓存 `emitter id + geometry version + stable extent sample index` 的传播结果；但缓存必须服从单调 revision，不能把旧 response publication 变成另一种抖动源。

### G. Listener-dependent virtual source/proxy

虚拟源不是绝对不可用。若它是“权威 extended source 的渲染代理”，从真实 tree extent 上选择 listener 可见的稳定 surface candidate，带 hysteresis/crossfade，并且只影响 renderer pose，它能以一个 HRTF voice 近似主到达方向。多源声学研究也会从 listener 追踪并聚类大量 source/path 来控制成本。[Schissler、Mehra、Manocha，*Interactive Sound Propagation and Rendering for Large Multi-Source Scenes*](https://gamma-web.iacs.umd.edu/MULTISOURCE/paper.pdf)

但本项目现在没有权威 extent，也没有 proxy/lifetime contract。简单 `source += normalize(listener-source) * offset` 既不是物理 surface sample，也不保留稳定方向；它只能用来做“只要 source 离开木头，问题是否消失”的诊断实验。长期若使用 proxy，应保留真实 canopy descriptor，用多个方向 lobes 或 bed/detail 分层表达能量，不能把 proxy 当作树叶真正位置。

## 推荐路线

### 最小方案：先纠正位置，再加一个有界 distributed-occlusion profile

建议把“最小”分成两个可独立验收的阶段。

#### M0：可观测诊断，不改变产品行为

先增加可在 debug/diagnostic 模式开启、默认无逐帧刷屏的 telemetry；固定树 seed、wind、source volume、clip phase 与 camera trajectory。它的目的是决定 M1 是否已经足够，以及是否需要 M2。所需指标见后文。

#### M1：Re: Flora-only 的语义纠错

- 从 `relative_leaf_placements`/leaf sprays 建立树冠候选，不再把 `relative_leaf_positions` 的 branch endpoints 当音频位置；
- 以树局部固定扇区/八分体选择最多 8 个稳定的外表面代表点，权重来自其代表的 leaf count；
- 用 Contree/树 trunk primitives 验证每点具有 `source endpoint epsilon + 至少 1 voxel` 的安全 clearance；若无合格候选，沿树冠外法线寻找最近合格 leaf candidate，而不是朝 listener 偏移；
- 保持 source ID、随机 loop phase 和总功率稳定；布局变更时 crossfade；
- 不改环境声学材质、不关闭全局 acoustics、不加入魔数音量补偿。

M1 完全可由现有 point API 支持，修复的是已证实的错误。它很可能消除最严重的自遮挡二态跳变，并把声音位置变得更像真实叶簇。若固定轨迹指标和盲听已经达标，可以先发布，而无需等待 PetalSonic API。

#### M2：PetalSonic 的最小鲁棒扩展

- 给 emitter 加 `PointExact` / `AmbientDistributed` profile 和 radius/稳定 sample set；
- 3–5 条 batched direct rays 求 extent visibility/三频段 energy，带衰减 floor、Schmitt hysteresis、minimum dwell、非对称 attack/release；
- direct response 暴露 raw 与 filtered diagnostics；
- 检测 superseded solve 后不发布；render 端拒绝 spatial/geometry revision 回退；
- 保持 early reflection 与 late FDN 第一版不变，避免范围膨胀。

M2 是“当前 0.7 point API 不能直接完成”的最小扩展。它让所有 foliage ambient emitter 使用同一可测、有限预算的策略，也为雨、溪流、火焰等扩展环境声留出通用接口。

### 长期方案：原生 extended ambient source

Re: Flora 发布一个不可变的 `CanopyAcousticDescriptor`，至少包含：局部中心/椭球 extent、稳定 weighted surface samples、总 power、source content/phase seed、geometry generation。PetalSonic 的公开输入表达 `EmitterShape::Point | Sphere | Ellipsoid | WeightedSamples` 与 routing/profile，而不是要求游戏每帧移动 proxy。

PetalSonic worker 对每个 extended source：

1. listener-centered 地选择/复用 extent samples，聚合 direct 三频段 energy；
2. 估计一小组方向 lobes 或低阶 spherical-harmonic direction distribution，供 HRTF/耳机渲染，不把全部能量放进单一点声像；
3. early reflection 以 source extent 的可见能量连接，而不是反射点只连一个 proxy；
4. late reverb 继续 listener-centric shared solve，只按 source/scene 能量注入，不为每片叶子单独求解；
5. response/path cache 以 emitter、geometry、spatial revision 和 sample ID 为键，稳定跨帧更新；
6. 后续按收益加入多表面 transmission 和 edge/wave diffraction。

Cornell 的 FDTD/方向 power field 在静态场景能以少量内存和快速运行时表达整个扩展源，但 Re: Flora 的可编辑体素和动态树不适合把其全局预计算作为唯一方案。[Ambient Sound Propagation 项目](https://www.cs.cornell.edu/projects/ambientsound/) 更合适的是借用其“扩展源的能量/方向场而非少量独立点”的模型，再沿用 PetalSonic 当前异步、bounded、immutable response 的实时架构。

## 建议文件边界

本节只定义未来改动归属，不代表本报告实施这些改动。

### Re: Flora

- [`src/tree_gen/tree.rs`](../src/tree_gen/tree.rs)：生成阶段明确区分 `branch_leaf_anchors`、render `leaf_placements` 与 canopy acoustic descriptor；不再让“leaf audio positions”含糊等同枝端点。
- 建议新增 `src/audio/tree_audio_layout.rs`：纯函数负责稳定 surface representative 选择、权重归一、ID/sector、clearance policy；便于无音频设备的确定性测试。不要把几何选择塞入 source lifetime manager。
- [`src/app/core/vegetation.rs`](../src/app/core/vegetation.rs)：在树创建/重建时产出并交付不可变 canopy layout；控制旧/新布局的 generation 与 crossfade。
- [`src/audio/tree_audio_manager.rs`](../src/audio/tree_audio_manager.rs)：拥有 source lifetime、clip phase、weight/gain 和 tree→source 映射；不再定义树冠几何真值。
- [`src/audio/tree_audio_source.rs`](../src/audio/tree_audio_source.rs)：保留 wind/content 控制；避免把 listener-dependent proxy 写回 physical wind sample position。
- [`src/audio/spatial_sound_manager.rs`](../src/audio/spatial_sound_manager.rs)：PetalSonic adapter、完整 frame 发布和可观测量汇总；未来转发 profile/extent，不在这里实现采样算法。
- [`src/builder/contree/mod.rs`](../src/builder/contree/mod.rs)：继续提供不可变 geometry snapshot/ray/material；只有在树生成 primitive 无法给出 clearance 时才增加批量 clearance query，避免音频布局反向依赖 mutable renderer state。

### PetalSonic

- `petalsonic/src/domain.rs` / public emitter config：定义 source shape/extent、occlusion profile 和 immutable spatial input；默认保持 `PointExact` 向后兼容。
- `petalsonic/src/acoustic_propagation.rs`：直接声多采样/能量聚合、分类状态、缓存、预算、revision publication correctness 和 diagnostics。
- `petalsonic/src/spatial/processor.rs`：只消费已发布 target，执行无分配的三频段 gain/LPF attack-release 和可选方向 lobes crossfade；不在 audio thread trace geometry。
- `petalsonic/src/acoustics.rs`：扩充传播响应/材质语义时保持 ray query 小而稳定；不要把 Re: Flora 的 tree 类型暴露给引擎。
- `petalsonic/src/world.rs`：公开 per-emitter policy/extent 更新和只读 diagnostics；全局 toggle/quality 继续作为总开关。
- `petalsonic/src/spatial/late_reverb.rs`：最小方案无需改；长期只接收 extended source 的能量注入，不承载 source geometry。

## 参数与性能预算

### 当前预算基线

- quality 100：最多 64 个 direct sources，每个 1 ray；最多 8 个 early sources、每源最多 2 taps/256 probe rays；late 1024 rays、12 bounces；solve interval 33 ms。
- startup 单树当前 7 个 leaf sources，不触发 64 direct source cap；大片树林可能触发按 `priority / (1 + distance)` 排名和 candidate churn。[PetalSonic：candidate rank](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L468-L484)
- 基线短运行 solve p50/p95/p99 桶为 `1024/1024/4096 us`，max `2914 us`，但只有 2 秒且是隐藏静音 startup，不能作为最终硬件预算。

### 建议预算原则

1. M1 保持 `<= 8` source/tree 和总 power；它不应增加 HRTF voice 或 direct ray 数。
2. M2 首先只给进入 direct candidate set 的 ambient source 做 3–5 samples；batched ray 数是清楚可控的 `ambient_candidates × samples`，应记录而不是隐藏在 quality 百分比后。
3. 可先将全局 direct ray hard cap 设为当前 64 的 3–4 倍作为测量候选，但 source prioritization 必须以“一个 extended source 的全部 samples”为原子单位，不能只算半个 extent 后偏置结果。
4. 若预算不足，优先按距离/屏幕外/总 power 降 samples（5→3→1），再降低 update rate；不要改变固定 sample 身份或每帧随机重采样，否则省下射线却增加 temporal noise。
5. late FDN 保持 listener-centric shared cost；树叶扩展源不应为每个 sample 创建独立 late reverb solver。

必须用 release 隐藏模式、固定场景/轨迹在目标硬件测 p50/p95/p99/max solve time、response age、rays/solve、audio underrun 和 frame time。debug/test 只能验证纯逻辑，不是性能证据。

## 验证场景与可观测指标

### 固定场景

至少保留四个小场景，树 seed、geometry revision、wind 和 camera script 可复现：

1. **自身枝端：** 当前 startup tree，选一个已知 source 在木内；listener 以恒速沿圆弧绕该枝端，穿过 trunk shadow，正向与反向各一次。
2. **外部粗树干遮挡：** source 明确位于 canopy surface，listener 路径让一根粗主干从无遮挡渐进到遮挡再退出，区分“自交”与真实遮挡。
3. **静止边界：** listener 停在 ray hit/no-hit 临界点 10 秒，检测数值 chatter、solver jitter 和旧 response 倒灌。
4. **多树压力：** 足以超过 64 direct candidates 的树林，固定穿行路径，检测 source rank churn、ray budget、response age 与 HRTF voice 成本。

每场景做以下 A/B，其他参数完全相同：

- environmental acoustics off（保留 HRTF/distance/air）；
- 当前 branch-endpoint 点源；
- M1 stable canopy-surface point samples；
- M2 volumetric/distributed profile；
- 仅作为诊断的 listener-facing proxy。

### 必需 telemetry

每个 acoustic response / emitter 至少记录：

- `spatial_revision`、`geometry_version`、captured/published revision、response age、solve superseded/discarded；
- emitter ID、tree ID、physical anchor、render proxy（若有）、canopy extent/sample ID、acoustic priority/candidate membership；
- direct sample count、hit count、visible fraction、first material/category、raw 三频段 gain、classified state、filtered 三频段 gain、hysteresis dwell/transition count；
- source content gain、wind target/current gain、distance/air gain，避免把风门控误判成遮挡；
- emitter clip/seed/loop phase、两两或聚合相关性，以及独立 seed / decorrelated grains A/B 标签；
- early tap count/energy、late send/parameters，用来判断 direct 降低时是否出现不合理总能量洞。

运行级指标：

- propagation solve p50/p95/p99/max、rays/solve、candidate count/churn、response age p95/p99；
- render/audio block time、ring-buffer underrun、HRTF active voices；
- 输出录音的 20–50 ms block 三频段 RMS、block-to-block dB derivative p95/p99、最大单步、遮挡状态 toggles/s、spectral centroid/LPF cutoff discontinuity、左右声像/azimuth continuity；
- 整棵树所有 leaf layers 的 summed 三频段 RMS/总 power，而不只看单 emitter；同一 12 秒 loop 仅相移与独立 seed/真正 decorrelated grains 必须作为独立 A/B，避免把内容相关叠加误报成传播抖动。

### 建议验收门槛（需用基线和盲听校准）

下列是让实验可判定的起始门槛，不是既定产品标准：

- 同一 run 中已发布 `spatial_revision`/`geometry_version` 不回退；superseded response publication 为 0；
- 固定边界静止 10 秒，filtered occlusion state 不持续 chatter；状态切换次数相对当前至少降低 80%；
- 恒速圆弧中，总 leaf bed 的 20 ms block 最大非内容电平步进 `< 1 dB`，p99 dB derivative 相对当前至少降低 50%；若素材本身波动过大，则用同一 input block 的 A/B 差值度量；
- M1/M2 不制造可听的 proxy azimuth jump；正向/反向路径的状态转折位置在 hysteresis 预期范围内；
- 目标平台 release 模式 audio underrun 为 0；solve p99 和 response-age p99 不因 3–5 samples 越过团队声明的声学预算；
- 环境声学 off 的控制组若仍出现同样跳变，停止调 occlusion，转查 wind gate、source recreate、loop phase 和普通 distance/HRTF 路径。

## “现有这套是否真的没法解？”

不是。

**现有 Re: Flora + PetalSonic 0.7 API 直接能做的：**

- 把声源从已证实错误的 branch endpoints 改到稳定、真实的 leaf/canopy surface samples；
- 保留多个分布式点、随机相位与总 power 权重；
- 用 point pose update 做严格受控的 proxy A/B；
- 用全局 environmental toggle 做因果隔离；
- 复用现有异步 geometry snapshot、first-hit transmission、early taps 和 late FDN。

这些足以做正确诊断，也很可能给出可发布的最小修复。**不能直接做的**是原生 area/volume HRTF、per-emitter distributed occlusion/bypass、visibility aggregation、hysteresis/cap、传播路径缓存和 diffraction；要获得对所有树型、近距离和复杂遮挡都稳定的长期语义，PetalSonic 需要小步扩展。

所以准确答案是：**当前架构有解，当前数据模型不完整。** 不应继续把错误枝端点当叶声位置，也不应因为 point API 暂时有限就用 listener-following point 伪装成物理位置。先修权威 canopy samples、补 observability，再用测量决定是否进入 per-emitter distributed profile；衍射和完整 extended-source rendering 是后续质量层，而不是解决眼前二元自遮挡的前置条件。

## 一手资料与固定源码

- PetalSonic 0.7.0 固定提交：[`acoustic_propagation.rs`](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs)、[`processor.rs`](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs)、[`late_reverb.rs`](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/late_reverb.rs)、[`domain.rs`](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/domain.rs)、[`world.rs`](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs)。本地 crates.io 0.7.0 源码与这些固定 URL 的相关文件逐文件核对。
- Valve，Steam Audio C API [Simulation](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)：volumetric occlusion 的 source radius / point sample 数、样本数与平滑/CPU 取舍、多表面 transmission、path visibility。
- Zhang、Raghuvanshi、Snyder、Marschner，[*Ambient Sound Propagation* 项目](https://www.cs.cornell.edu/projects/ambientsound/) 与 [论文](https://www.cs.cornell.edu/projects/ambientsound/SAsia-2018-ambient2.pdf)，ACM TOG / SIGGRAPH Asia 2018：分布式 ambient source、少量点代理的 loudness wobble、位置相关方向能量场。
- Zhang、Savioja、Manocha，[*Acoustic Texture Rendering for Extended Sources in Complex Scenes*](https://research.cs.cornell.edu/ambientsound/acoustictexture/)：随机扩展源的 listener-dependent acoustic texture、reverberation 与 diffraction。
- Schissler、Nicholls、Mehra，[*Efficient HRTF-based Spatial Audio for Area and Volumetric Sources*](https://www.carlschissler.com/downloads/publications/ieeevr2016.pdf)，IEEE VR 2016：listener-centered area/volume source projection与低复杂度方向渲染。
- Calamia、Svensson，[*Fast Time-Domain Edge-Diffraction Calculations for Interactive Acoustic Simulations*](https://link.springer.com/article/10.1155/2007/63560)：衍射使几何声学的 specular/shadow boundary 总声场连续，并减少可闻不连续伪影。
- Schissler 等，[*Adaptive Impulse Response Modeling for Interactive Sound Propagation*](https://gamma-web.iacs.umd.edu/ADAPTIVEIR/paper.pdf)：动态传播的 IR cache、temporal coherence、指数平滑与 ray/响应速度取舍。
- Schissler、Mehra、Manocha，[*Interactive Sound Propagation and Rendering for Large Multi-Source Scenes*](https://gamma-web.iacs.umd.edu/MULTISOURCE/paper.pdf)：listener-based backward tracing、source/path clustering 与有界多源渲染。

## 本报告验证记录

- 开始状态：`git status --short --branch` → `## agent/leaf-audio-occlusion-research`；HEAD 为 `0b607897bd2cf41bcc1ad686a379f5cabde710ba`。
- 只读核对当前 Re: Flora source、`Cargo.toml`/`Cargo.lock`、本机 crates.io PetalSonic 0.7.0 source，以及 PetalSonic `v0.7.0` tag 解引用提交 `06d992f755fdc17a26b52a4eef97341ebe8d6e12`。
- release 隐藏静音基线命令：`cargo run --release -- --hidden --mute --auto-exit 2`；本地日志 `target/re-flora-logs/re-flora-20260823-224232.617-411502.log` 正常退出。该 ignored run log 仅验证启用链与预算计数，不作为听感证据，也不随报告提交。
- 除本 Markdown 报告外未修改生产代码、配置或生成文件。

## 实施验证记录（2026-08-24）

所有需要 extended-source API 的 Cargo 命令均使用未提交的命令行 patch，指向精确 producer `b65ef9b56f29466dfaafb875793e04d91bf49e2a`；下列记录中的“patched”都表示这一点：

- `cargo fmt --check`：通过；
- patched `cargo check --offline`：通过；
- patched `cargo test --offline audio::canopy`：13 passed；
- patched `cargo test --offline audio::tree_audio`：2 passed；
- patched `cargo test --offline audio::spatial_sound_manager`：4 passed；
- `python3 -m unittest scripts.tests.test_analyze_canopy_audio_diagnostic`：3 passed；
- patched 全量 `cargo test --offline`：collision binary 4 passed；主 binary 为 520 passed、1 failed、1 ignored。唯一失败是未改动的基线 PATT fixture `patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`，其保存配置含 2 snapshots、旧断言预期 1；按任务边界未修改无关 fixture。排除该已知测试后为 520 passed、0 failed、1 ignored、1 filtered out；
- patched `cargo run --release --offline -- --hidden --mute --auto-exit 0.5`：正常退出；`python3 scripts/check_latest_run_log.py --tail 30` 验证日志 `target/re-flora-logs/re-flora-20260824-014834.452-738251.log` 通过。该运行只证明 native window/Vulkan/audio lifecycle 启动路径，不作为音频行为证据；
- 单树与多树 10 秒 diagnostic 分析器均为 `verdict=PASS`，数值见上文；
- `config/gui.toml`、PATT snapshot/fixture、shader-derived/generated files均无本任务 diff；最终 manifest/lock 无 producer path/source 漂移。
