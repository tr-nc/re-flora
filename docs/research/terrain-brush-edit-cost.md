# 地形编辑与草刷：单次操作成本调研

研究快照：`288a907f`，2026-10-29。只调研，未改变输入、笔刷、Rust 或 shader 实现。

**第一轮历史调查。后续已补齐分段计时与 1,980 笔回放，结论以[闭环调研](terrain-brush-edit-cost-closure.md)为准：原先约 7 ms 的修改阶段主要是共享队列完成等待，而不是体素修改 kernel 自身。本文保留初始证据与假设，不再视为最新优先级。**

[可视化报告](../reports/terrain-brush-edit-cost.html) · [保留的原始日志摘录](../evidence/terrain-brush-edit-cost.log)

## 结论与边界

存在明确优化空间：局部编辑被放大为 chunk 级重建、整块 occupancy 往返和 CPU 状态重新对齐，而且多个 GPU 工作在主线程同步等待。应先缩小工作量并测清等待来源，保留 capsule 连线兜底；不要先增加补点、降低法线质量或直接提高编辑频率。

但「轨迹不够细」不能全部归因于编辑慢。挖地和普通草刷固定间隔均为 80 ms（理论上限 12.5 次/秒），GPU 编辑拾取同时只允许一个 pending 请求。`CursorMoved` 更新最新坐标；普通连续工具没有保存全部鼠标事件轨迹，`BrushStroke` 仅保存上次成功编辑端点。降低编辑成本能减少额外停顿，却不会自动恢复未采集的中间轨迹。[S1–S4]

**范围必须区分**：下文「草刷」特指无限材料下的普通草 capsule 绘制；有限种子模式的草通过 `plant_owned_flora → try_insert_finite_grass → try_append_grass` 单根追加，只写一个 8-byte 实例及其含水量，并更新根集合，不经过整块 occupancy 流程。模型花卉也有独立 authored 路径，不能用 `--authored-flora-bench` 代替普通草刷基准。[S5, S6]

## 当前成本链路

### 普通草 capsule 绘制

```text
80 ms 节流 + 单个异步 GPU pick
→ apply_surface_flora_regeneration_at
→ 对笔刷包围盒覆盖的每个 chunk：
    build_surface(chunk, false) → 等待完成
    clear occupancy（256³）
    existing grass instances → occupancy（按草数量）
    edit occupancy（256³，shader 内判断 capsule 距离）
    occupancy → instances（256³）
    等待完成 → 读取数量
    读取两类普通草的所有实例
    重建 root HashSet / hydration HashMap，按根身份恢复含水量并上传
    同步该 chunk 的 authored 植物实例
→ 帧内后续 grass soil cover 增量更新
```

每个 chunk 的三次全体积 dispatch 合计约 **5,033 万个 invocation**，不是说每个 invocation 都做同样多的工作；空值或 capsule 外部会提前返回。但即便极小笔刷也支付整块调度/扫描成本。`clear_occupancy` 的 R32UI 图像在 256³ 下逻辑大小为 64 MiB。[S7–S9]

`build_surface(false)` 不是 cheap cache lookup：清理共享 surface 图像、准备 sparse dispatch、重新生成该 chunk 的表面，并同步完成。已有 sparse solid-workgroup 和 packed smooth-normal 优化，因此不能描述成「表面每次都扫描完整 256³」；整块 occupancy 和 sparse surface 是不同路径。[S10]

**不能直接删掉 build_surface**：surface 是共享 scratch，并非每个 chunk 都有保鲜的独立表面缓存。需由 SurfaceBuilder 维护 `(chunk, terrain revision)` 有效性，或从权威 atlas/局部活跃表面重建所需区域；跨 chunk、地形/材质变动都要失效。整块图像永久缓存每块 64 MiB，有明显内存代价，不应作为默认方案。[S10]

CPU 读回也需准确描述：这里是等待 GPU 完成后复制 host-mapped buffer，而不是已证实的独立 PCIe DMA stall；随后按实例总数建立集合、映射和上传 hydration。这部分是 O(该 chunk 草数量)，在密草下值得专项测量，本次未单独计时。[S11]

### 地形挖/填

```text
compile capsule / bounds
→ modify_surface_with_readback：
    清统计和 1 MiB removal-candidate buffer
    在裁剪后的笔刷 AABB 内修改 atlas + 抽取最多 50 个粒子位置
    提交到 general queue，并等待该 job 完成
    读取统计、采样结果
→ VisibleTerrainPublication（运行时同步 run_to_completion）：
    各候选 chunk：surface → contree → scene texture
    PreserveFlora 路径中的 contree 当前为同步 build_and_alloc
    必要的 authored 植物清理
    terrain_changed 时：受影响有草 chunk 的整块 occupancy 存活检查
    collider / DDGI / tree dependency 等观察者通知
→ 连通性观察及帧内后续派生工作
```

