# Restorative Garden 世界切片交付（scope v2）

## 已实现

普通、无存档的启动现在直接进入可编辑的 Restorative Garden，不需要额外场景参数。
配方 v1、默认 seed `20260519`；偏心落脚空地、通向第二片空地的弯曲通道、宽缓且不等高
的地形、四棵错位框景树、粉/蓝两组主色花带与少量白色点缀。默认初始实景为
130,010 个普通草实例、180 个接近成熟的花实例、4 棵树。花的候选上限 2,304、数量上限
180；草仍受既有逐 chunk/物种 40,000 实例存储上限约束，不新增逐帧填充。

`--legacy-terrain` 明确保留旧程序地形和调试树；`--flat-terrain`、固定场景、各诊断/评测
owner 保留旧入口。载入存档永远优先，包括零花、零树、全空地形的合法存档。
显式相机/截图不被新开场落地逻辑替换。没有改写 `config/gui.toml`、相机库、用户存档，
没有新音频、材质、湿润状态或水足迹模拟。没有发布、推送或打包。

使用和验证命令见 [garden-opening.md](../garden-opening.md)，生态证据及其转用限制见
[garden-spatial-ecology.md](../research/garden-spatial-ecology.md)。生态文献仅提供聚集、软
habitat 权重等构图词汇；这里不是温带植物生态模型或“物种友好/排斥”结论。

## 架构与兼容

- `src/garden_layout.rs` 是配方、整数 hash、稳定物种 key、花带/空地/通道、树描述和缓坡
  的唯一静态布局 owner；不读取实时土壤水分，不在玩家编辑后重新生成。
- `PlainBuilder::chunk_init_with_terrain` 的 `Garden(seed)` 分支写普通 dirt/rock atlas 字节；
  CPU 上传后标记 solid workgroups，后续走原有可见地形与碰撞 publication。没有固定场景位。
- `garden_opening.rs` 在地形查询/碰撞就绪后，通过普通 Grow、树事务、花批次、Trim/Mow
  接口建立初始状态。成熟木材/叶冠径向包络以实际树生成结果有界拟合；不受用户调试树参数
  和年龄影响。存档仍使用原格式，个体初始年龄/大小差异烘入普通树描述，保证 reload 外形。
- CLI 只是输入；App 还通过 `LaunchOwners::permits_fresh_garden` 窄化实际 owner，不能仅凭
  `LoadingDirective::Garden` 判定。特别排除同样用 Garden loading 的水编辑、Hybrid、
  Environment、Canopy、Foliage、Denoiser、树/花 benchmark 等 fixture。
- 新的 `garden-*` 截图预设从 realized 地面求高度，不写相机库；普通显式用户预设保持原语义。
  没有新增需要保存的 Debug 控件。审查用环境变量明确是临时验收操作，不是持久偏好。

源码提交：`621b0944`（完整新开场）、`26ab3d97`（有界成熟树/稳定物种与 native 验收）、
`92681ed6`（Denoiser owner 保护）。文档提交另见分支 log。
**生成文件：没有 tracked diff**，所有 check/build 的生成输出均由工具产生，没有手工改生成文件。
没有触碰 wetness/audio 专属源码，也未修改已有 sprinkler 优化。

## 图像验收与迭代

证据根目录：`target/garden-delivery/world/implementation/`。
最终四张实际 native 图像都已打开检查，而非只检查文件存在：

| 图像 | 观察 |
| --- | --- |
| `arrival-final.png` | 低视角可读的粉色左花带、蓝白右花带；中间有空地和前进方向 |
| `corridor-final.png` | 弯道/第二空地连续，右侧树框景，没有树干封路 |
| `reverse-final.png` | 回望空地仍有明显空房间、花边和树影；前景没有密花堵路 |
| `overview-final.png` | 两片不等空地和连接路径、非对称树位置、延长花团与零星离群花均可辨 |

最终匹配条件：4096×2560 physical、FOV 60、相同保存设置、默认灯光/质量、无 HUD/perf
overlay，`--screenshot <preset> <path> --screenshot-delay 3`，隐藏静音 native Release。
arrival 使用新生成并保存的最终世界；其余三个直接载入同一个最终 snapshot。

有意义的比较保留在 `arrival-v1.png` 与 `arrival-v2.png`，二者也是 4096×2560 同视角同设置。
第一版只有树花、没有通过 Grow 建立草，而且带 HUD；打开图像后否定其作为完成结果。
第二版补齐普通草路径和 capture HUD 隐藏，形成实际草地/通道。最终再固定稳定物种 hash、
成熟树包络与保存年龄差异。`overview-v3.png` 是较早 audit 的不同 extent 调试图，**不作为
最终匹配图或性能证据**。视觉仍由玩家最终审批；当前候选有较亮的草色、灰色土壤和轻缓地势，
遵守没有为截图擅改用户颜色/灯光的边界。

