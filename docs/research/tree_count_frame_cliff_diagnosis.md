# 多树逐步变慢与帧时间突变：Release 诊断

## 结论与边界

这次完成诊断和可重复的无人值守复现，**没有实施性能修复、扩大容量、改默认值或降低画面质量**。

确认了两个不同的问题：

1. 正常树表面已经同时参与 CPU 变形／碰撞和 GPU 变形／渲染／查询，每棵树都会增加每帧工作。
   当前 CPU 每帧重算完整视觉表面，并重新构造传给碰撞系统的顶点和三角形数组，存在明确的优化入口。
2. 存在**树表面资源硬上限 + 同一失败输入每帧全量重试**的严重问题。失败后的主线程每帧重新读取
   atlas、提取整片树表面、绑定蒙皮、准备上传，再次失败。不是普通的“树多一点，GPU 就慢一点”。

本机稳定复现了第六棵附近的持续突降，也复现了其他复杂度下第三／第四／第八棵的突降。
触发点由实际几何数量决定，不是固定的第六棵树。

**独立捕获的错误是 `dynamic tree scene capacity exceeded` 和 `tree solid cell capacity exceeded`。
没有独立捕获用户那条 `unbound GPU tree vertex`。** 后者同样会进入这个失败重试路径，但为什么某个
顶点未绑定仍未确认；不能把它说成已证明由容量上限造成。

## 场景与反馈命令

工作树 `/home/terence/.herdr/worktrees/re-flora/new-tree`，原始 HEAD `b5fa69b0`。
读取 `AGENTS.md`、`CONTEXT.md`、树 publication 文档；仓库没有 `docs/adr/`。
诊断独立运行，没有子代理、合并、推送或关闭用户应用。

- i5-12600KF、RTX 3060 Ti 8 GB、驱动 580.159.04。
- Release、原生 Vulkan hidden 窗口、mute 保留音频处理；5120×2880 屏幕、1024×576 scene。
- 自动选择 FIFO，不强制 present mode；不关闭 DDGI、阴影、植被、水地形跟随或果实物理。
- 默认地形／默认玩家相机，无用户存档，未操作真实鼠标、Grow 背包或预览 UI。
  复现使用 Grow 最终调用的 `App::add_tree` → `GardenTrees` publication 入口；树种描述走同一 species compiler。
- 启动调试树为一棵 Garden Tree，seed 122；逐棵增加，默认新树 seeds 123、124……。
  每个数量保持 2 秒，前 0.5 秒不采帧时间，避免把种树当帧的一次性编译当作持续卡顿。
- 新树 XZ 相对根 `(1,1)` 的固定偏移依次为 `(0.48,0)`、`(0,0.48)`、`(-0.48,0)`、
  `(0,-0.48)`、`(0.36,0.36)`、`(-0.36,0.36)`、`(0.36,-0.36)`、`(-0.36,-0.36)`。
  高度通过真实 CPU 地形查询取得。没有冻结全世界动画／果实时钟；几何 counts 在重复运行一致。
- 原 GUI SHA256 `b346260347e1ef295b772e04963e364251be0564f4bf3faff3ef32a4be5dd8d7`；
  脚本备份并恢复 GUI／camera 原始字节，size／wind 消融不改变持久默认值。

已实际执行，能在**帧时间症状**上变红的命令：

```bash
cargo build --release
uv run --no-project scripts/diagnose_tree_scaling.py \
  target/tree-scaling-final-01 --trees 6 --tree-size 16
```

输出（最终移除分段探针后的 binary）：

```text
run=1 trees=1 median_ms=17.016 errors=0 cliff=False
run=1 trees=2 median_ms=17.093 errors=0 cliff=False
run=1 trees=3 median_ms=16.720 errors=0 cliff=False
run=1 trees=4 median_ms=21.103 errors=0 cliff=False
run=1 trees=5 median_ms=25.365 errors=0 cliff=False
run=1 trees=6 median_ms=111.226 errors=18 cliff=True
run=2 trees=1 median_ms=16.986 errors=0 cliff=False
run=2 trees=2 median_ms=16.928 errors=0 cliff=False
run=2 trees=3 median_ms=16.975 errors=0 cliff=False
run=2 trees=4 median_ms=20.896 errors=0 cliff=False
run=2 trees=5 median_ms=25.106 errors=0 cliff=False
run=2 trees=6 median_ms=110.239 errors=18 cliff=True
cliff_captured=True contended=False
exit=1
```