真正 atlas 写入已经局部化；主要扩张发生在后续 chunk 重建和草存活阶段。一般多 chunk rebuild 有 Contree pipeline，但不能将此优化等同于 PreserveFlora 编辑路径也完全流水化。[S12–S15]

存在 no-op 工作候选：挖/填即便统计为零也可进入物理重建；`terrain_changed` 主要控制观察者而非阻止 Physical phase。可以区分「地形未变」和「仅植物发生变化」，避免无用 terrain rebuild，但不能直接整笔 return——挖地还可能有明确的植物清理语义。[S12, S14]

长 capsule 的 AABB 会扩大候选 chunk/体素范围。可进一步使用 capsule-vs-chunk 剔除、实际改动 bounds 和 dirty bricks，保留法线邻域 halo、跨 chunk 草埋藏规则及观察者依赖。[S7, S12–S15]

## Release 证据

### 环境与限制

- Apple M4 Pro，macOS / Vulkan 经 MoltenVK；源码和构建使用默认 feature。
- 屏幕 5120×2880，场景 1024×576；hidden 原生 surface/swapchain，mute，未指定 present mode。
- 先执行 `cargo build --release` 和 `cargo run --release -- --hidden --mute --auto-exit 0.5`，均成功；构建后 tracked source/generated/config 无变化。
- 下面都是现有 Release app replay；没有 mock、debug build 或新性能补丁。源码构建的 release 即使打开 debug 日志，仍是 Release 优化构建。
- 日志摘录保留源码完整 SHA、binary SHA256、OS 信息及原始测量。[E1]
- 每个 far/near 运行只有 3 笔，且三笔位置不同；不是可靠的 p95，不是优化 A/B，也不是鼠标输入到显示的延迟。near/far 的体素数量不同，不能据此做严格同工作量性能比较。

### 1. 实际挖地：约 9.3–12.3 ms/次

半径 0.055（约 14.08 voxel），三个位置在渲染开始后 5/8/11 秒执行，各自真正移除了 568–616 个 dirt voxel。`edit_ms` 包含地形高度查询、挖地/发布和诊断日志；无鼠标拾取等待，不含未来帧所有派生工作。

| 回放 | 第 1 笔：2 chunks | 第 2 笔：1 chunk | 第 3 笔：1 chunk |
|---|---:|---:|---:|
| far | 11.836 ms | 9.737 ms | 9.276 ms |
| near | 12.348 ms | 10.171 ms | 9.815 ms |
| far，额外分段日志 | 12.018 ms | 10.156 ms | 10.378 ms |

far 分段运行的 3 次累计 BENCH 平均：

| 分段 | 均值 | 解释 |
|---|---:|---|
| prepare buffers | 0.045847 ms | 含候选缓存清零，暂不是主要热点 |
| GPU modify and sample | 7.207583 ms | **CPU wall time：录制/提交 + 排队 + GPU 执行 + 等待** |
| stats readback | 0.000944 ms | 完成后的映射内存复制 |
| sample readback | 0.000514 ms | 同上 |
| plain builder total | 7.256819 ms | 修改阶段整体 |
| terrain removal total | 10.638471 ms | 不含外层 query / BENCH summary 等 |

其中一笔单 chunk 的重建分解为 surface 1.87 ms、contree 0.39 ms、scene texture 0.16 ms，总计 2.44 ms（日志四舍五入，各项不必严格相加）。本组编辑没有 `FLORA_EDIT_PASS_TIMING`，不能当作密草地形性能样本。[E1]

**首要缺口**：`terrain_edit_gpu_modify_and_sample` 还未拆 queue backlog / job wait / `chunk_modify` GPU timestamp / `chunk_modify_sample` GPU timestamp。这 7.2 ms 不能被写成「shader 花了 7.2 ms」，也不能被写成「CPU 复制统计花了 7 ms」。目前它是优先测清的时间块。

### 2. 极少量草仍支付整块 occupancy 固定成本

现有 `RE_FLORA_GRASS_SOIL_COVER_REVIEW=1` 实际执行两次微小草移除（一次每个 chunk），随后两次草埋藏/露出存活检查。GPU timestamp：

| 场景 | clear | instances → occupancy | edit | occupancy → instances | 合计 |
|---|---:|---:|---:|---:|---:|
| 移除，chunk 0 | .254 | .002 | .186 | .324 | .767 ms |
| 移除，chunk 1 | .253 | .002 | .186 | .297 | .738 ms |
| 存活检查 1 | .235 | .002 | .309 | .247 | .793 ms |
| 存活检查 2 | .246 | .002 | .314 | .261 | .824 ms |

