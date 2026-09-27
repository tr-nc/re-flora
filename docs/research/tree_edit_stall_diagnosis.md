# 树附近编辑卡顿：Release app 诊断（未实施修复）

> 本文保留原分析阶段结论。用户随后授权的实现与验证见
> [树编辑蒙皮绑定优化](tree_edit_stall_optimization.md)。

## 结论与边界

**已稳定复现树旁挖土引起约 0.87 秒停顿，主因是同步整树表面重编译，尤其 CPU 蒙皮绑定。置信度高。**

编辑的 AABB 进入树表面 atlas 读取 AABB 后，`TreeSurfaceCache` 被置 dirty；当帧
`sync_static_raster_trees` 重新读取并编译所有树。即使本次没有移除木体素、最终树 mesh fingerprint
完全不变，仍重新执行逐顶点/逐角点的树枝归属与蒙皮绑定。本例 `bind_tree` 为 717–738 ms。

这是本场景中确认的机制，不是对用户所有工具/存档的泛化判断。没有修复、改默认值、改 GUI 配置、
改生成文件、合并或推送。诊断 Rust 探针和脚本明确隔离在 `RE_FLORA_TREE_EDIT_DIAGNOSTIC` opt-in 路径。

## 来源与场景

- 工作树 `/home/terence/code/re-flora-agent-tree-edit-stall`，分支 `agent/tree-edit-stall`。
- base `7310bd8265cc12223d90ecdb890e239d2f753fa1`；不涉及另一 Pretty Leaves Worker 的后续工作。
- 使用指定 `gpt-6-astra` / `medium`，没有创建子 Worker。
- 阅读 `AGENTS.md`、`CONTEXT.md`、diagnosing-bugs skill；仓库没有 `docs/adr/`。
  相关决策见 `docs/terrain_visual_rebuild_pipeline.md`、`docs/tree_publication_architecture.md`。
- i5-12600KF、RTX 3060 Ti、驱动 580.159.04；hidden 原生 Vulkan 窗口 5120×2880，自动选择 FIFO。
  未强制 present mode，未关闭 DDGI、阴影、树风或植被。
- 正常默认启动地形，无 terrain-load/光照测试 fixture；配置为 base 的 `config/gui.toml`。
  没有另设全局场景随机 seed；固定代码/配置并核验生成几何及每次移除计数相同。
  树 branching seed **122**、size **20**、iterations **7**、age **1**，1 棵默认树，1941 个 trunk cones。
  `raster_tree_static=true`、`raster_tree_wind=true`；启动水粒子为 0，保留水地形跟随队列。
- 树初始表面：5576 cells、29636 triangles；fingerprint `ae11f950c7b738e1`。
- GUI 文件 SHA256：`c45e9ca4a8bb6fa10b88818721b2ba455532d28c1fddbb47659db5c742367256`。

## 已实际运行的最小无人值守复现

```bash
cd /home/terence/code/re-flora-agent-tree-edit-stall
cargo build --release
python3 scripts/diagnose_tree_edit_stall.py target/tree-stall-final
```

目录必须为空；重跑换新目录。实际输出：

```text
stall_captured: true
near 1 edit frame ms 890.85 tree compiles 1
far 1 edit frame ms 21.11 tree compiles 0
far 2 edit frame ms 21.35 tree compiles 0
near 2 edit frame ms 865.13 tree compiles 1
near 3 edit frame ms 888.35 tree compiles 1
far 3 edit frame ms 22.0 tree compiles 0
exit=1
```

退出码 **1 是捕获性能症状（红）**，不是应用崩溃；0 未捕获，2 证据无效。
判据为每个 near 编辑帧 ≥100 ms、每个 far 编辑帧 <100 ms。100 ms 只是本症状的诊断阈值，
不是项目新性能验收标准。脚本验证一个编辑、完整帧日志、成功退出及无 ERROR/panic/VUID。
修复后应同时看绝对帧时间与 near/far 差值，不能仅凭退出 0 宣告验收。

脚本内部每个进程执行：

```bash
env -u WAYLAND_DISPLAY RE_FLORA_TREE_EDIT_DIAGNOSTIC=near \
  target/release/re-flora --hidden --mute --perf --water-edit-soak --auto-exit 7
# 同样执行 far；交错 near/far、far/near、near/far，共 3 对，每次全新启动。
```

