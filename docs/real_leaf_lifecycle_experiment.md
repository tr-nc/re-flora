# 共享叶模型、风中旋转与真实落叶

实现位于 `pretty-leaves`。生命周期设计见[风致脱落调研](research/wind_leaf_detachment.md)。**足够强或持久的风允许把树吹秃；不加保叶下限、概率豁免、发射限流或延迟队列。**

## 两个独立的可保存开关

Debug → **Falling Leaves**：

### Attached Leaves Rotate in Wind

默认未勾选。两边都是同一个新叶模型，使用相同的确定性初始朝向（包括绕叶面法线的随机转角）。

- **A，未勾选**：位置跟随树枝与叶位运动，完整的世界空间朝向保持初始值，不在树上随风自转。
- **B，勾选**：在同一初始朝向上施加已有 GPU 风响应的叶柄旋转，实际叶缘、叶尖和轮廓改变，不只是光照法线变化。
- 脱落后两边都使用已有落叶飞行模型，从实际发布的模型姿态开始旋转。
- 切换只影响仍附着的叶子；不重抽种子、不重置再生、不清除或重新定向已经脱落的叶子。回到 A 恢复原初始朝向。

### Real Detachment + Regrowth

默认未勾选，保留此前生命周期实验：

- A：原装饰性发射器，树上叶位不减少；装饰落叶也使用新模型。
- B：停用装饰发射，每个原可见叶体素/socket 是一片真实叶；附着叶消失与对应落叶诞生原子发布。
- **这个生命周期开关**的两个切换方向仍清理落叶、重置叶位。蝴蝶、水滴和地形碎屑不受影响。B 再次进入从完整冠层开始。
- 生命周期仅保留在当前运行期，不含存档或离线生长。暂停世界更新、隐藏叶子或停用粒子模拟会暂停 B；模式复位仍可用。
- 树拓扑替换重建该树叶位；已脱落叶继续飞行，删除树不会召回它们。

| 生命周期参数 | 默认值 |
| --- | ---: |
| Connection Strength Multiplier | 1 |
| Connection Weakening Half-life | 120 s |
| Empty Socket Recovery | 8 s |
| New Leaf Growth | 20 s |

设置全部由 `config/gui.toml` 声明，使用统一 Save。生命周期参数只在 B 可编辑；**模型尺寸和分辨率现在同时控制附着、装饰落叶、真实落叶**，不再受生命周期模式限制。显示尺寸默认 1（范围 0.25–4），像素分辨率默认 22（范围 8–64）；不改变物理尺寸或气动力。旧配置保留用户填写的尺寸/分辨率，补充默认关闭的旋转开关，并移除 `falling_leaf_mesh` 和旧的外观控件禁用条件。

## 渲染与发布边界

**叶子旧 sprite 与真实落叶的源 cube 绘制路径已经退役。** 水滴、地形等通用粒子仍保留自己的路径；非叶子的采收/调试光学点明确归为 `LitDebris`。

- `shader/slang/leaf_model_pose.slang`：附着绘制与脱落交接共用的完整姿态。稳定的世界叶位种子决定初始朝向与 64 种共享模型之一；不会由新粒子槽位重抽形状。
- `shader/slang/tree_leaf_model.comp.slang` / `src/tracer/leaf_models.rs`：从活叶索引、GPU 树姿态和风响应产生共享模型实例。**不把所有附着叶搬入 CPU 粒子模拟，也不逐帧回读其姿态。**
- 附着、LOD 和飞行叶共用模型表面缓存、逐对象照明、像素 tile 和 fragment display。LOD 仍做树级距离选择，不切回 cube。旧树叶体素照明缓存不再分配或调度。
- `src/tracer/leaf_handoff.rs` / `leaf_handoff.comp.slang`：事件时批量回读上次发布姿态，转移位置、尺寸、颜色、模型种子、完整旋转及速度。一个批次一次同步，不是每片一次等待。新拓扑需先正常发布才可交接。
- 可见源实例仍为 **8 bytes**，占用/生长为独立 **4-byte/slot** 流。模型输入、索引和 transient tiles 属于已取得 fence 的 frame slot；不会覆盖在飞 draw 的缓冲。
- 阴影仍是按存活面积加权的粗代理；空代理消失。它不是新薄叶轮廓的逐片精确阴影。

## 生命周期与数量所有权

`src/leaf_lifecycle.rs` 独立拥有叶位规则，身份为 `(tree, topology, socket, generation)`。只读 `plan` → 成功交接 → `commit`，拒绝过期/重复事件；来源身份保留到粒子消亡，槽位复用会清除旧身份。

局部负载为风速向量模长平方乘生长面积 `growth²`；连接强度为：

```
multiplier * [0.09 + (initial_strength - 0.09) * 2^(-mature_age/half_life)]
```