这是稀疏验收 fixture 的 **GPU 子流程**，不是完整草添加笔刷耗时，不含前面的 surface rebuild、CPU hydration 整理及后续渲染；不能把约 0.8 ms 当成完整草刷成本或预计可消除的全部耗时。它证实全块扫描在极少草时仍有固定成本。[E1, S16]

### 3. 已增量化的泥土变绿不应优先重写

`RE_FLORA_GRASS_SOIL_COVER_BENCH=1` 使用两档根数量，各执行 12 次单根变化，计时只包围 `refresh_grass_soil_cover`，**不包含前面的 write_species_instances/hydration 对齐**。

| fixture 总根数 | refresh 中位数 | 最大值 | 无变化成本 |
|---|---:|---:|---:|
| 1,024 | .198 ms | .623 ms | .000 ms（日志精度） |
| 65,536 | .246 ms | .269 ms | .001 ms |

该流程已消费 changed roots，只上传局部 patch，没有全 atlas 读回。两个 fixture 的结果支持暂时降低此处优先级，不能泛化成所有草相关 CPU 工作都已增量化。[S17, E1]

## 推荐次序（提案，尚未实现或证明收益）

1. **先补一个准确的单笔成本基线**：为普通草 add/remove 和地形 dig/place 设置相同种子/地形/草密度的短点、长段、跨 chunk 回放；分离 GPU dispatch、排队/等待、CPU hydration、发布及接下来数帧。当前 7 ms 时间块必须拆清，再决定优先改 kernel 还是提交编排。
2. **优先缩小草编辑的数据访问范围**：remove/trim/terrain-survival 遍历现有根而不是 256³；add 遍历 capsule 附近的可种植表面/活跃砖并查重。先可局部化 edit dispatch，但要明确它仅消除一个 pass，clear/rebuild/hydration 仍是全量。最终以稳定根身份和增量结构变化维护实例与含水量，避免重复重排所有草。须保留长/短草排他、增长/出生时间、种植法线、竞争场和土壤覆盖语义。
3. **让地形派生工作依赖真实变化范围**：no-op 地形不重建；采用 actual dirty bounds/chunks，后续才评估砖级 surface/contree 更新。5×5×5 法线估计不能为了速度降质；变更邻域、材质可种性、跨 chunk 埋藏判定都要算入依赖。
4. **测到等待主导时再拆同步流水线**：分离每笔 scratch、批次提交、GPU indirect count/延迟 readback，减少 CPU↔GPU 同步边界。不能仅去掉 `wait_complete` 或丢给后台线程：GPU 资源生命周期、按次库存统计、可见 revision、拾取有效性、碰撞/光照一致性都受现有同步契约保护。草/地形各自的 owner 应承担此状态机。
5. **最后才考虑采样频率**：保留原有 capsule 兜底；若以后要求保留细轨迹，应另立输入意图采样/编辑消费解耦任务，而不是把现有 capsule 切成更多昂贵完整事务。本次不实施这个任务。

不优先做：继续微调 capsule 距离数学、先将 80 ms 改成每帧、重写已经增量化的 soil cover、把 1 MiB 候选清零当作主要热点、照搬历史 RTX surface benchmark 作为本机草刷结论。

## 下一轮验证契约

- 场景：空地/稀草/密草，单 chunk/跨边界，小/大半径，点/短段/长对角段，已有草重刷 no-op，挖/填/移除/trim；有限种子和 authored 花卉单独验收。
- 指标：每笔候选/实际变化体素、根数量、触及 chunks/bricks、读取/上传字节、submit/wait 数量；CPU 和 GPU 分别报 median/p95；记录 input request → pick completion → publish → first presented frame，避免用 throughput 冒充交互延迟。
- 保持输出不变量：未受影响根的位置、species、growth、spawn time、hydration 不变；重复草刷不重复种根；有限库存恰好扣费；跨边界/埋藏与挖出一致；surface/contree/collider/lighting 接受相同已发布 revision；miss/切换工具不桥接路径。
- 基线与候选同工作量、同配置、固定回放，Release 下 A,B,B,A；若需要用户 A/B 比较按项目要求提供 Debug 运行时复选框。不能用降低视觉质量换取等价优化名义。
- 本次没有完整普通草添加、密草 CPU 对齐、长 capsule 或真实高频鼠标端到端 benchmark；收益排序仍需补这些证据。

## 复现

在源码快照 `288a907f`、当前机器运行。普通开发 checkout 运行后检查 `config/gui.toml` 等持久化文件的差异；本次没有 tracked config 改动。完整本地日志在 `target/perf/edit-cost-research/`（未提交），关键摘录提交在 E1。

