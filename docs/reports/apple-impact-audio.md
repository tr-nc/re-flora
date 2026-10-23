# 苹果落地音效与独立混音

## 行为与所有权

- 恢复 `assets/sfx/apple-dirt/` 的三条原始录音、三条去前置延迟版本和完整来源说明。运行时只预加载三条裁切版，未修改 WAV 字节。
- 物理世界在每个固定子步之后识别动态刚体首次受到静态地面支撑的实际碰撞：有正碰撞冲量、朝上法线和 solver contact。预测接近、树表面以及动态刚体之间的碰撞不算落地。
- 每个动态苹果刚体生命周期只产生一次落地事件。静止、轻微弹跳、渲染帧率变化不会重复触发。刚体删除会清理去重状态和未消费事件，新掉落的苹果可以产生新事件。已落地刚体被扰动不重新触发。
- `TerrainPhysics` 将真实接触点从 voxel 单位转换为 world 单位，并只转发仍属于苹果的刚体事件。空间音频帧消费事件后不留重放队列。
- 每次独立等概率选择三种音效之一，允许连续选中同一条；原速、单次播放。声源位于接触点上方半个 voxel，避免放在土内部。
- `AppleImpactAudio` 负责准备、空间发布后开始播放、结束后的资源回收。128 个总槽位包含音频线程仍持有的退役引用；极端超载时明确记录并丢弃多余事件，不延后补播。关闭、暂停、读档及退出走相同退役路径。
- 录音到期加 0.5 秒后回收 emitter；连续安全包络负责停止/过期静音，不改变冲击的节奏。源增益抵消安全包络的 0.72 稳态增益，净前级倍率为 1，之后正常应用混音和空间衰减。

## 可保存的音量

`Debug → Audio → Live mixer`：

- **Sprinklers enabled / scale (x)**：喷头独立音量，不再跟 Digging / terrain editing 共用。
- **Apple impacts enabled / scale (x)**：苹果落地独立音量。
- **Master**：仍控制全部声音，`M` / CLI `--mute` 的主静音保持有效。

两个新通道默认 enabled、1x，旧存档缺字段可正常加载。控件使用现有 saved-field-bound 入口，点击 Save 即持久化，没有 App-only 滑杆或逐设置保存钩子。独立混音与实际保存/加载回归通过。

## 验证

- `cargo fmt --check`、`cargo check`、默认验证层 `cargo build --release`。
- 完整应用测试：1686 passed、11 ignored；另有 4 个库测试及 1 个显示授权测试通过。
- `cargo test -p re-flora-physics`：70 个测试通过。新增真实物理回归覆盖 15/60/144 FPS、空中不触发、首次接触一次、静止和弹跳不重复、删除重建可再触发；动态刚体互撞不触发落地事件。
- 显式原生音频所有权测试：三个随机候选均被选到；准备阶段不播放；空间发布后播放；空事件帧不重复播放；128 槽上限；期限回收；关闭时不积压；发布失败不遗留未播放资源。
- 原生测试固定不存在的音频设备，ALSA 探测告警不是应用运行失败，也不作为实际听音证据。
- 隐藏静音 Release 真掉落场景：
  ```sh
  RUST_LOG=info RE_FLORA_FRUIT_GROUND_TRACE=target/apple-impact-validation/legacy-ground.jsonl \
    cargo run --release -- --legacy-terrain --hidden --mute --auto-exit 15
  ```
  日志 `target/re-flora-logs/re-flora-20261024-005903.061-165059.log`。核对物理 trace 的 17 个 body ID 与播放日志完全一致，每个恰好一次，三个候选均出现；最后 `all voices reclaimed`、shutdown `failures=0`，无 ERROR/VUID/panic。
- 首次普通新花园运行没有可掉落苹果，trace 为空，**不计为落地行为验证**；随后使用有结果的 legacy tuning-tree 场景。
- 三原件及三裁切版全部 SHA-256 与随包 provenance.json 相符。资产清单只加入三个运行时版本。

本轮不自动启动可见游戏，不宣称已经过人工听感、实际扬声器响度或物理设备端到端延迟验收。已有素材使用限制保持在随包说明中；没有推送或发布。
