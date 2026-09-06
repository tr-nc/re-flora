# 树枝纯黑体素：复现、根因与修复

基线：`29cbeff9cda43582ec1d091fc63f6ed7f94ab3be`；独占分支：
`agent/tree-branch-lighting`。2026-09-07。

## 结论

固定启动树上已确认的错误黑体素来自 DDGI 距离矩的遮挡误判。树枝附近多个
不同距离的表面落入同一方向纹素；实际畅通的 probe 线段被距离矩估计衰减到
极小概率，再被贡献权重阈值清零。它们背向太阳，失去环境光后呈纯黑。

修复在地形接收端用当前体素几何验证受到距离矩衰减的 probe 线段。只有精确
线段畅通才恢复该 probe 的贡献；否则保留原来的距离矩估计。probe 状态、空间
范围、法线权重、几何 revision 检验以及原有的四分之一体素 visibility bias 均保留。
没有增加环境光底值、调整材质/太阳/天空参数、扩大原点偏移或关闭阴影。
Raster 消费者和光照传输仍采用各自原有查询。

## 可运行复现

```bash
CARGO_BUILD_JOBS=2 cargo build --release
python3 scripts/check_tree_branch_lighting.py --output target/summer-evidence/repro
```

脚本使用当前 worktree 的 release 程序，内部以
`flock /tmp/re-flora-summer-gpu.lock` 保护实际运行，临时添加固定镜头并恢复配置。
它同时保存第一份已发布 DDGI 场的真实截图、浮点捕获、命令与原始 GUI 配置，
通过本 worktree 的 `--latest-log` / `--tail-latest-log` 保存日志。`--binary` 可选取
独立保存的 release 对照程序。`--screenshot` 单独保存运行 3 秒后的实际画面。

本次场景前提是基线 GUI 配置：树 seed `122`、size `20`、树龄 `1.0`、
时间 `0.47`、自动昼夜关闭。镜头 `(1.0, 0.64, 1.55)`，yaw/pitch `0/0`，
FOV `55°`，窗口 `1600×900`，tracer `800×450`。暂时隐藏叶片、粒子和云以暴露
树枝；没有禁用地形阴影。光照判定取生产浮点输出，不使用显示 RGB 的亮度阈值。

| 运行 | 树木采样 | 完全无能量且太阳线段畅通的采样 | 体素坐标 |
| --- | ---: | ---: | --- |
| 原查询，两次重复 | 34,429 | 103 | `(257,174,317)`、`(257,176,315)`、`(257,177,316)` |
| 修复后 | 34,429 | 0 | 无 |

原查询的 107 个 combined-zero 采样中，103 个满足上述无遮挡筛选。这里不把
太阳线段畅通等同于必须有直射光：三个表面法线背向太阳，直射光为零是正确的。

## 排除与定点证据

1. 真实截图先复现了枝条上的黑块，随后建立能返回 `RED` 的 app 命令，
   复现入口提交为 `0afacfe9`。
2. GPU 临时读回的三个法线分别约为 `(-.752,.373,.543)`、
   `(-.831,.532,.161)`、`(-.600,.479,.641)`；与真实地形快照的 5³ 邻域
   计算一致，仅有正常的 Oct16 量化差异。材质均为 `5`，albedo 为
   `(.590619,.434154,.107023)`。排除了零法线、黑材质和命中身份错误。
3. 唯一有有效正面几何权重的 probe `3000` 位于
   `(.998046875,.748046875,1.248046875)`。它的 RGB 辐照度之和约为
   `2.23–2.33`，但距离矩可见度为 `1.08e-12`、`1.28e-8`、`2.54e-11`。
4. 对保存的真实地形字节进行精确 voxel-AABB 线段检查：从原有 quarter-voxel
   bias 接收点到 probe 的三个线段均没有遮挡。实际 GPU 精确线段对照同样恢复
   了这三个体素。
5. 完全替换为硬可见性的诊断实验产生了其它黑块，未采用。最终方案保留原有
   软可见性，只用精确畅通证据纠正错误衰减。

临时 shader 捕获改写已经全部移除。原始诊断捕获在 `target/summer-evidence/`
下保留，并与正常辐照度捕获分开命名；`probe-normal` / `probe-cage` 是诊断数据，
不能作为最终画面或正常光照验收输入。

## 验证