```bash
cargo build --release
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log

RE_FLORA_TREE_EDIT_DIAGNOSTIC=far target/release/re-flora \
  --hidden --mute --perf --water-edit-soak --auto-exit 12
# 相同命令，DIAGNOSTIC=near 再运行一次。

RUST_LOG='info,re_flora::builder::surface=debug,re_flora::util=debug,re_flora::app::physical_visible_terrain=debug' \
RE_FLORA_TREE_EDIT_DIAGNOSTIC=far target/release/re-flora \
  --hidden --mute --perf --water-edit-soak --auto-exit 12

RUST_LOG='info,re_flora::builder::surface=debug' \
RE_FLORA_GRASS_SOIL_COVER_REVIEW=1 target/release/re-flora \
  --hidden --mute --perf --auto-exit 0.5

RE_FLORA_GRASS_SOIL_COVER_BENCH=1 target/release/re-flora \
  --hidden --mute --perf --auto-exit 0.5
```

所有 5 个研究运行均退出 0，具有成功退出标记，未发现 `ERROR`、`VUID-`、panic。没有 Rust/shader 变更，因此未为文档重复执行 cargo check/test；这不是优化实现的正确性或性能验收。HTML 已在浏览器检查 1440×1000 与 390×844 视口及截图，页面无横向溢出；窄屏表格保留内部横向滚动。35 个相对文件链接均已校验。

## 第一方来源索引

以下行号对应调研快照，函数名用于后续定位；代码是行为证据，历史文档不是当前性能证据。

- **S1** [core/mod.rs](../../src/app/core/mod.rs):960、1977–1979、2178–2181：80 ms、鼠标坐标更新。
- **S2** [species.rs](../../src/flora/species.rs):46–47：普通草 80 ms。
- **S3** [terrain_picking.rs](../../src/app/core/terrain_picking.rs):44–60、117–169；[session.rs](../../src/app/core/terrain_picking/session.rs):62–85：节流、异步 pick、一个 pending。
- **S4** [brush_stroke.rs](../../src/app/brush_stroke.rs):10–39：成功端点间 capsule，不保留事件路径。
- **S5** [input.rs](../../src/app/core/input.rs):1227–1273；[garden_economy/world.rs](../../src/app/core/garden_economy/world.rs):112–172；[surface/resources.rs](../../src/builder/surface/resources.rs):267–294：有限种子分支和增量 append。
- **S6** [vegetation.rs](../../src/app/core/vegetation.rs):3265–3330：普通草与 authored 分支。
- **S7** [world_ops.rs](../../src/app/world_ops.rs):18–49、134–162：chunk 候选、surface rebuild、普通草编辑。
- **S8** [surface/mod.rs](../../src/builder/surface/mod.rs):1581–1810：occupancy 编辑、同步等待、hydration、authored 同步。
- **S9** [clear_occupancy.slang](../../shader/slang/clear_occupancy.slang)、[edit_occupancy_capsule.slang](../../shader/slang/edit_occupancy_capsule.slang)、[occupancy_to_flora_instances.slang](../../shader/slang/occupancy_to_flora_instances.slang)：dispatch 内部行为。
- **S10** [surface/mod.rs](../../src/builder/surface/mod.rs):639–830；[surface/resources.rs](../../src/builder/surface/resources.rs):482、518–542：真实表面重建和共享 scratch。
- **S11** [surface/resources.rs](../../src/builder/surface/resources.rs):342–407；[buffer_impl.rs](../../crates/re-flora-vkn/src/memory/buffer/buffer_impl.rs):423–446：根集合、含水量对齐、host-mapped readback。
- **S12** [vegetation.rs](../../src/app/core/vegetation.rs):2964–3105：修改、统计判断、植物清理和发布。
- **S13** [plain/mod.rs](../../src/builder/plain/mod.rs):1811–1926、2313–2371；[chunk_modify.slang](../../shader/slang/chunk_modify.slang)；[chunk_modify_sample.slang](../../shader/slang/chunk_modify_sample.slang)；[command_buffer.rs](../../crates/re-flora-vkn/src/command/command_buffer.rs):301–318：局部 atlas dispatch、候选清零、同步 helper。
- **S14** [physical_visible_terrain.rs](../../src/app/physical_visible_terrain.rs):205–295；[visible_terrain.rs](../../src/app/core/visible_terrain.rs):408–428、446–459、507–554：重建和观察者。
- **S15** [surface/mod.rs](../../src/builder/surface/mod.rs):1460–1493：仅候选 chunks 限域、普通草存活仍走整块 occupancy。
- **S16** [soil_cover_review.rs](../../src/builder/surface/soil_cover_review.rs):158–219：微小移除与埋藏/露出验收调用。
- **S17** [soil_cover_review.rs](../../src/builder/surface/soil_cover_review.rs):6–63；[soil_cover.rs](../../src/builder/surface/soil_cover.rs):179–220：计时范围与 delta patch。
- **S18** [water/simulation.rs](../../src/app/core/water/simulation.rs):102–239：已有诊断回放及外层计时。
- **E1** [本次 Release 原始日志摘录](../evidence/terrain-brush-edit-cost.log)。
