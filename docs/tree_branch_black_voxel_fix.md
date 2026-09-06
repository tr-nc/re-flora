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
- 排除该项再运行：915 passed、0 failed、1 ignored、1 filtered out。
- 同镜头生产回归 `recovery` / `recovery-paired`：GREEN，无非有限数。
- `recovery-sealed`：360,000 个接收采样的环境辐照度全部精确为零，
  `--require-zero-rgb --require-nonnegative-rgb` 通过。
- `recovery-walls`：生产薄墙场景捕获有效、非负且无非有限数；尚不把这次基础
  检查宣称为整个 DDGI correctness suite 的通过证据。
- 标准隐藏静音 smoke 及其日志见 `target/summer-evidence/hidden-smoke*`。

## 真实证据位置

以下路径均相对当前 worktree：

- `target/summer-evidence/close-bare-image/final.png`：修复前，3 秒近景。
- `target/summer-evidence/recovery-image/final.png`：修复后，同场景同镜头，3 秒近景。
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
镜头、probe 密度、可编辑场景以及其它 GPU。精确线段带来额外运行成本；release
对照测量将单独记录，不为本次视觉候选设新性能放行阈值，也不宣称性能验收通过。