正下限防止静风自行脱落，不保证强风下保叶。空叶位恢复与长叶独立于旧叶飞行，每代重新生成确定性连接强度。应用发布 owner 是 `src/app/core/vegetation/leaf_lifecycle.rs`。

- `INITIAL_PARTICLE_CAPACITY = 16_384` 是粒子系统初始预留，不是渲染器叶数上限。真实批次先完整预留再提交，模拟存储随需求扩展。
- 模型渲染器移除了独立叶数准入限制；实例、三角形、排序索引和 tiles 按帧需求分配。
- 单个 tile 批次受 64 MiB 存储预算和 Vulkan 保证的 65,535 Z dispatch 范围约束；超出后在**同一帧分批绘制**，不是少画或延迟脱落。
- GPU 地址范围、单缓冲资源限制及有限内存仍存在，越界明确报错；不承诺任意规模实时运行。连续数值诊断有自己的有界 fixture 容量，不是生产叶数策略。

生态按活叶索引与再生代数验证宿主；幼叶达到半大后提供栖息位。树叶沙沙声功率乘活叶面积比例及原有代际 crossfade，秃树无叶片功率；不逐片重建声场。

## 验证

```
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
scripts/check_real_leaf_lifecycle.sh
env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock node scripts/validate-leaf-model.mjs
```

- 单元测试：**1117 passed / 4 ignored**，collision bench **4 passed**。涵盖大于旧容量的分批、模型姿态/尺寸传递、配置迁移保存、生命周期与无关粒子隔离。
- 生命周期 replay：**14,572 → 全脱落 → 14,572 再生 → 再次全脱落**。两代 **29,144** 个模型落叶共存，粒子池扩到 **32,768**；另一个水滴保留。
- replay 用同一注入风快照驱动生命周期和 GPU 叶姿态，并加速再生时钟。64 个实际 GPU 姿态探针验证 A 时间稳定、B 几何旋转、B→A 不重抽、两种模式脱落时完整模型姿态连续；再生期间切旋转开关不重置进度或落叶。
- 最终 replay 日志：`target/re-flora-logs/re-flora-20260926-233915.670-662414.log`，包含 `rotation_ab=true release_model_pose=true` 的 PASS 和 `phase=complete failures=0`，无 ERROR/panic/VUID。
- 独立连续数值诊断进行了 8/16/64px、0.25/1/2/4 倍显示尺寸的活切换，连续两次通过。历史 `ab` 参数现在仅表示模型分辨率/尺寸 sweep，绝不恢复 sprite。
- 修正了该诊断的覆盖参考输入：微小世界空间叶子的 CPU/GPU 重投影会得到不同末位，曾间歇出现“GPU addition absent in geometry plan”。诊断现在捕获共享投影函数的实际裁剪多边形，再由独立 CPU planner 检查覆盖和深度；原始 ray/depth、RGBA 不变和叶型多样性检查仍保留。没有放宽比较阈值或忽略错误。额外参考存储只在诊断模式分配。
- 隐藏截图 `target/leaf-model-tree.png` 确认树上有真实薄叶模型。未启动可见游戏；截图与 fixture **不是用户视觉/听感验收**，也不是自然风的完整验证。

## Release 实测与已知范围

RTX 3060 Ti，5120×2880，保存的 `glitch` 冠层近景，默认 14,572 叶位，旋转 A、分辨率 22。各运行 5 s，统计 frame ≥120；基线为 `754aee41`（cube），候选为本次共享模型。两次均隐藏、静音、开启 `--perf`，未强制 present mode。

| 指标 | 基线 | 共享模型 |
| --- | ---: | ---: |
| 整帧 CPU/wall p50 | 19.40 ms | 19.60 ms |
| 整帧 CPU/wall p95 | 20.27 ms | 20.52 ms |
| GPU frame p50 | 9.392 ms | 9.935 ms |
| 叶照明缓存 / 模型准备与 tiles | 0.026 ms | 0.422 ms |
| 叶 draw | 0.049 ms | 0.145 ms |

CPU/wall 各 143/142 样本；GPU 各仅 5 个 profiler 样本。日志为 `/tmp/leaf-model-baseline-perf.log`、`/tmp/leaf-model-final-perf.log`。这是**一个短场景的测量**，显示约 0.54 ms GPU 增量；不是普遍回退结论、大场景压力测试或性能验收。不以保叶/渲染数量限制掩盖代价；先由用户比较外观，再进行专门性能阶段。

阈值仍在 CPU 逐叶采样 **rest root**，而非随枝叶位移后的瞬时位置；已有风场为 32×32 水平场，无逐叶遮风。同步交接、CPU 遍历、空叶位仍参与风响应、每片可见模型的照明/tile 存储和粗阴影都是已知成本。叶位恢复与单调弱化是游戏抽象，不声称完整植物气动或生理模型。