## 实际验证结果

所有重命令组均使用 `flock -o /tmp/re-flora-garden-delivery-heavy.lock`；没有并行重负载、
没有嵌套 worker。原生运行均 Release、`--hidden --mute`；最终匹配图和所有性能运行在
有 DISPLAY 时只对子进程去掉 Wayland 环境，不改桌面配置、不强制 present mode。

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：main 1659 passed / 8 ignored；collision bench 4 passed；display authorization
  1 passed / 1 ignored；clipboard 的 3 个环境依赖测试按既有设置 ignored。
  后续新增的 launch-owner 专项测试也通过。日志 `full-validation.log`、`final-build.log`。
- 全部 49 个 Slang CPU tests 通过，包含原 sprinkler wetting/contact 合约。
- 纯布局测试：同 seed 同坐标/数量/树描述、不同 seed 变化、全拒绝有限终止、边界、花距、
  量化后空地排除、连通路坡度、成熟树包络/通道/边界通过。
- CLI/owner matrix 覆盖 fresh、seed、legacy、flat、载入（含 minimal 名称）、显式相机、
  House/Rooftop/Water/Hybrid/Environment/Canopy/Denoiser/Foliage fixture 和非法组合。
- native `audit-final.log`：真实 capsule 沿通道到达第二空地（212 步）；Dig 移除 129 个 soil
  voxels；在挖后的真实可种表面 Grow 成功，再用普通 removal/mow 路径移除该株。
  写出 `audit/dug-planted.rflterrain` 与 `audit/dug-mown.rflterrain`，只影响临时生成世界。
- `reload-final.log`、`reload-dug-planted.log`、`reload-dug-mown.log`：startup + 两轮 runtime
  replacement 全通过 terrain bytes、flora layout/growth/identity、tree descriptor/identity
  的 exact 检查，没有再生成/重复植物。分别保留 180 / 181 / 180 authored plants。
- `empty-normal.log`：全空合法存档成功恢复并 resave，flora=0、trees=0，无 garden stamp；
  正常退出。没有以“空所以应该补花园”为理由重新生成。
- `final-correctness.log`：无参数普通 fresh launch 确实建立新花园，并进入 walk；原始 eye
  `(0.72, 0.5682811, 1.46)` 经真实 capsule entry 小幅纠正为
  `(0.71774477, 0.5705364, 1.4577447)`，正常 shutdown failures=0，无 Vulkan/resource error。
- `preferences-before.sha256` 在全部运行后仍校验 GUI 配置和相机库均 OK。
- 最后恢复 default validation-layer `cargo build --release` 成功，且再次 hidden/muted correctness
  运行成功；不是留在 no_validation_layer 构建作为交付二进制。

### 已修正的验收迭代与未解决边界

早期纯测试发现浮点候选通过 mask、量化到 voxel 后却进入 mask：已把最终整数坐标重新检查。
早期 CLI 测试误写 `--rooftop` 而非真实 `--rooftop-poc`，修正后矩阵通过。
早期 minimal fixture 的 flora chunk 顺序不是 snapshot canonical 顺序，已按坐标排序，初始 exact
校验通过。这些失败日志保留，没有当作成功。

**仍未通过的更强边界**：`reload-minimal-empty-fixed.log` 中，既有 snapshot smoke 先向全空
地形添加树，再 runtime-load 全空存档；初始 exact 校验和零植物恢复通过，但 runtime water
publication 未在 20s 内 settle，验收 harness panic。没有修改外部 water owner，也不声称
该项通过。controller 已收到精确文件/日志请求，应检查空世界 water-dependent publication
readiness。普通/已挖/已种/已割花园的相同两轮 runtime smoke 都通过；正常全空 startup/resave
也通过。这是尚需集成判定的 runtime 空世界 edge case，而不是允许吞掉 crash 的审美让步。

## Release 性能（不是 Debug 推测）

GPU NVIDIA GeForce RTX 3060 Ti；4096×2560；默认保存质量/灯光，AUTO 选到 FIFO，3 images；
`--features no_validation_layer --hidden --mute --perf --camera-snapshot garden-arrival --auto-exit 12`。
Fresh garden 与 `--legacy-terrain` 交错各 3 次。首个正常 frame 后丢弃 3 秒，剩余约 9 秒每次
约 521–527 个 CPU frame 样本、18 个 GPU scope 样本。日志 `perf-*.log`，汇总
`perf-summary.json`。性能阶段之后已恢复普通 Release build。