- render-start 后 **5 秒**才编辑，7 秒退出；另报编辑前最后 120 帧中位数，避免混入启动整树编译。
  不把 hidden 0.5 秒 smoke 当复现证据。
- 复用 shovel 的 `apply_surface_terrain_removal` 球形表面移除入口：radius **0.055 world units = 14.08 voxels**，
  单笔、无材料筛选、max_write_count=65536（实际仅 583/616，不触限）。不驱动鼠标、背包和 harvest 粒子。
- near：XZ **(1.125,1.0)**，距树根 XZ (1,1) **0.125 = 32 voxels**。
  从上向下查询地表，实际中心 **(1.125,0.434,1.0)**（日志取三位小数）。
- far：XZ **(1.625,1.0)**，距树根 **0.625 = 160 voxels**。
  实际中心 **(1.625,0.496,1.0)**。两笔同类型、半径、XZ chunk 边界相位、均重建 **2 chunks**。
- 地形高度/局部形状不完全一致，near 移除 **583**、far **616** 个 type=2 土体素；木 type=5 均为 **0**。
  不把两地视为逐体素相同场景；但一般编辑工作量接近，而停顿差约 40 倍。
- 树表面读取范围 voxel AABB 为 **(222,94,193)..(304,213,323)**，含法线邻域 halo。
  near brush AABB 与此相交，far 不相交。
- 初始三笔探索 replay 运行 `--auto-exit 14`：5/8/11 秒三次移除，Z 每笔加 0.08；
  `trunk` 模式 X=1。第二/三笔可能从上命中枝条，因此最终最小对照只保留第一笔。

## 实测数据

主证据 `target/tree-stall-final/report.json` 及 `near-{1,2,3}.log`、`far-{1,2,3}.log`。
测量二进制 SHA256 `3d208b171b00ff6e0fc5ab2d8358cbc348fc429ea784b9b6d075ad2f204d30f7`。
report 记录当时 HEAD `3e60a634`，其 `source.diff` 保存当时未提交的最后一级诊断细化；
这些 Rust 细化随后提交为 `d9a95ee6`。没有性能修复夹杂在测量版本中。

| 指标（ms） | near 1 | near 2 | near 3 | far 1 | far 2 | far 3 |
|---|---:|---:|---:|---:|---:|---:|
| 编辑前 120 帧中位数 | 17.25 | 17.08 | 17.07 | 17.10 | 17.115 | 17.115 |
| 编辑当帧 wall time | 890.85 | 865.13 | 888.35 | 21.11 | 21.35 | 22.00 |
| 编辑 API 自身 | 14.814 | 12.847 | 12.929 | 13.055 | 12.989 | 13.661 |
| visible terrain publication | 5.51 | 4.69 | 4.47 | 4.64 | 5.01 | 5.26 |
| 后续同步树 compile | 866.473 | 843.488 | 866.727 | 无 | 无 | 无 |
| 其中 `mesh.bind_tree` | 738.324 | 717.255 | 738.365 | 无 | 无 | 无 |
| 重编译前 device idle 等待 | 0.096 | 0.089 | 0.104 | 无 | 无 | 无 |

near compile 进一步分解：atlas readback **1.215–1.304 ms**；`append_region` **89.281–92.229 ms**；
`finish` **1.503–1.845 ms**；绑定（含叶/果 attachment map）**717.892–739.008 ms**；
upload **31.910–32.439 ms**。蒙皮绑定约占整树 compile **85%**。
三个 near 运行编辑前后 mesh fingerprint、cells、triangles、normal confidence 统计全部相同。

例 `near-1.log`：编辑帧 295 total **890.85 ms**，`water_edit_soak=14.82 ms`，
`untracked_cpu=869.24 ms`，`gpu_present=6.37 ms`。树重编译在原帧日志中落入 untracked CPU，
仅看 terrain_edit 或 GPU 汇总会漏掉它。此帧 source/collider/cache 队列均无 active/pending。
树动态更新（另一个阶段）total CPU **2.432 ms**、physics **0.880 ms**、pose wait **0.020 ms**、
pose GPU **0.0766 ms**，不是静态重编译约秒级停顿的来源。
CPU 段为主线程 wall-clock 计时，不是 CPU 硬件采样；upload 段混合 CPU/驱动/GPU等待，不作纯 GPU 时间解释。