退出 1 表示捕获症状（相邻树数的帧时间中位数至少三倍且至少 80 ms），0 未捕获，2 证据无效。
80 ms／三倍是本次症状诊断判据，**不是新的 Release 性能预算**。
输出目录必须不存在，重跑请换目录。不要把 app 正常退出或 hidden 0.5 秒 smoke 当成修复通过。
最终 replay 在 `src/app/core/tree_bench/scale_diagnostic.rs` 明确隔离为 opt-in 诊断；只有设置
`RE_FLORA_TREE_SCALE_DIAGNOSTIC` 并使用 `--tree-bench` 才启用，普通游戏／原替换 benchmark 不变。

## 规模曲线

分段诊断 binary 的两次串行运行（单位 ms／帧，中位数；首棵也计入总树数）：

| workload | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Garden size 16，run 1 | 17.09 | 17.10 | 16.53 | 19.69 | 24.75 | **107.45** | 179.95 | — |
| Garden size 16，run 2 | 17.12 | 16.99 | 16.68 | 19.41 | 24.63 | **108.30** | 154.79 | — |
| Garden size 14，run 1 | 16.90 | 17.15 | 16.90 | 17.42 | 21.95 | 23.67 | 28.89 | **118.55** |
| Garden size 14，run 2 | 16.99 | 17.02 | 16.92 | 17.01 | 21.64 | 23.39 | 28.75 | **119.55** |
| Garden saved size 20，baseline run 1 | 17.07 | 16.90 | **91.59** | 113.26 | 147.18 | 169.61 | 236.72 | 257.59 |
| Garden saved size 20，baseline run 2 | 16.74 | 21.22 | **88.70** | 110.55 | 211.70 | 238.56 | 281.96 | 321.42 |

size 16 / 14 只是单变量诊断输入，不是建议把树缩小作为修复。
最小 saved-size 场景保留三棵、运行两次，第三棵分别 99.89 / 95.74 ms，前两棵没有 raster 错误。

另外，启动 Garden Tree + 逐棵添加 axis Conifer（size 20）在八棵以内没有 raster 错误：
八棵为 23.24 / 21.76 ms。换 added seeds 起点 1、37、65535（size 20）均在第四棵触发容量突变；
size 14、seed 起点 923 在第八棵触发。均未捕获 unbound 错误。
这些对照支持“几何规模阈值而非固定树数”，但不证明所有树种／种子／重叠布局都安全。

FIFO 使低负载阶段约 17 ms；“FPS 没降”不代表新增树没有成本。
日志帧时间是 app wall time，不是 GPU 忙碌时间；`gpu_present_ms` 也包含 CPU 录制／发布和等待。
GPU 结论使用 Vulkan `[GPU_FRAME_SCOPE]` 时间戳，不把它与 CPU 段直接相加当作总帧时间。

## 为什么会突然崩塌

`src/tracer/tree_scene.rs` 固定最多 **131072 vertices / 131072 triangles**。
`RasterTreeMesh::finish` 对每个 surface cell 生成 8 个顶点；整片树共享这组资源，不是每棵树一份上限。
实际 size 16 对照：

- 五棵：121472 vertices、83132 triangles，上传成功。
- 六棵：**146904 vertices**、101120 triangles，顶点超过 131072；solid cells 35502，没有超 solid 上限。
- saved size 20 三棵：136128 vertices、91284 triangles，同样超顶点上限。

因此不是显存耗尽或 GPU 只能画五棵。资源固定容量在 CPU 准备阶段先拒绝了几何。
另一个现成五树 canopy diagnostic 则首先撞到 solid cell 容量，说明不止一个固定预算。

失败循环：

```text
每帧 App::sync_static_raster_trees
  TreeSurfaceCache::is_current(...) == false
  wait_idle → 读取每棵树 atlas → append_region → finish → bind_tree
  → Tracer::publish_static_raster_trees → RasterTreeGeometry::upload
      → gpu_skin → TreeScene::new → capacity error
  ? 提前返回，没有执行 source.compiled(...)
App 记录 ERROR、enabled=false、失效阴影历史
下一帧同一 terrain/canonical revision 再走全部流程
```

临时分段探针显示，size 16 六棵在 revision=6 / canonical_revision=6 不变时仍每帧编译：

