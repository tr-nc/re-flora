# 本地玩家脚步接入空间音频与环境声学：故障诊断和方案设计

> 日期：2026-08-23
>
> 审计基线：`0b607897bd2cf41bcc1ad686a379f5cabde710ba`（分支 `agent/footstep-spatial-audio-research`）
>
> 依赖版本：PetalSonic `0.7.0`，crate 对应上游提交 `06d992f755fdc17a26b52a4eef97341ebe8d6e12`
>
> 范围：只诊断、调研和设计；不包含实现。
>
> 证据标记：**项目事实**来自上述 Re: Flora 基线或其 Git 历史；**PetalSonic 事实**来自 0.7.0 发布源码；**一手资料**来自引擎/SDK 官方文档；**推断/建议**会明确标出，不能当成已测量结果。

## 结论先行

1. **当前脚步没有进入空间化或环境声学路径。** `play_jumping`、`play_landing` 和 `play_step` 虽然仍接收位置，却全部丢弃 `_position`，通过 `add_non_spatial_source` 播放。因此当前版本不会出现旧空间脚步的“拖在身后”，但也没有脚下方定位、遮挡、反射或材质驱动的空间感。参见 [`camera/audio.rs` L77-L113](../src/gameplay/camera/audio.rs#L77-L113) 和 [`spatial_sound_manager.rs` L199-L219](../src/audio/spatial_sound_manager.rs#L199-L219)。
2. **以前严重“落后”的最可能主因不是有限声速，而是把完整脚步 clip 固定在落脚瞬间的世界点。** 提交 [`68278efa`](https://github.com/tr-nc/re-flora/commit/68278efa02a4ab5cdef315b71fcdb34971400cc2) 创建一次性 spatial source 后再也不更新它；listener 继续前进时，后续音频块的 `source - listener` 必然逐渐指向后方。仓库实测 run clip 平均约 `0.620 s`、walk clip 平均约 `0.627 s`，所以这不是一两个采样点的小误差，而是持续大半秒的确定性相对运动。
3. **“一开始就不在正下方”还有一个独立但非必现的首块竞态。** 旧路径先在 camera update 内积分新位置并创建/播放 source，`Tracer::update_camera` 返回后才更新 listener；如果音频线程恰好在两者之间生成首块，会看到“本帧 source + 上一帧 listener”。这应通过 revision/onset telemetry 证实，不能断言每一步都发生。即便把此顺序完全修好，世界固定 source 在后续块中仍会确定性落后。
4. **PetalSonic 0.7.0 的 direct path 没有按 `distance / 343 m/s` 延迟。** direct 每个渲染块直接用当前 source/listener 差计算衰减和 HRTF 方向；`343 m/s` 只用于 early reflection 的额外路程和 late reverb pre-delay。33 ms 声学 solver、50 ms gain/tap 平滑会使遮挡/反射/tail 参数滞后，却不会把 direct HRTF 方向缓存 50 ms。参见 [direct DSP](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L602-L655)、[HRTF direction](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L807-L880) 和 [reflection delay](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L608-L645)。
5. **绝不能“拖在身后”的最强保证来自参考系，而不是预测。** 2D/head-locked direct 永远不会产生世界方位；而同一原子 `SpatialFrame` 中严格保持 listener-local 脚下偏移的 direct，也能保证平移、转向和输出排队都不改变其听觉方向。世界接触点 fully spatialized one-shot 天生会在玩家离开后位于身后；这对远处 NPC 正确，对本地玩家的主 direct 层不合适。
6. **本项目最小可行方案：第一阶段保留当前 2D dry 作为安全基线；空间接入阶段再用原子更新的 listener-relative 脚下 emitter 替换它，而不是把两份 direct 叠播。** controller 只产生语义化 `FootstepEvent`，app 必须按 `movement → resolve event/clip and register emitter → listener/follower poses → publish complete SpatialFrame → play prepared events` 排序。PetalSonic 现有 API 足以实现这条“跟随的 spatial direct”，但环境响应会跟着玩家移动，不是真实的世界接触点 tail。
7. **长期推荐：一份 PCM、两种空间语义的 perceptual split。** 即时 dry/direct 固定在 listener-local 脚下，禁用本地 direct 的 geometry/physical delay；同一声音的 early reflections/late environment send 使用真实世界接触点。这样 direct 绝不落后，同时保留脚下定位、地面材质和房间感。PetalSonic 0.7.0 还不能让同一 voice 的 direct placement 与 per-play acoustic origin 分离，也没有独立 wet send/direct-geometry-bypass，需扩展一个小而明确的接口。

## 当前声音怎样进入 PetalSonic 与 native environmental acoustics

### 两条已有入口

**项目事实。** Re: Flora 在一个 `SpatialSoundManager` 中创建 world-owned PetalSonic runtime：48 kHz、1024 帧 block、`SpatialQuality::LowLatency`、`LatencyProfile::Balanced`、native HRTF、`distance_scaler = 15`，并传入 Contree `AcousticSceneSnapshot`（[`spatial_sound_manager.rs` L75-L101](../src/audio/spatial_sound_manager.rs#L75-L101)；创建处为 [`app/core/mod.rs` L789-L795](../src/app/core/mod.rs#L789-L795)）。`Cargo.toml` 和 lockfile 都固定 PetalSonic 0.7.0（[`Cargo.toml` L30-L38](../Cargo.toml#L30-L38)、[`Cargo.lock` L2808-L2812](../Cargo.lock#L2808-L2812)）。

入口分为：

- **Spatial looping source。** 树叶环境声和地形编辑 loop 创建 spatial emitter，保存在 `uuid_to_source`，位置和优先级随每个完整 `SpatialFrame` 进入 native HRTF 与 acoustic worker。树声入口见 [`tree_audio_manager.rs` L297-L313](../src/audio/tree_audio_manager.rs#L297-L313)，地形编辑声见 [`app/core/input.rs` L526-L550](../src/app/core/input.rs#L526-L550)。
- **Non-spatial one-shot。** UI 和当前脚步按 clip path 缓存 non-spatial emitter，再为每次触发创建自动回收的 one-shot voice；它们不在 `SpatialFrame.emitters` 内，所以没有 HRTF、distance、occlusion、reflection 或 late environment send（[`spatial_sound_manager.rs` L199-L219](../src/audio/spatial_sound_manager.rs#L199-L219)）。

`publish_spatial_frame` 从同一个 host 快照构造 listener pose 和所有登记的 spatial emitter pose，再发布递增 revision（[`spatial_sound_manager.rs` L235-L270](../src/audio/spatial_sound_manager.rs#L235-L270)）。PetalSonic 要求该列表恰好包含所有已注册 spatial emitter，并在 render quantum 边界原子消费；缺项、重复或 non-spatial 项都会拒绝（[PetalSonic `world.rs` L243-L280](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs#L243-L280)）。

默认 GUI 并未把脚步压到 `-40 dB`：`footstep_volume_db = 33.5`（[`config/gui.toml` L651-L660](../config/gui.toml#L651-L660)），app 先做 `-40 + 33.5 = -6.5 dB` 的 controller gain，再叠加 step 自身的 `-4..0 dB` 速度增益，所以配置范围约为 `-10.5..-6.5 dB`（[`app/core/mod.rs` L3934-L3937](../src/app/core/mod.rs#L3934-L3937)、[`camera/audio.rs` L103-L112](../src/gameplay/camera/audio.rs#L103-L112)）。未来做 spatial priority 时不能把 `-40 dB` 当作实际脚步响度。Environmental acoustics 默认也是 `enabled = true, quality = 100%`（[`config/gui.toml` L794-L816](../config/gui.toml#L794-L816)），因此默认验收应同时测 environmental on 和显式 off，而不是假设环境路径未启用。

### 地形已经有声学材质，但脚步没有材质选择

**项目事实。** Contree closest-hit 能返回 `position + voxel_type`，并已有同步 CPU ray query（[`contree/mod.rs` L128-L143](../src/builder/contree/mod.rs#L128-L143)、[`contree/mod.rs` L197-L207](../src/builder/contree/mod.rs#L197-L207)）。dirt、sand、stucco、wood、rock 已映射到三频 absorption/scattering/transmission，供 PetalSonic 传播求解（[`contree/mod.rs` L2477-L2516](../src/builder/contree/mod.rs#L2477-L2516)）。

但脚步只从 `Footsteps SFX - Undergrowth & Leaves` 的单一素材族随机选择（[`camera/audio.rs` L22-L48](../src/gameplay/camera/audio.rs#L22-L48)）；KCC 结果只有 translation 和 grounded，没有 contact point/material（[`physics.rs` L221-L242](../src/app/core/physics.rs#L221-L242)）。因此要区分两个概念：

- acoustic material 已能改变遮挡、反射和 room response；
- footstep surface 必须在 gameplay/content 层选择 dirt/wood/stone 等 clip bank，目前尚不存在这条 seam 和对应素材。

## 当前脚步事件、坐标和帧时序

### 事件何时产生

**项目事实。** walking/landing/regular step 在 KCC 返回后先应用本帧 `result.translation`，再从更新后的 collision position 计算中心线脚点 `(x, y - camera_height, z)`，最后触发声音（[`controller.rs` L399-L472](../src/gameplay/camera/controller.rs#L399-L472)）。所以普通 step 与 landing 当前并不是直接使用“上一帧 source 位置”。jump 是例外：它在本帧 desired translation 应用之前，用当前 collision position 触发离地声（[`controller.rs` L335-L377](../src/gameplay/camera/controller.rs#L335-L377)）。

Stride phase 按 `dt / interval` 前进，跨过 1.0 的这一帧触发，因而周期事件只有最多约一个 game frame 的量化误差，没有额外的固定“落地后延迟”（[`stride.rs` L27-L63](../src/gameplay/camera/stride.rs#L27-L63)）。停止后 phase 重置为 0.85，所以开始行走的第一步有意在 15% interval 后发生：walk 约 52.5 ms、run 约 37.5 ms；这是提交 [`dfa9b46c`](https://github.com/tr-nc/re-flora/commit/dfa9b46c8374305f85264902b085b857907460d9) 缩短后的“起步节拍”，可能解释“按键后声音晚一点”，不能解释声像持续位于身后。

### 坐标空间

**项目事实。** 默认 camera height 为 `0.08` world unit；正常速度 `0.25 world unit/s`、boost 倍率 `2.2`，即 sprint 目标 `0.55 world unit/s`（[`desc.rs` L8-L15](../src/gameplay/camera/desc.rs#L8-L15)、[`desc.rs` L57-L74](../src/gameplay/camera/desc.rs#L57-L74)）。PetalSonic 的 `distance_scaler = 15` 将它们解释为约 1.2 m 耳脚高度、3.75 m/s 正常移动和 8.25 m/s sprint；这里是声学标尺换算，不代表 gameplay 常量本身以米命名。

当前 foot position 始终位于角色/相机中心线，没有实际左、右脚横向 offset。Head bob 虽有 parity 和横向偏移，但只改 view matrix，并未改 collision anchor、listener pose 或 foot event pose（[`head_bob.rs` L52-L85](../src/gameplay/camera/head_bob.rs#L52-L85)）。因此当前“左右脚 seam 搞反”不是已有脚步落后问题的直接原因；若以后引入左右脚，必须显式把 side 写入事件，不能从渲染 head-bob 的瞬时符号猜测。

### 主循环目前先 publish，后 movement

当前一帧的相关顺序是：

```text
RedrawRequested
  tree source update
  publish_spatial_frame(cached listener + cached emitters)  // L1948
  update environmental controls
  mouse/free-look update
  render + present
  update_camera_for_current_mode                       // L3937
    KCC movement
    CameraController::apply_walk_movement
      footstep play occurs inline (currently non-spatial)
    SpatialSoundManager::update_player_pos              // after apply returns
```

证据为 [`app/core/mod.rs` L1948-L1959](../src/app/core/mod.rs#L1948-L1959)、[`app/core/mod.rs` L3929-L3938](../src/app/core/mod.rs#L3929-L3938)、[`app/core/input.rs` L165-L185](../src/app/core/input.rs#L165-L185) 和 [`tracer/mod.rs` L5996-L6013](../src/tracer/mod.rs#L5996-L6013)。这意味着当前每次空间帧发表的是上一次已完成的 camera pose。对现有 non-spatial 脚步无影响；如果只把脚步调用机械地改回 spatial，新 source/play 仍可能先于包含本帧 listener 的下一次 `SpatialFrame`。

## Git 历史：以前做过什么、为何撤销

| 时间/提交 | 确切变化 | 与故障的关系 |
|---|---|---|
| 2025-08-07 [`68278efa`](https://github.com/tr-nc/re-flora/commit/68278efa02a4ab5cdef315b71fcdb34971400cc2) | `play_spatial_footstep` 调 `add_single_play_source(path, volume, position)`（[当时 `audio.rs` L114-L120](https://github.com/tr-nc/re-flora/blob/68278efa02a4ab5cdef315b71fcdb34971400cc2/src/gameplay/camera/audio.rs#L114-L120)）；camera 在积分位置后用 `self.position - camera_height` 触发（[当时 `camera.rs` L321-L370](https://github.com/tr-nc/re-flora/blob/68278efa02a4ab5cdef315b71fcdb34971400cc2/src/gameplay/camera/camera.rs#L321-L370)）。 | 建立一次性 world-space 接触点，没有 follower/lifecycle update。 |
| 2025-08-07 [`e7019d1b`](https://github.com/tr-nc/re-flora/commit/e7019d1b79fcbd9eaf462a023c6118495d5be1ed) | 标记并改动当时自研 ambisonics buffer/one-shot summing。 | 这是另一个 DSP/混音问题的证据；提交文字不足以证明它记录了“位置落后”，报告不把两者混同。 |
| 2025-10-19 [`85de7e7b`](https://github.com/tr-nc/re-flora/commit/85de7e7b29e812aa5061c015920488b5b0bd48e0) | 从旧 Audionimbus 路径迁到当时的 PetalSonic，空间脚步仍以一次性 world pose 播放。 | 更换后端没有改变 fixed-contact 语义，所以不是该方向问题的修复。 |
| 2025-10-20 [`c9a92b3a`](https://github.com/tr-nc/re-flora/commit/c9a92b3abd51d8169b44cc4290602e5f64053fc1) | 改为 `_position` + `add_non_spatial_source`（[当时 `audio.rs` L109-L165](https://github.com/tr-nc/re-flora/blob/c9a92b3abd51d8169b44cc4290602e5f64053fc1/src/gameplay/camera/audio.rs#L109-L165)）。 | 明确撤回 spatial footstep，恢复不可能产生世界方位拖尾的 2D 播放。 |
| 2025-11-01 [`7507bfd5`](https://github.com/tr-nc/re-flora/commit/7507bfd587abac45c3768501a53c63faba47d362) | 修正 listener rotation update。 | 可能影响普通空间源的方向，但发生在脚步已改回 non-spatial 之后；不能解释或修复 fixed-contact 拖尾。 |
| 2026-05-28 [`dfa9b46c`](https://github.com/tr-nc/re-flora/commit/dfa9b46c8374305f85264902b085b857907460d9) | resting stride phase 从 0 改 0.85。 | 修的是第一步节拍，不是 spatial direction。 |
| 2026-08-10 [`900b88b5`](https://github.com/tr-nc/re-flora/commit/900b88b53ac5e64790b86d2019d7dff75b336599) | 迁移到 world-owned PetalSonic runtime。 | 生命周期和原子 `SpatialFrame` API 已改变；不能原样复活 2025 代码。 |

### 旧故障的两段因果链

**A. 首块 source/listener 顺序竞态（可能发生，不保证每步发生）。** 旧 `Camera::update_transform_walk_mode` 先积分 `self.position`，再在内部创建/播放 spatial source；外层 `Tracer::update_camera` 等 camera update 返回后才 `update_player_pos`（[旧 `tracer/mod.rs` L1439-L1453](https://github.com/tr-nc/re-flora/blob/c9a92b3abd51d8169b44cc4290602e5f64053fc1/src/tracer/mod.rs#L1439-L1453)）。若音频线程恰在两次 host 调用之间处理，就会组合 current source 与 previous listener。由于没有当时的 per-block revision telemetry，不能断言这就是每次“永远不在正下方”的唯一原因。

**B. 固定世界点相对移动（确定发生）。** 旧 `add_single_play_source` 注册 `SourceConfig::spatial_with_volume(petal_pos, volume)` 后只 `play(...Once)`，没有保存 contact-follow policy 或后续位置更新（[旧 `spatial_sound_manager.rs` L104-L147](https://github.com/tr-nc/re-flora/blob/c9a92b3abd51d8169b44cc4290602e5f64053fc1/src/audio/spatial_sound_manager.rs#L104-L147)）。对任意之后的渲染时刻：

```text
source_world(t) = contact_world(t_event)
listener_world(t) = player_world(t)
local_direction(t) = listener_basis(t)^-1 · (source_world - listener_world)
```

只要玩家沿原方向前进，`source_world - listener_world` 的前向分量就越来越负，direct 声像必然转到后下方。若耳脚高度为 `h`，从 source 与 listener 正确对齐的生成时刻起又前进 `Δt`，简化后倾角量级为 `θ(Δt) = atan(v·Δt / h)`。`Δt` 必须由实际“首个含声块生成/播放”telemetry 测得，不能直接拿 ring buffer 目标填入。

本地用 `ffprobe` 审计 70 个现有素材：walk 25 个为 0.566–0.760 s、中位数 0.619 s、平均 0.627 s；run 25 个为 0.502–0.829 s、中位数 0.604 s、平均 0.620 s；全部 70 个平均 0.641 s。以 0.55 world unit/s sprint 和约 0.60 s 作纯几何示例，listener 可离开约 0.33 world unit，而耳脚高度仅 0.08；固定 source 相对“竖直下方”的偏角量级为 `atan(0.33 / 0.08) ≈ 76°`。这比拿 64 ms buffer 猜 transform age 更直接地解释长素材为何明显跑到后方。

旧 walk interval 0.35 s 小于 0.619 s 中位 clip 长度，run interval 0.25 s 也远小于 0.604 s；因此旧实现通常不止一个声源落后，而是约 2–3 个仍活跃 one-shot 分布在不同历史接触点，高速前进时形成向后的声源串。以上时长只描述文件总长，尚未测每条素材的实际能量包络；不能假定整段等响，最终严重度仍需 capture/voice envelope telemetry。素材均为 48 kHz、16-bit、mono；mono 本身适合 HRTF，不是问题来源。

## PetalSonic 0.7.0 的真实时序和传播模型

### Direct path

- 每个 spatial voice、每个 render block 都从 voice 当前 `SourceConfig::Spatial.pose` 读取位置（[`processor.rs` L544-L566](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L544-L566)）。
- direct DSP 立即用当前 `source_position - listener_position` 算距离衰减、空气吸收和异步 acoustic response 的 transmission gain；这里没有传播 delay line（[`processor.rs` L602-L655](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L602-L655)）。
- native HRTF 同一块再把当前 world delta 投影到 listener 的 right/up/front，方向没有经过 50 ms path cache（[`processor.rs` L807-L880](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L807-L880)）。
- 50 ms `DIRECT_GAIN_SMOOTHING_SECONDS` 平滑的是三频 direct gain（主要为 transmission/occlusion），不是 source pose 或 HRTF direction；early tap delay/gain 也用 50 ms 平滑（[`processor.rs` L21-L29](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/spatial/processor.rs#L21-L29)）。

因此，“关闭本地 direct 的物理传播延迟”在 0.7.0 里不是一个可修复现状的开关：direct 已经是 immediate。真正仍需的 option 7 是 **per-emitter 跳过 direct geometry/transmission，同时继续算 reflections/tail**，当前只有全局 `set_environmental_acoustics_enabled`，没有这种路由。

### 传播时间、反射、遮挡和 path cache

Acoustic worker 最多每 33 ms 解一次最新 captured spatial + scene 输入（[`acoustic_propagation.rs` L11-L17](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L11-L17)、[`acoustic_propagation.rs` L364-L450](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L364-L450)）。当前实现发现 solve 已被新输入 supersede 时会计数，但仍发布该旧 response，再等待下一轮；所以 transmission/reflection/late 参数可能比 33 ms 更旧。Re: Flora 已能记录 `acoustic_response_spatial_revision`、solve time 和 response age（[`spatial_sound_manager.rs` L302-L312](../src/audio/spatial_sound_manager.rs#L302-L312)）。

`343 m/s` 的使用范围是：

- early reflection：只延迟“反射总路径 - direct 距离”的 excess path（[`acoustic_propagation.rs` L608-L645](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L608-L645)）；
- late reverb：用最近墙面往返距离计算 pre-delay（[`acoustic_propagation.rs` L788-L796](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L788-L796)）；
- direct：无 `distance / 343` delay。

这使“有限声速导致本地脚步严重落后”在当前版本被源码否定。即使未来给 1.2 m 耳脚 direct 加物理时延，也只有约 3.5 ms，且 listener-relative 共动 source 的距离不变；它不能解释大半秒的后拖。

### Ring buffer 与设备缓冲

Balanced schedule 的 ring 是 8 blocks、low-water 2 blocks、high-water 3 blocks（[PetalSonic `engine.rs` L54-L92](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/engine.rs#L54-L92)）。Re: Flora 的 48 kHz / 1024-frame world quantum 是 21.33 ms；ring 的 3 × 1024 **device-frame** 目标只有在输出设备同为 48 kHz 时才约为 64 ms，其他设备率必须按 diagnostics 重算。Linux 还默认请求 1024-frame device buffer（[`engine.rs` L835-L889](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/engine.rs#L835-L889)）。

但 **64 ms 不是已证实的 HRTF transform age，也不是固定 event-to-ear latency**。Render pump 每次先消费最新 `SpatialFrame` 和 commands，再看 occupancy；若已经达到 high-water，就不生成新样本，已有预渲染样本先被 callback 播放（[`engine.rs` L1301-L1368](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/engine.rs#L1301-L1368)）。下一批含脚步样本的方向取“实际生成该批时”的 pose，然后排在已有样本之后。因此：

- ring/device buffer 确定增加 event → audible 的视听延迟；
- 它可能让玩家在听到第一声前已经走远；
- 但不能未经 telemetry 就把完整 64 ms 位移说成已经编码进首块 rear direction。

需要同时记录 command acceptance、生成首个非零 sample 时的 transform revision/ring occupancy，以及预计 device presentation time，才能拆开 command latency、render-ahead 和 world-source 相对运动。

### Source / voice 生命周期

PetalSonic 把 emitter 与每次 play 的 voice 分开：

- `play(...once())` 的 voice 播完自动回收，但 emitter 不自动销毁（[`domain.rs` L288-L350](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/domain.rs#L288-L350)）。
- attached voice 每个 spatial frame 跟随其 emitter；detached voice 在 emitter 销毁后继续播放，但停止跟随更新（[`engine.rs` L1488-L1503](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/engine.rs#L1488-L1503)、[`playback.rs` L89-L105](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/playback.rs#L89-L105)）。
- `play_controlled(..., PlaybackTag)` 能通过 completion event 管理 host 生命周期（[`world.rs` L842-L911](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs#L842-L911)）；当前 Re: Flora 只把所有 events debug log 掉，没有把 completion 路由回脚步池（[`spatial_sound_manager.rs` L272-L274](../src/audio/spatial_sound_manager.rs#L272-L274)）。

不能简单按 70 个 clip path 永久缓存 spatial emitter：所有登记 spatial emitter 都必须进入每个完整帧并参与 acoustic candidate 排序，即使当前没有 voice；而一个 emitter 的重定位会让它所有尚在播放的 attached voices 一起移动。对统一的 listener-local 脚下 anchor，重叠 voice 一起跟随同一 local offset 在语义上可接受，但 70 个空闲 acoustic emitter 不是好预算。

PetalSonic 0.7.0 在 `create_emitter(clip, desc)` 时绑定 `ResidentClip`，`update_emitter` 只能改 desc，没有更换 clip 的 API（[`world.rs` L677-L717](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs#L677-L717)）。因此下文的“小池/cap”准确含义是**有上限的 active set**：每个已选定随机 clip 的 pending event 创建并登记自己的 emitter，完成后销毁；不是让六个固定 emitter 任意换绑素材。若以后要复用 emitter，只能按相同 clip 管理空闲缓存和 active voice 引用计数，并仍须限制进入完整 spatial frame 的空闲项。

对 world-contact 层则绝不能在旧 voice 尚活跃时复用并重定位同一个 emitter，否则旧 voice 和按 emitter identity 索引的 acoustic response 都会被新脚步带到新接触点，重造另一种“历史脚步跳位”。现有 API 下应让每个重叠 contact 持有独立 active emitter/controlled slot，直到 completion 后才销毁/复用；detached voice 只适合已接受“不再跟随 emitter，也不再持续获得该 emitter 新 acoustic response”的路径。长期 API 可以进一步把 `acoustic_world_pose` 捕获到 per-play/voice identity，而非隐含每个 emitter 同时只有一个 contact。更稳妥的最小实现是只维护有上限的 active one-shot emitter set。

## 把“落后”拆成八个可证伪候选

| 候选 | 当前证据与判断 | 如何单独观测/排除 |
|---|---|---|
| 1. 事件晚发 | regular step 只受 stride 跨阈值的帧量化；第一步有意延迟 15% interval。landing 在本帧 KCC 结果后立即触发。可造成按键/接触到声音晚，但不造成播放中方向继续后移。 | 每步记录 `event_seq`、理想 phase crossing sim time、实际 fire frame/time、ground contact time；用 impulse clip 对比 animation/contact marker。 |
| 2. source 使用上一帧位置 | 当前 regular/landing 的 event pose 来自本帧 post-KCC collision position；jump 是 pre-translation。当前空间帧却在 movement 前 publish。未来 naïve spatial 接入可能令首块 frame/source revision 不一致。 | 在首次含声 render block 记录 `event_seq`、source pose revision、post-KCC foot pose；比较而不是只看 game-thread log。 |
| 3. listener/source update 顺序 | 旧实现确有“play source 后 update listener”的窗口；当前也在 camera apply 内触发声音，之后才缓存 listener，且 spatial frame 到下一帧才 publish。它解释首块偶发错误，但不是后续持续拖尾的必要条件。 | 每块暴露所用 `SpatialFrame.revision`、listener pose、emitter pose、play command seq；断言首次有声块来自同一完整 revision。 |
| 4. 每步固定在世界接触点 | **最强、确定性主因。** 旧 source 创建后不更新；listener 继续移动，所有后续生成块的 local vector 自然转后。快速移动和长 clip 使症状更严重。 | 每个含声块记录 `dot(source-listener, movement_forward)` 与 local direction；固定点模型会随时间越来越负。静止时消失、速度提高时斜率近似线性。 |
| 5. 有限声速/传播延迟 | 当前 PetalSonic direct 没有该 delay；只有 reflection excess path 和 late pre-delay。源码排除它作为当前 direct 严重落后的原因。 | impulse test 分离 dry direct 和 wet tail；记录 first direct sample 与反射首达。若关闭 environmental 后 direct onset/方向不变，则不是 propagation worker。 |
| 6. audio buffer latency | Balanced 约 3-block occupancy + device buffer 增加 event-to-ear 延迟，但不是固定 transform age；会使听到时角色已前进，也会放大“事件与画面不同步”的主观感。 | 记录 command time、ring occupancy、first nonzero render sample、callback consumption/device timestamp；外部 loopback 同录输入/画面 marker。切 Responsive/设备 period 只作为诊断 A/B。 |
| 7. smoother 或 path cache 滞后 | 33 ms solve 与 50 ms gain/tap smoothing 会滞后 transmission/reflections/tail；direct HRTF direction 每块用当前 delta，没有 50 ms pose smoother。 | 记录 current spatial revision、response spatial revision、response age、direct local direction。全局关闭 environmental：若 rear direction 仍在，则 path cache 无罪；若仅遮挡/尾响响应变快，则归因成立。 |
| 8. 左右脚/相机/角色坐标 seam | 当前 source 是中心线脚点，无左右脚 offset；listener 用 camera vectors/position，但视觉 head bob 只改 view matrix。它不能解释“速度越快拖得越远”，但可能解释静止转头时固定方位错误。 | 静止原地 yaw/pitch，记录 collision anchor、visual camera、listener basis、foot side、listener-local vector；用已知右/上/前 impulse 检查轴符号。左右脚必须由 semantic event 给出。 |

关键判别优先级：先做 4 的 relative-vector trace，再做 3/6 的首块时序；不要先调 HRTF、声速或 smoothing。一个最小实验是“原地触发 + 直线 sprint 触发 + environmental off”，它已能把坐标 seam、固定世界点和 path cache 三类拆开。

## 其他引擎的一手惯例能说明什么

这里比较的是官方 API/工作流提供的选择，不声称所有商业游戏都使用同一个配方：

- Unity 官方 `AudioSource.spatialBlend` 明确把 `0` 定义为全 2D、`1` 为全 3D；[`PlayClipAtPoint`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AudioSource.PlayClipAtPoint.html) 在指定 world position 创建临时 source 并在 clip 结束后销毁。[Unity `spatialBlend`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AudioSource-spatialBlend.html)
- Unreal 的 Animation Sound Notify 专门列脚步等 foley，并明确写出：`Follow` 开启则跟随 mesh；关闭则声音留在 spawn location。这个官方说明与本故障几乎是同一个语义分叉。[Unreal Animation Notifies](https://dev.epicgames.com/documentation/en-us/unreal-engine/animation-notifies-in-unreal-engine)
- Unreal `Spawn Sound Attached` 支持相对 attach point offset，适用于需要跟随对象的 spatialized/distance-attenuated 声音；`PlaySoundAtLocation` 则是不可再修改的 fire-and-forget。[Spawn Sound Attached](https://dev.epicgames.com/documentation/unreal-engine/BlueprintAPI/Audio/SpawnSoundAttached?lang=en-US)、[Audio Engine Overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/audio-engine-overview-in-unreal-engine)
- Valve 的 Steam Audio integration guide 建议直接声与间接声可用独立效果、由音频图决定混合；也建议 occlusion 在主 update 线程运行以避免可闻滞后，而 reflections/pathing 可放其他线程。这是一手依据，支持“即时 dry direct + 异步 world wet”的分层，而不是让一个滞后的传播结果决定全部声音。[Steam Audio integration guide](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html)
- Steam Audio 的 direct effect 把 distance/directivity/occlusion/transmission 与 reflections 分开，且 listener-centric reverb 可把虚拟 source 放在 listener 位置、使成本不随 source 数增长。[Steam Audio programmer guide](https://valvesoftware.github.io/steam-audio/doc/capi/guide.html)

## 七种方案比较

| # | 方案 | “绝不能落后”保证 | 脚下/材质/环境感 | 当前 API 与决策 |
|---:|---|---|---|---|
| 1 | 本地脚步不空间化，2D/head-locked | **最强。** 根本没有世界方位，所以不可能被听成 rear world source。 | 保留素材/表面音色；没有脚下方位、遮挡与空间 tail（除非另加 wet）。 | 已实现，是安全回退与对照，不是终点。 |
| 2 | listener-local 下方固定、immediate direct | **强保证。** 若 source/listener 来自同一原子 frame，local offset 恒定；旧帧或缓冲中的块也仍是相同“下方”。 | 有下方 HRTF 和距离；若用同一 moving emitter 做声学，room response 有但接触点随人移动。 | Petal 0.7.0 可由 host 每帧换算 world pose实现；需修事件/发布顺序和生命周期。**最小推荐。** |
| 3 | 脚底世界接触点 fully spatialized one-shot | **不能保证，且移动后必然后移。** | 最真实的世界接触点、遮挡和反射；适合 NPC、掉落物、远处脚步。 | API 足够，但不应作为本地玩家占主导的 direct。 |
| 4 | perceptual split：listener-relative dry/direct + world reflection/environment tail | **direct 强保证；tail 可在身后是有意的环境记忆。** | 同时保留即时脚下定位、表面 clip 和真实空间 tail。 | 当前 API 不足；需 direct/acoustic 双 pose 和 per-emitter route。**长期推荐。** |
| 5 | emitter 每帧跟随 player | 平移可保证；若只是“追踪上次 player world pose”而不是 listener-local 原子约束，首块/转头仍可能有 seam。 | direct 不拖后；反射和尾响跟人移动，世界接触感被涂抹。 | 当前 API 足够；应把它规范成方案 2 的 listener-local invariant，而非松散 chase。 |
| 6 | motion prediction/source compensation | **无保证。** input、render occupancy、device latency 和突然停步/转向都可变化，预测会过冲。 | 可让远处 world source 在稳定速度下视觉更同步，但牺牲真实接触点。 | Host 可做，不推荐用于本地脚步 correctness；只可作为测量后的可选 polish。 |
| 7 | 禁用本地 direct 的物理传播 delay，保留 occlusion/reflections | 单独不能修 fixed-world 后移；当前 direct 本来就无物理 delay。若再配合 listener-local direct 才有保证。 | 可保留环境互动，避免 local direct 被异步 geometry 突变。 | 全局开关做不到 per-emitter；长期需要 direct geometry bypass / wet route。 |

**回答用户的两个核心取舍：**

- 真正保证“绝不能落后”的是 1，以及满足“同一原子 frame + listener-local 恒定 offset + play 在该 frame publish 后”的 2/4 direct。5 只有被约束为 2 时才同样可靠。6 永远只能近似。
- 保留最多空间/地面材质/环境感的是 4。3 保留物理世界感，却与“本地 direct 绝不后拖”目标冲突；可只作为 4 的 wet/tail 坐标语义。

## 本项目最小方案：现有 PetalSonic API 内完成

### 不可省略的事件 seam 与顺序

不能只把 `publish_spatial_frame` 从帧头挪到 movement 后；当前 footstep play 发生在 camera apply 内，而 listener update 在 apply 返回后。最小设计必须先把播放副作用移出 controller：

```text
CameraController::apply_walk_movement
  -> 只产生 FootstepEvent { seq, kind, side?, contact_world, surface?, speed, sim_time }

App/Tracer 每帧：
  1. movement / KCC 完成
  2. 先消费 completion 并移除已结束 emitter；再消费 FootstepEvent，解析 surface/clip/gain，并为新事件创建、登记 emitter
  3. 获取同一份 post-movement listener snapshot
  4. 为全部 active local-footstep emitter 计算 listener-local offset 对应的 world pose
  5. publish complete SpatialFrame(listener + all spatial emitters)
  6. play 已准备且已进入该 frame 的 footstep emitters
```

第 6 步在第 5 步之后，使 Petal render pump 先消费完整 frame，再处理 play command；Petal pump 本来就是 `consume latest spatial → set listener → process commands → generate`（[`engine.rs` L1313-L1335](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/engine.rs#L1313-L1335)）。若创建新 spatial emitter，必须在 publish 前将它加入完整 frame；不能先 play、下帧再补。

### Direct anchor 和参数起点

**建议起点，不是调音结论：**

- listener-local offset：`(0, -0.08, 0)` world unit；先用中心线，确认真正的 left/right foot event 后再尝试 `x = ±0.01..0.02`。
- direct propagation：immediate（当前即如此）；不要对 local direct 做 motion prediction。
- follower pose：不做位置平滑。增益/环境参数可继续平滑；任何 pose smoother 都会重新引入相对误差。
- active footstep voice/emitter 并发上限：从 6 起测。run interval 0.25 s、最长 run clip 0.829 s，active set 至少要容纳 4 个重叠 run voice；6 为 jump/land 和未测的有效尾声留余量。
- 超限策略：先结束/复用最老且已低于阈值的 local foot voice，记录计数；绝不能无限创建 emitter。

### 最小文件边界

- `src/gameplay/camera/controller.rs`：只决定步态、jump/land/step semantic event 和准确 contact position，不直接调用音频 world。
- `src/gameplay/camera/audio.rs`：clip family、速度到 gain、surface 到 bank 的 content policy；不负责 listener 或 Petal emitter transform。
- `src/tracer/mod.rs`：暴露/转交 pending footstep events；不在 camera apply 内 play。
- `src/app/core/input.rs` / `src/app/core/mod.rs`：拥有上述六步 orchestration，确保 movement/publish/play 顺序。
- `src/audio/spatial_sound_manager.rs`：有上限的 per-event active emitter set、controlled completion、完整 spatial-frame 合成、每帧 listener-relative follower 更新和 telemetry。
- `src/app/core/physics.rs` 与 `src/builder/contree/mod.rs`：把 ground contact position/voxel type 转成稳定的 gameplay `FootstepSurface`；不要把 Petal `AcousticMaterial` 当音效素材 ID。

### 现有 API 是否够用

**够用但 adapter 不够。** `EmitterDesc::spatial`、`EmitterSpatialState`、atomic `SpatialFrame`、attached voice、`play_controlled` 和 completion event 已能实现 active local follower。Re: Flora adapter 仍需：

- 一个 spatial one-shot create/play API；
- `PlaybackTag → event_seq/emitter` 生命周期表；
- 在构造下一份完整 frame 前消费 completion，从 host registry 移除并 `destroy_emitter`；
- 先 publish 再 play 的批处理入口；
- 每块/首次 onset 诊断所需的 Petal telemetry（后一项需要 Petal 扩展）。

该最小方案会让 direct 不后拖，并能经过现有 native HRTF。若让 environmental acoustics 继续作用于同一个 follower emitter，它也有反射/尾响，但 acoustic source 会随 listener 移动，不能声称是固定落脚点的物理 tail。因此它是最小可接受空间感，不是最终物理模型。上线前仍应保留 2D dry feature flag 作为 A/B 与安全回退。

## 长期方案：一次解码的 direct/acoustic 双空间语义

### 推荐 PetalSonic 接口

目标是一个 voice 读一次 PCM，再分别送入 direct 与 environment 路径，避免播放两份 clip 造成 onset 偏移、相位叠加和双倍解码。`direct` policy 可以稳定地属于 emitter，但每一步的 world contact 必须在 play 时捕获到 voice；否则同一 clip 的下一步会重定位上一脚的环境响应。概念接口可以拆成：

```rust
pub enum DirectPlacement {
    World,
    ListenerRelative(Pose),
    Disabled,
}

pub enum DirectObstruction {
    SimulatedTransmission,
    BypassTransmission, // local dry: skip async occlusion/transmission only
}

pub enum DirectPropagation {
    Immediate,
    Physical, // future; current renderer only implements Immediate
}

pub struct EmitterSpatialState {
    pub emitter: Emitter,
    pub world_pose: Pose, // existing/default pose for ordinary world emitters
    pub direct_placement: DirectPlacement,
    pub direct_obstruction: DirectObstruction,
    pub direct_propagation: DirectPropagation,
    pub environment_send_db: f32,
    // existing acoustic_priority remains
}

pub struct SpatialPlayOptions {
    pub acoustic_world_pose_override: Option<Pose>, // captured per voice/contact
    // regular PlayOptions and completion tag remain
}
```

也可把低频不变的 routing policy 放 `EmitterDesc`，把动态 `world_pose` 留在 `EmitterSpatialState`；关键是 immutable contact override 属于 voice/play，且不要把 policy 散成 Re: Flora 对内部 DSP 的多个开关。普通 world emitter 不给 override，声学 worker 继续使用 frame 中的 `world_pose`。兼容默认值应等价于当前 `World + SimulatedTransmission + Immediate + 0 dB send`。

处理语义：

1. direct renderer 对 `ListenerRelative` 直接使用 local pose，不先换成会过期的 world source；`BypassTransmission` 只跳过异步 direct transmission/occlusion，仍保留 HRTF 以及明确选择的近场距离/空气增益。
2. acoustic worker 以 voice identity 读取 play 时捕获的 `acoustic_world_pose = contact_world`，求 direct geometry（若需要）、early taps 和 late parameters；不能只按 emitter identity 保存一个会被下一步覆盖的 contact。
3. local footstep 设 `direct_placement = ListenerRelative(feet_offset)`、`direct_obstruction = BypassTransmission`、`direct_propagation = Immediate`；environment send 使用 per-play 世界 contact。若产品希望保留 direct occlusion，可独立选 `SimulatedTransmission + Immediate`；这正是把“是否有物理直达延迟”和“是否做遮挡”拆开的意义。
4. `environment_send_db` 建议从 direct 的 `-12 dB` 起扫 `-9..-15 dB`；这是感知调音起点，不是标准值。
5. direct 与 wet 必须来自同一个 voice cursor/sample block，确保 sample-accurate onset；world tail 允许在玩家离开后留在原处，因为其音量较低且被感知为房间响应，而非主脚步实体。
6. 若业务继续复用 clip-level emitter，`acoustic_world_pose` 必须是 per-play/voice 捕获值；兼容实现做不到时，每个 active contact 使用独立 emitter identity，直到 controlled completion。
7. 当前 normal completion 以 dry clip cursor 结束为准，render thread 随即退休该 voice 的 spatial state。实现 world wet 时必须验证 early-delay tail 是否会因此被截断；若会，应让 per-voice early processor 进入有界的 tail-drain 状态，而共享 late FDN 可继续自然衰减。

### PetalSonic 文件边界

- `src/domain.rs`：公开稳定 policy 与完整 spatial state；保留兼容默认。
- `src/world.rs`：验证完整 frame、per-play acoustic pose、policy 值和 lifecycle，不暴露 DSP 细节。
- `src/spatial/processor.rs`：同一 mono block 分 direct/wet；listener-relative direct direction；per-emitter geometry bypass；不要平滑 pose。
- `src/acoustic_propagation.rs`：按 active voice/contact 使用 `acoustic_world_pose`，并应丢弃 superseded solve 而不是发布旧 response。
- `src/engine.rs` / diagnostics domain：增加 command→first-render→callback presentation 可观测链，以及 render block 使用的 spatial revision。
- Petal 单元/离线渲染测试：验证 local direct invariant、wet delay、单 voice cursor 对齐和兼容默认。

### 地面材质

长期的 `FootstepEvent` 应携带稳定语义，例如 `FootstepSurface::{Soil, Sand, Wood, Stone, Stucco, Unknown}`，由 Re: Flora 将 `voxel_type` 映射到 clip bank。Petal `AcousticMaterial` 继续只表达 absorption/scattering/transmission。这样同一次脚步：

- dry 的音色来自 surface clip；
- direct 方位来自 listener-local feet；
- reflection/tail 的频谱与路径来自 contact_world 周围的声学几何。

当前只有 undergrowth 素材，未增加其他 bank 前，验收只能证明路由与 material ID 正确，不能声称已实现听觉上的木/石/沙差异。

## 性能和资源预算

- **不要登记 70 个永久 spatial one-shot emitter。** Petal 完整帧必须列出所有 spatial emitter，acoustic worker 当前会从登记项中选择最多 32 个 direct、最多 4–8 个 early source（质量计划见 [`acoustic_propagation.rs` L19-L41](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/acoustic_propagation.rs#L19-L41)）。空闲脚步 emitter 会挤占排序/帧构造工作。
- 最小 active set 只保留仍播放的脚步，建议并发 cap 6；每个 completion 后 active emitter 数必须回到基线。由于 emitter 绑定 clip，这首先是 per-event create/destroy 的容量上限，不是假定可任意换 clip 的对象池。
- 方案 4 应使用单 voice 双 send，不播放两份 PCM。这样不增加 decode/cursor 数，只增加少量 routing；early/tail 本来已有有界预算。
- local dry 不需要 acoustic direct solve；将 `direct_obstruction = BypassTransmission` 后可以避免把本地脚步计入 direct occlusion source cap，但它的 world wet 仍可按环境 send/当前响度给 acoustic priority。
- 所有性能结论以 release hidden 固定脚本和音频 diagnostics 为准；普通 `cargo test` 只验证确定性状态机/数学，不作为音频延迟或 CPU 证据。

## 回归测试与可观测验收

### 必须先补的 telemetry

每一步以同一 `event_seq` 串起：

```text
FootstepEvent:
  event_seq, kind, side, surface, sim_time, fire_frame,
  contact_world, post_kcc_listener_world, velocity

Spatial publication:
  spatial_revision, listener_pose, direct_local_pose,
  acoustic_world_pose, publish_time

Petal render/onset:
  play_command_seq/accepted_time,
  first_nonzero_render_block, spatial_revision_used,
  listener_pose_used, direct_pose/vector_used,
  acoustic_response_revision/age, ring_occupancy

Device estimate:
  callback frame index / backend timestamp where available
```

没有这些字段，“听起来晚”无法区分事件、渲染、排队和设备，“听起来后”也无法区分 pose、HRTF 坐标和 world contact policy。现有 diagnostics 只有 underrun/queue/acoustic response age 等汇总（[PetalSonic `world.rs` L1160-L1220](https://github.com/tr-nc/petalsonic/blob/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic/src/world.rs#L1160-L1220)），不足以闭环单步 onset。

### 确定性测试

1. listener-local `(0, -h, 0)` 在任意 world translation/yaw 下换算 world 再变回 local，结果仍为 `(0, -h, 0)`。
2. 首个含声块的 listener 与 emitter/direct pose 必须来自同一 `SpatialFrame.revision`；不允许 play 使用尚未发表的新 emitter pose。
3. 两步重叠时各 voice cursor 独立，旧 voice 的 acoustic world origin 不因新 contact 改变，direct local anchor 始终相同；completion 后 emitter/voice/registry 计数回基线。
4. direct impulse 第一块没有 `distance / 343` silence；world wet 在 direct 后有延迟能量，direct 不被重复混入。
5. environmental on/off 时 local dry 样本和 direction 完全一致，只有 transmission policy允许项和 wet 不同。
6. stride 测试继续覆盖 first-step 15% interval、regular step frame quantization、landing immediate、空中无 regular step。
7. surface 映射用固定 voxel fixture 验证 Soil/Sand/Wood/Stone/Unknown，不依赖随机音频选择。

### 可听与脚本化场景

| 场景 | 通过条件 | 主要排除项 |
|---|---|---|
| 快走直线 | dry local vector 全程在脚下容差内；不随 clip 年龄向 rear 漂移；event/onset latency 有分段数据。 | fixed world、上一帧 source、buffer 混淆 |
| 冲刺直线 | 相比快走，direction 误差不随速度增长；无“拉橡皮筋”声像；voice cap 不触发或触发可解释。 | 固定接触点主因的回归 |
| 移动中快速转头/转身 | listener-local dry 仍在下方；world wet 随头部 orientation 正确旋转，左右/前后符号不反。 | listener/source order、坐标 seam |
| 原地转头 | 无平移时四个已知方向 impulse 与 right/up/front 约定一致；脚步仍下方。 | HRTF/相机 basis seam |
| 起跳与落地 | takeoff 使用离地 contact，空中无 regular step；landing 使用实际着地 contact/surface；移动落地不会重复 step + land。 | 事件晚发、jump pre/post pose |
| 狭窄走廊/小房间 | dry onset/direction 与室外相同；early/tail 更明显且 response age 有界；关 environmental 后只消失环境层。 | solver/path cache、wet routing |
| 连续跨材质 | event surface 与脚下 voxel 一致，clip bank 对应；world tail 使用相同 contact 周围声学材质。 | gameplay/acoustic material 混层 |
| 突停、倒退、180°反向 | direct 不因预测过冲跑到前/后；world tail 留在旧 contact 是预期且低于 dry。 | motion prediction 风险 |

建议固定 48 kHz/1024 block、固定 camera script 和可重复 impulse/脚步 clip 做离线或 loopback capture，并至少比较四个开关组合：current 2D、listener-local follower、world-fixed、split dry+wet。验收报告应同时呈现方向 invariant、event-to-render、render-to-device 估计、acoustic response age、underrun 和 active emitter/voice high-water；只凭现场主观试听不能证明“绝不落后”。

## 分阶段决策

1. **安全基线：** 保持当前 2D 脚步，先落 telemetry 和 semantic event seam。此阶段已经保证不后拖。
2. **最小空间接入：** active listener-local emitter + 原子 frame + publish-before-play；环境先跟随 emitter，明确记录其物理局限。若快走/冲刺/转头任何一项 direction invariant 失败，立即回退 2D，不上 motion prediction。
3. **材质接入：** ground contact/surface seam 与对应 clip banks；这与 Petal acoustic material 分层验证。
4. **长期 split：** PetalSonic 增加 direct/acoustic 双 pose、per-emitter direct bypass 和单 voice wet send；在狭窄空间验证 tail，再替代最小 follower 的环境路径。

最终推荐不是把旧 [`68278efa`](https://github.com/tr-nc/re-flora/commit/68278efa02a4ab5cdef315b71fcdb34971400cc2) 重做一遍，也不是用预测把 world source 推到玩家前面。正确的不变量是：**本地玩家听到的主脚步 direct 永远属于 listener-local 脚下；世界接触点只属于材质查询和低电平环境响应。**

## 一手外部来源

- [Unity `AudioSource.spatialBlend`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AudioSource-spatialBlend.html)
- [Unity `AudioSource.PlayClipAtPoint`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AudioSource.PlayClipAtPoint.html)
- [Unity `AudioSource.PlayOneShot`](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AudioSource.PlayOneShot.html)
- [Unreal Engine: Animation Notifies / Sound Follow](https://dev.epicgames.com/documentation/en-us/unreal-engine/animation-notifies-in-unreal-engine)
- [Unreal Engine: Spawn Sound Attached](https://dev.epicgames.com/documentation/unreal-engine/BlueprintAPI/Audio/SpawnSoundAttached?lang=en-US)
- [Unreal Engine: Audio Engine Overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/audio-engine-overview-in-unreal-engine)
- [Valve Steam Audio: Integrating Steam Audio](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html)
- [Valve Steam Audio: Programmer's Guide](https://valvesoftware.github.io/steam-audio/doc/capi/guide.html)
- [Valve Steam Audio: Simulation API](https://valvesoftware.github.io/steam-audio/doc/capi/simulation.html)
- [PetalSonic 0.7.0 source at release commit](https://github.com/tr-nc/petalsonic/tree/06d992f755fdc17a26b52a4eef97341ebe8d6e12/petalsonic)