| 指标（逐次中位数） | 新花园 1 / 2 / 3 | Legacy 1 / 2 / 3 |
| --- | --- | --- |
| 进程日志开始 → 首个 game frame（ms） | 2521 / 2469 / 2417 | 2002 / 2009 / 2007 |
| CPU frame total（ms，含 present 等待） | 16.90 / 16.90 / 16.92 | 16.90 / 16.89 / 16.89 |
| GPU `frame.render`（ms） | 8.916 / 8.334 / 8.349 | 6.109 / 6.239 / 6.200 |
| 草 raster `graphics.flora`（ms） | 1.935 / 1.347 / 1.349 | ~0.002 |
| flora lighting cache（ms） | 0.844 / 0.847 / 0.852 | ~0.001 |
| flower display（ms） | 0.328 / 0.327 / 0.328 | ~0.002 |
| 树 update CPU（μs） | 32.99 / 31.96 / 31.42 | 33.36 / 31.81 / 33.76 |
| 平滑 FPS | 58.5 / 58.6 / 58.5 | 58.4 / 58.6 / 58.3 |

新花园比本机旧空场景 startup 中位数约 +462ms；GPU 总渲染约 +2.1–2.8ms。这里是不同实际
初始世界的总成本对比，**不是同一几何上隔离的植被开关 benchmark**，不应相加各嵌套 scope
或把 FIFO-limited FPS 不变说成“免费”。首轮草 raster 较高的变动也如实保留，尚未解释为代码
改善。没有拿此结果冒充全场景 sprinkler 0/1/4/8/16 预算验收。

额外载入同一个 realized snapshot：startup 2168ms，GPU frame 8.325ms，草 raster 1.347ms、
flora cache 0.852ms、flowers display 0.329ms，CPU frame 中位数 16.74ms / p95 17.77ms。
所有 perf run 正常退出；没有创建昂贵的新水粒子群（普通开场 water particles=0）。

## 给集成者的 realized snapshot

保留 `target/garden-delivery/world/implementation/garden-final.rflterrain`（约 129MiB）及
`garden-final.sha256`。这是未经 audit Dig 的完整最终花园，snapshot format/schema 都没变。
controller 后续完整 sprinkler A/B 应把此文件和同设置复制到 integration 的同相对路径。

旧 baseline 和新 final 二进制都可以：

```text
--hidden --mute --terrain-load target/garden-delivery/world/implementation/garden-final.rflterrain
--camera-snapshot player-default
```

`player-default` 是两边都有的内建显式相机，避免旧 binary 不认识新 `garden-*` preset，也避免
新 fresh-generation 路径介入。两边同加载 realized world、同显式相机和设置，再按控制者 harness
放置相同 sprinkler。不要在 baseline 重新程序生成一份“差不多”的花园。精确水量、spray、
near/far/terraces、四种 hardware 的最终集成验收仍由 controller 持有。

需复制的核心证据：最终四张图、v1/v2 比较图、final snapshot+hash、audit 三个临时 snapshot、
全部本报告引用的日志、perf-summary.json、preferences hash。无须复制额外 build cache。


## Scope v3 补记：全空运行时重载已修复

上文 stage 2 的 20 秒失败是当时真实结果，继续保留。现已在水地形权威 owner 修复：
初始化依赖完整域观察与任务完成，不再错误地依赖至少存在一个实体 collider；正常 advance
每轮都推进完成，包括空与去重路径。旧性能 baseline ELF 也实际复现同一失败，准确引入提交仍未知。

`target/garden-delivery/empty_load/` 内新证据确认：原 tree→empty smoke 两次精确替换通过；
普通花园、挖掘种植、挖掘割草各两次替换通过；真实 water Running / terrain Ready /
world updates 解冻均断言通过；单独全空 startup/resave 保持零植物零树、无花园建立。
fmt/check、全量测试及默认 Release 构建通过；native 均 hidden/mute、无 ERROR 或 Vulkan
validation 错误。原全空 smoke 树印章的边界裁剪 WARN 仍如实保留。生成文件与用户设置无变化。
详见 [全空重载根因及验证](empty-world-reload.md)。这项功能验收不代表玩家视觉批准、声音批准，
也不替代 controller 的集成与性能验收。

## 最终集成（scope v4）

同一 realized garden 已用于66次原生、交错的喷头性能对照；当前声音、湿草和空世界修复全部在场。
控制器已在 main 重建并验证活跃声音读档、保存优先与普通新花园开局，任务工作树已正常清理。
详见[最终验收](garden-final-acceptance.md)及[总报告](garden-delivery.md)。视觉仍是候选，未替玩家批准。