## 假设验证与调用链

复现后、分段测量前提出了四个可证伪假设：

1. **树失效范围过宽**：不碰木但 brush 在树 read AABB 内仍编译，远处不会。观察符合，且输出树 fingerprint 不变。
   注意“过宽”指性能保守性，不意味着失效逻辑本身不正确：周边土也可能影响树法线/外露面，不能简单按木计数跳过。
2. **CPU 网格/蒙皮绑定主导**：细分后绑定应占大头而非 GPU readback。观察符合，定位到 `mesh.bind_tree`。
3. **地形同步 publication 主导**：若成立应解释 0.8 秒以上耗时。只有约 5 ms，排除为本例主因。
4. **碰撞/DDGI/GPU 等待主导**：若成立余下阶段/等待应占停顿大头。树 compile 已解释约 97% 编辑帧，
   idle/readback 很小，排除为本例主因；未对所有 DDGI 阶段逐个 ablation，也不宣称它们在任何场景都无开销。

对应代码（诊断分支）：

```text
真实鼠标 shovel: src/app/core/input.rs:814 try_shovel_dig
诊断 replay: src/app/core/water/simulation.rs process_water_edit_soak
  -> src/app/core/vegetation.rs:2797 apply_surface_terrain_removal
     -> TerrainSurfaceRemovalService::compile (同文件 :172，brush AABB)
     -> PlainBuilder::chunk_modify_surface_spheres_with_voxel_type
     -> clear_surface_occupants_in_brush / publish_visible_terrain
        -> src/app/core/visible_terrain.rs:535 commit_visible_terrain_revision
           -> src/tracer/tree_surface_cache.rs:44 observe_terrain
              read_bounds 任一个相交 -> 整体 dirty=true
  -> 同帧 src/app/core/mod.rs:3377 sync_static_raster_trees
     -> src/app/core/vegetation.rs:4835
        wait_idle -> 遍历所有 tree records -> atlas read -> append_region -> finish
        -> src/tracer/raster_tree.rs:159 bind_tree
           每个顶点扫描 trunks 做归属检查，角点 cache miss 再做最近 cone 查找
           -> src/tree_gen/skin.rs:18 SkinBinding::at_rest_position
              对全部 cones 做 signed_distance 的 min_by
        -> upload mesh/skin/secondary tree scene、attachments -> source.compiled
  -> begin_frame / publish_tree_surface_pose / 渲染
```

现有角点 BTreeMap 已复用重合顶点，但每次重编译都会重建它；归属测试仍按顶点扫描，
最近 cone 搜索仍按唯一角点扫描全部 1941 cones。该复杂度解释了实测绑定热点；
没有进一步用 profiler 拆分 signed_distance、map、内存分配各自占比，因此不把其中任何单条指令声称为已测热点。

可见地形 publication 会同步通知碰撞/DDGI、树 source 和藤蔓，但此次树绑定已经占主导。
没有看到 TREE_GUI replace/add 重放，问题不是普通编辑重新生成 canonical tree 描述或叶像素模型。
没有证明后续 Pretty Leaves 改动存在同一性能数据；它不在本次 checkout。

## 搜索与失败/非复现尝试

1. `cargo run --release -- --help`，输出 `/tmp/tree-stall-help.log`，发现现有水编辑 soak 和性能标记。
2. 未加探针前运行 `python3 scripts/check_tree_terrain_edit_perf.py target/tree-stall-baseline-01`：
   三次池边编辑，编辑窗口 269 帧中位数 **17.04 ms**、p95 **19.50 ms**、max **21.02 ms**，树重编译 0。
   此脚本只覆盖 unrelated edit，不能捕捉树 read AABB 内无关土编辑。
3. 初次 near/far/trunk 三笔 replay：`target/tree-stall-diag/{near,far,trunk}-1.log`；
   near 编译 819–1635 ms，trunk 750–829 ms；后续可能挖木且有较大噪声，不以 1635 ms 作为代表值。
4. 缩小为第一笔，交错 3 对：`target/tree-stall-diag/{near,far}-{2,3,4}.log`，near 稳定约 0.85 秒。
5. 最终加入材料计数/mesh bind 子计时，运行正式脚本 3 对，见主证据。

## 噪声与未知