| 重试段，中位数 ms | run 1 | run 2 |
|---|---:|---:|
| wait_idle（单独计时） | 0.27 | 0.19 |
| atlas readback | 5.25 | 5.00 |
| append/extraction | 28.06 | 29.22 |
| binding（含 attachments） | 30.72 | 30.52 |
| publication/upload 直到报错 | 13.43 | 13.38 |
| compile total（不含前面的 wait_idle） | **80.83** | **82.10** |
| 整帧 | **107.45** | **108.30** |
| GPU frame.render | **8.45** | **8.53** |

CPU 全量重试解释主要突增；upload 是 CPU／驱动／传输混合 wall time，不称为纯 GPU 执行时间。
关闭 raster 后正常 CPU surface 变形／碰撞反而基本为零，不能把约 100 ms 的崩塌归因于这些正常更新。

`gpu_skin` 遇到未绑定顶点也会在同一 publication 中返回错误，且缓存同样未执行 `compiled`。
这解释**如果**用户那个错误持续发生，为什么也会持续卡顿；其顶点绑定缺失的根因仍未知。

## 正常每棵树的开销与优化空间

size 16，在尚未失败的一棵／五棵阶段，Release app 实测：

| 正常更新，中位数 ms | 1 棵 | 5 棵 |
|---|---:|---:|
| CPU surface skin | 0.77–0.80 | 4.62–4.81 |
| CPU deforming collision publication | 0.32 | 1.57 |
| TREE_UPDATE total CPU | 1.22–1.30 | 6.54–6.65 |
| GPU frame.render | 5.48–5.68 | 8.59–8.68 |
| GPU tree surface update/refit | 0.08–0.09 | 0.20 |
| GPU raster tree lighting | 0.25–0.27 | 0.75 |
| GPU DDGI probe trace | 0.87–0.90 | 1.59–1.62 |
| GPU leaf color draw | 0.04 | 0.24 |

当前骨架求解／渲染蒙皮已经有 GPU 路径，但 CPU 为真实碰撞与命中又变形全部视觉顶点。
`RasterTreeMesh::posed_surface` 逐顶点建立 skin transform；`TerrainPhysics::publish_tree_surface`
每帧重新生成 positions Vec 和不变的 triangle Vec，再交给 `set_deforming_surface`。
因此“已有 GPU 树动画”不等于主线程没有树几何成本。

另外，正常增加一棵也同步重编译所有树表面；五棵成功编译约 132–135 ms，主要 publication/upload
约 77 ms。这是**种树当帧的一次性停顿**，与失败后每帧 80 ms 的持续卡顿要分开处理。

建议分阶段、保持外观与碰撞正确性：

1. **先解决 publication 正确性与资源容量**：资源按实际需求增长／明确分区预算，所有必要容量和
   skin 完整性在 canonical 提交前验证；失败时保持完整一致的旧 publication 或事务回滚。
   完整准备成功后才切换 resident resources。不能只调大一个常量——solid/attachments/vertices/
   primitives/BVH/refit/skin 都有相关预算与 buffers。
2. **再消除失败重试风暴**：失败输入必须有明确状态，只在相关输入或资源可用性改变时重试。
   不能为了性能把失败缓存标成 current，不能仅隐藏 ERROR、停止重试而让新树缺失或碰撞错配。
   unbound 的几何归属／绑定问题需要另外定位与回归覆盖，不能用随便补一个 bone 糊过去。
3. **优化正常 CPU surface/collision 管线**：保留不变拓扑，避免每帧重构 triangles；在相同 rest corner
   和相同 binding 下复用变形结果／压缩 CPU collision 表面；按树／空间分区更新而不是反复发布整片视觉 mesh。
   需要保持实际 posed collision、GPU shading、查询与 rest-coordinate 编辑一致。
4. **优化种树的一次性成本**：按树／受影响空间区维护派生表面并复用未变的其他树 publication；
   如做异步准备，仍只发布完整一致结果，不分帧交付半棵树。

CPU 段有毫秒级、随规模增长的明确优化入口；未实施候选，**不能承诺具体 FPS 增益或支持树数**。
这些测量不支持优先重写叶片显示或关闭阴影来解决崩塌。

## 假设与消融

复现后向用户列出四个可证伪方向：容量／失败重试、skin 绑定缺失、正常物理成本、GPU 叶片／阴影成本。