- `CARGO_BUILD_JOBS=2 cargo fmt --check`、`cargo check`：通过。
- `python3 scripts/run_slang_tests.py`：8/8 通过。
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`：63/63 通过。
- `CARGO_BUILD_JOBS=2 cargo test`：915 passed、1 failed、1 ignored。唯一失败为已有
  PATT 相机 fixture 假设：测试要求一个名为 `snapshot` 的镜头，基线配置实际有
  四个不同镜头；失败是 `left: 4, right: 1`，相关 Rust/相机配置均未改动。
- `CARGO_BUILD_JOBS=2 cargo test -- --skip app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`：
  915 passed、0 failed、1 ignored、1 filtered out。
- 同镜头生产回归 `recovery` / `recovery-paired`：GREEN，无非有限数。
- `recovery-sealed`：360,000 个接收采样的环境辐照度全部精确为零，
  `--require-zero-rgb --require-nonnegative-rgb` 通过。
- `recovery-walls`：生产薄墙场景捕获有效、非负且无非有限数；尚不把这次基础
  检查宣称为整个 DDGI correctness suite 的通过证据。
- `flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5`：
  通过；本 worktree 的 `--latest-log` / `--tail-latest-log` 确认
  `phase=complete failures=0`、`Application exited successfully`，无 ERROR、panic、
  Vulkan VUID 或 device-lost。日志见 `target/summer-evidence/hidden-smoke*`。

## Release 运行成本对照

按控制器追加要求，仅量化当前修复成本，没有扩展优化或设立新的放行阈值。
原查询和候选分别独立 release 构建，保存为
`target/summer-evidence/bin/re-flora-{reference,candidate}`。原查询采用基线 shader
和能力日志，候选采用 `191c8f9f`；当前默认 `target/release/re-flora` 已恢复为候选，
SHA-256 与保存的候选一致。构建均使用 `CARGO_BUILD_JOBS=2` 和本 worktree 的默认 target。

两者依次运行同一固定树、镜头、GUI 字节、分辨率及参数：RTX 3060 Ti，物理输出
`1600×900`、tracer `800×450`，自动选择 MAILBOX。树编译几何统计、太阳方向/颜色/
亮度、首次 DDGI 发布 revision 均一致。GUI SHA-256 为
`0e592f7c770b8f754ced628dfb7310a3310e8ea5a2c5f053bb28f7d3dfc62217`。
测量使用上述暴露枝条的场景，不代表所有叶片与特效开启的完整场景成本。

```bash
# binary 分别取当前 worktree 中保存的 reference / candidate release 程序；
# 临时镜头与 GUI 的保存/恢复由 target/summer-evidence/benchmark.py 完成。
flock /tmp/re-flora-summer-gpu.lock "$binary" \
  --hidden --mute --windowed --perf \
  --camera-snapshot tree-branch-regression \
  --no-flora --no-particles --no-clouds --no-god-rays --no-lens-flare \
  --auto-exit 55
```

每个程序运行 55 秒，取共同预热后 frame `3300..6900`（含两端），每 30 帧的
原生 profiler 日志取一个样本，每项均为 **121 个 GPU / 121 个 app 样本**。
所有预期帧齐全，GPU scopes `dropped=0`。两个完整运行分别记录了 533 / 399 个
GPU 帧样本，最后记录帧号为 16020 / 12000；下表仅比较共同窗口。
每 0.5 秒检查一次 `rustc`，两次运行观察到的最大编译进程数均为 0。
启动耗时和截图 FPS 均未进入统计。

| 原生计时项 | 原查询中位数 (ms) | 候选中位数 (ms) | 增量 (ms) | 变化 | 样本数（原/候选） |
| --- | ---: | ---: | ---: | ---: | ---: |
| GPU 总帧 `frame.render` | 1.447 | 2.448 | +1.001 | +69.2% | 121 / 121 |
| GPU 地形主 pass `tracer.pass` | 0.425 | 1.443 | +1.018 | +239.5% | 121 / 121 |
| GPU tracer 合计 `tracer.render` | 0.547 | 1.564 | +1.017 | +185.9% | 121 / 121 |
| GPU 阴影预处理 `tracer.shadow_prepass` | 0.531 | 0.497 | -0.034 | -6.4% | 121 / 121 |
| App 帧耗时 `frame.cpu_total` | 3.333 | 4.434 | +1.101 | +33.0% | 121 / 121 |

GPU pass 是嵌套 scope，不能相加；app 帧耗时是主线程该帧的实际 elapsed time，
包括 swapchain acquire/present 等等待，不是纯 CPU 算术成本。主要新增 GPU 时间
落在消费修复查询的 `tracer.pass`。此场景成本明显，不能据此宣称性能验收通过；
这是一组顺序对照，没有跨运行方差/置信区间，也没有推广到其它 GPU、分辨率或
probe 密度。视觉候选与测量结果一并交付。

原始日志、每项中位数/样本数、二进制 hash、命令、场景标记和一致性检查保存在
`target/summer-evidence/cost/{reference,candidate}.{log,json}`、`comparison.json`、
`validation.json`。每次运行另存同 worktree 的 `*-latest-log.txt` / `*-tail.log`；
两次均正常退出、shutdown failures=0，无 ERROR/panic/VUID/device-lost。

## 真实证据位置

以下路径均相对当前 worktree：

- `target/summer-evidence/close-bare-image/final.png`：修复前，3 秒近景。
- `target/summer-evidence/recovery-image/final.png`：修复后，同场景同镜头，3 秒近景。
- `target/summer-evidence/reference-paired/final.png` 和 `light.rfirr`：原查询第一份已发布场的成对捕获，RED 103。
- `target/summer-evidence/recovery-paired/final.png` 和 `light.rfirr`：修复后的成对捕获。
- `target/summer-evidence/repro-red{,-repeat}/result.json`：原查询重复判红结果。
- `target/summer-evidence/probe-cage/probes.json`：逐 probe 的 GPU 诊断读回。
- `target/summer-evidence/tree.rflterrain`：真实游戏保存的地形与树木快照。
- `target/summer-evidence/{final-check,cargo-test,slang-tests,capture-tests}.log`：检查日志。

## 范围与交接

生产修改只有 `shader/slang/ddgi_query.slang` 和 `src/tracer/mod.rs` 的一条能力日志。
`src/tracer/mod.rs` 是共享文件，集成时仅保留该条日志变化。没有修改
`src/gui_adjustables.rs`、环境光权威结构、相机/GUI 默认配置；没有生成文件 diff。

固定场景确认的错误黑体素已恢复，实际遮挡的暗处保留。尚未覆盖所有树 seed、
镜头、probe 密度、可编辑场景以及其它 GPU。精确线段的额外 release 成本如上，
不为本次视觉候选设新性能放行阈值，也不宣称性能验收通过。