- 每轮串行执行；使用共享 GPU 文件锁并检查其他 `re-flora` 进程；未启动可见游戏、未终止用户应用。
  `*.processes.txt`、`*.gpu.txt` 保存检查。没有发现另一个 re-flora GPU 实例。
- 桌面 GNOME、kitty、Steam webhelper 等仍存在；初始 GPU utilisation 约 9%，不具备完全独占 GPU/CPU。
  这是 CPU wall-time 及 FIFO 渲染测量，不用毫秒级差异声称微优化收益；800+ ms 稳定差值不依赖这种解释。
- 没有用户原存档、原工具/brush 设置和录屏；本诊断重现了树附近停顿这一症状，但不能证明所有编辑卡顿都同源。
- 未测平滑、放置、长按连笔、多树规模曲线、风动画 hit 到 rest-space 的完整鼠标路径。
  当前 replay 只复用核心真实编辑入口，未自动化 ray-pointer/UI 输入。
- 未关闭树 raster 做消融（避免混淆显示/查询路径）；没有改失效/绑定算法作实验修复。
- 材料 seed/世界运行时动画没有作为统一随机种子冻结；本例几何 fingerprint、土计数重复一致，
  帧号因时序不同为 295/298/297。探针日志和 `--perf` 自身也有开销。

## 后续修复建议（只建议，不实现）

1. **准确分离影响事实与缓存失效**：让树表面缓存按树/局部占用及 radius-two 法线邻域追踪依赖，
   避免一个大 read AABB 内任何土变化都强迫所有树重新绑定。不能仅凭 removed_wood=0 跳过，
   因为相邻土会改变树外露面和法线；也不能仅凭 cells/triangle 数未变判断网格相同。
2. **分离 rest topology / skin binding / surface shading 的缓存生命周期**：树 rest 描述没变时复用角点绑定；
   新出现角点才计算；保证删木、树替换、重叠树、相同角点一致性和 deterministic tie-break 正确。
   即使确实改木、必须重建局部表面，也不应无条件重做全部绑定。
3. **缩短必须执行的绑定工作**：测量空间索引/生成时 cone ownership 等方案，减少每个顶点和角点全 cone 扫描。
   保持 signed_distance 最近 cone 语义与平局规则；先做正确性对照，再 Release app 比较。
4. 若以上仍不足，再设计异步完整 publication；不能直接延迟半棵树或分帧部分提交。
   可见地形/树表面、碰撞、阴影、query 必须仍保持一致，遵守现有 publication 决策。

后续验收：重跑本脚本（每轮新输出目录）并比较 near/far 编辑帧、绑定与 compile 次数；
加树 read-bound 边界 sweep、紧贴树干土的法线变化、真挖木、树替换/年龄、两棵交叠树、多树测试，
核验网格/法线/碰撞/命中及 DDGI 一致性。连续编辑用 Release app 看 p95/p99/max，不以单元测试作性能证据。
在用户确认前不实施上述任何方案。

## 产物、验证与交接

保留文件：

- `src/app/core/water/simulation.rs`：opt-in near/far/trunk replay 和材料/耗时日志。
- `src/app/core/vegetation.rs`：opt-in 树编译分段日志，无行为修复。
- `scripts/diagnose_tree_edit_stall.py`：串行真实 app 对照、帧阈值红灯、配置备份还原。
- 本报告。原始大日志保留在本工作树 `target/`，未加入版本库。

验证：`cargo fmt --check`、`cargo check`、`cargo build --release`、
`cargo test tree_surface_cache`（5 项通过，仅逻辑检查）、
`uvx ruff check scripts/diagnose_tree_edit_stall.py`、`git diff --check` 全部通过。
全量 cargo test 未运行；全局 pyright 未安装，未作类型检查。
Rust 每个诊断阶段均做 check 和 hidden muted Release smoke，最新命令：

```bash
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --latest-log
```

`target/tree-stall-final/smoke.log` 成功退出，无 ERROR/panic/VUID；日志助手输出在 `latest-log.txt`。
六次正式对照也均成功退出且无上述错误。GUI/camera 字节已还原，GUI SHA256 前后相同；生成文件无 diff。
诊断阶段局部提交 `91d5e94e`、`3e60a634`、`d9a95ee6`；报告另行提交。
仅本分支本地提交，不 push，不操作其他工作树。停在分析交接，等待用户。