相同 saved-size 三棵，只变一个开关：

| 诊断输入 | 第三棵 median ms | 重复 raster 错误 |
|---|---:|---:|
| 完整渲染，分段探针 | 90.28 | 22 |
| raster wind off | 89.20 | 23 |
| `--no-shadows` | 90.38 | 22 |
| `--no-flora` | 87.72 | 23 |

均未消除容量失败／帧时间突变。不是产品候选，也没有把这些开关保存成新默认值。
`[COLLISION][FRUIT] physics hitch dropped` 在独立日志中也出现；代码在长 frame delta 触发物理
catch-up 丢弃时记录它。此次与同步编译／失败重试相伴，不能仅凭这条 warning 断言果实物理是首因。

## 证据、清理与验证

原始日志／JSON 留在本工作树 `target/`，没有提交大日志：

- `tree-scaling-baseline-01` / `tree-scaling-minimal-01`：完整八棵／最小三棵，各两次。
- `tree-scaling-probe-01`：分段定位。
- `tree-scaling-size16-01` / `tree-scaling-size14-01` / `tree-scaling-conifer-01`：规模／种类对照，各两次。
- `tree-scaling-wind-off-01` / `tree-scaling-no-shadows-01` / `tree-scaling-no-flora-01`：单开关消融。
- `tree-scaling-size14-seed923-01` / `tree-scaling-seed{1,37,65535}-01`：额外种子检查。
- `tree-scaling-final-01`：移除生产路径分段探针后，两次原始症状仍然为红。

每组报告包含 binary SHA256、实际命令、原始逐帧样本、GUI SHA、启动前其他游戏进程检查、GPU 信息及
tracked source diff。初始探路时另一工作树有可见游戏；没有关闭它，也未把该并发 2 秒 run 当正式计时。
上述正式组启动前均没有其他 `re-flora`。桌面／驱动仍运行，不是完全独占 GPU。

分段 binary SHA256 `2e891ee93c7ebcffddf225b3e95236cfedae866e7aaf14ad46f3276c375ec557`。
最终 binary SHA256 `03c354515c931ad4737f89836caca45d8334b5044b8b7a774f969027527f6314`。
分段探针曾位于 `vegetation.rs`、`raster_tree.rs`、`core/mod.rs`；已全部移除，这三个文件相对原分支无 diff。
只保留明确标记的独立诊断 module、启动 hook、Python replay/parser 测试和本报告。
没有修改 shader／生成文件／GUI 默认值；没有性能修复，所以原症状仍可复现。

验证结果：

- `cargo fmt --check`、`cargo check`、`cargo build --release`：通过，Release binary 已就绪。
- `cargo test --bin re-flora tracer::raster_tree::tests`：10 项通过。
- `cargo test --bin re-flora tracer::tree_`：14 项通过。
- `cargo test --bin re-flora tree_gen::`：30 项通过。
- parser unittest：6 项通过；`uvx ruff check`、`git diff --check`：通过。
- 全量 `cargo test` 尝试 120 秒，看到 5 个未改动 GUI／配置用例失败，且藤蔓长耗时用例导致超时；
  **没有宣称全量通过，也没有为本任务修这些无关项**：
  `saved_control_adapter_uses_type_specific_dimensions_and_one_source_spec`、
  `stone_search_checkbox_edit_save_and_reload_are_the_real_saved_fields`、
  `glass_unrefracted_raster_fallback_defaults_off`、
  `main_saves_gain_lifecycle_controls_without_changing_authored_display`、
  `older_saves_get_disabled_supersampling_and_saved_choices_survive_reload`。
  未在另一个 baseline checkout 重跑，故不将其宣称为已独立确认的 baseline 失败。
- 全局 pyright 不可用，未声称已做类型检查。
- 最新 `cargo run --release -- --hidden --mute --auto-exit 0.5` 成功退出，日志助手检查无 ERROR/panic/VUID。
  有一次首帧 fruit hitch warning，非多树复现本身；不隐瞒、不把 smoke 当成性能证据。

诊断工具步骤提交为 `b53c76cb`、`5ad01552`；只在本分支本地提交。
后续修复验收必须让原始完整数量 replay 不再错误／持续突变，再单独比较健康多树 CPU/GPU 规模曲线，
加容量边界、树替换／年龄／移除、交叠、多种子、skin 完整性、碰撞／GPU／query 一致性检查。
