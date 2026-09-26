# 树表面依赖与发布优化：可复用规则和 Release 验证

接续 `tree_edit_stall_optimization.md`。本轮目标是在保留正常树编辑/法线更新的前提下，
消除树旁无关编辑的整树检查及重复发布，不采用复现点特判、距离阈值或材料计数捷径。

## 设计与实现

### 精确输出相等时，保留已发布资源（198b61b0）

`Tracer::publish_static_raster_trees` 统一拥有 mesh 与 attachment 发布决策；
App 不再自行编排两次上传。比较：

- 顶点完整字节，包含位置、中心、法线、confidence；
- 索引、solid occupancy、蒙皮绑定、cell vertex lookup；
- attachment 的 anchor、tree id、branch。

这些事实全部相等才不上传。保留 GPU 资源、查询/refit 结构与 attachment pose 历史；
不因缓存 memoization 或 terrain revision 更新而重复创建相同几何。
比较不是 hash，也不是仅比较三角形数量。开始写资源前清除 publication-valid，
全部 fallible 上传结束才恢复，避免失败后的重试将部分上传当成完整发布。

该阶段 near 编辑从 75–76 ms 到 **42.34 ms**（阶段性单次测量）；
树发布段从约 33 ms 到 **0.445 ms**，`published=false`。
依赖确实相交、但最终表面未变的情况仍由这层保护。

### 由采样函数推导依赖，而不是使用整树 readback AABB（3a1f2aaf）

`RasterTreeMesh::append_region` 同步生成 `TreeSurfaceDependencies`。
每个 authored world-space round cone 的 AABB 扩展表面采样 halo，再裁剪到实际读回范围。
当前实现保守地保留每个 cone 的盒子，不用启动时当前木体素做唯一依据。

- 两枝之间的大块空隙不再算作依赖。
- 当前空的潜在木体位置仍算依赖，后续添加木体素不能漏更新。
- 周围土石影响外露面/法线的采样位置仍算依赖。
- 采样循环、依赖扩展和读回 halo 共用 `TREE_SURFACE_SAMPLE_RADIUS`。
- `TreeSurfaceCache` 继续单独拥有 canonical/terrain revision 与 sticky dirty 规则。
- 依赖使用闭区间 voxel 交集，兼容单体素和边界编辑。测试发现普通 `UAabb3::intersects`
  要求正体积相交，会漏掉边界；本轮只修正依赖模块的判定，没有修改全局几何语义。

调用者只把 mesh 生成的依赖交给 cache，无需知道锥体分布、halo、编辑材料或 brush 类型。
这是采样结果对输入的依赖声明：可替换为更精确的空间索引而不改工具，且适用于所有经过
visible terrain publication 的编辑入口。仍保留同步完整发布，不引入异步半成品状态。

架构合同已补充到 `docs/tree_publication_architecture.md`。

## Release 实测

与前两轮同一 base 派生分支、机器、默认 config、seed=122 树与固定球形移除 replay：
near 距树根 XZ 0.125，far 0.625，radius=0.055，5 秒暖机，7 秒退出，交错 3 对。
具体坐标与窗口设置见原诊断报告；near 每次仍移除 583 dirt、far 616 dirt，wood=0。

```bash
cargo build --release
python3 scripts/diagnose_tree_edit_stall.py target/tree-publication-final-perf --repeats 3
```

| Release 编辑帧 ms | 第 1 次 | 第 2 次 | 第 3 次 | 中位数 |
|---|---:|---:|---:|---:|
| near | 24.62 | 23.11 | 22.81 | **23.11** |
| far | 23.43 | 21.84 | 23.82 | **23.43** |

六次的编辑后树重编译数都是 **0**。编辑前常态帧中位数约 17.0–17.6 ms。
前一轮 near 中位数 75.63 ms；原始诊断为 888.35 ms。
本场景已没有可分辨的 near 额外整树工作；不能用 0.32 ms 差异宣称 near 比 far 更快。
仍保留一般地形编辑约 22–25 ms，而不是宣称任何操作都能稳定 16.7 ms。

另一次独立 3 对：`target/tree-dependencies-perf/`，near 22.22/23.37/24.52、
far 23.33/23.77/23.93 ms，与最终复测一致。

最终测量 HEAD `3a1f2aafe79515cd357febd9866966d06138b5b3`，source.diff 为空；
二进制 SHA256 `b5d293c1afc164780aa9ca26fb8f9a239c22537da5e5058cc160b0ba0d9cd10c`。
证据目录内保留 GUI/camera 原始字节、进程/GPU 检查和完整日志。未混跑其他 re-flora；
桌面 GNOME/Steam 等噪声仍存在，不是独占 GPU 环境。

### 真挖木不能跳过

```bash
env -u WAYLAND_DISPLAY RE_FLORA_TREE_EDIT_DIAGNOSTIC=trunk \
  target/release/re-flora --hidden --mute --perf --water-edit-soak --auto-exit 14
```

`target/tree-dependencies-trunk.log`：首两笔移除 212、308 木体素，均重编译并 `published=true`；
第三笔移除 592 地面体素，未触发树重编译。成功退出，无 ERROR/panic/VUID。
前一阶段 `target/tree-publication-trunk-1.log` 中第三笔仍重编译，但正确判为 `published=false`，
分别验证两层优化的作用。真实改木仍有构建/发布成本，本轮没有承诺让砍树等同无树编辑。

## 正确性证据

### 常驻测试

- 精确相等判据逐项变更测试：法线、confidence、位置、索引、solid occupancy、绑定和 lookup 任一变化都不能判为未变。
- 空隙编辑不失效；法线 halo 编辑和当前尚未存在的木体位置编辑必须失效。
- 保留 canonical 改变、多个树依赖、sticky dirty、空缓存不能被无关编辑复活等测试。
- 对小 fixture 的 **512 个 voxel × 3 个材料值（empty/dirt/wood）**逐一编辑，独立完整重建表面；
  只要任何可观察结果变化，原 mesh 的依赖必须命中。含添加木、删除木、无木变化但土改变法线等情形。

`cargo test` 最终完整通过：**1171 passed、4 ignored**，另一目标 **4 passed**，耗时 201.65 秒。
运行时间主要来自已有 climbing plant 测试；不把单元测试当性能证据。

### 临时强制重建 oracle（已清理，非性能证据）

为检测错误跳过，临时让每一帧都完整读取、提取、绑定树表面；
当正式依赖逻辑声称 current 时，逐项比较完整新旧表面，必须完全一致，否则报错。

- near：343 次 `full_rebuild_matches=true`；
- far：239 次；
- trunk：360 次；
- 隔离树 lifecycle smoke：124 次。

三个 replay 均运行 14 秒，包含原来的三笔；不是只检查最终第一笔。
全部成功，无 oracle mismatch、ERROR/panic/VUID。
日志 `target/tree-dependencies-oracle-{near,far,trunk,smoke}.log`。
临时源补丁 `target/tree-dependencies-oracle.patch` 保存复查；源码和最终二进制均已恢复为正常路径。

### 已知标准 smoke 阻塞仍存在

最终版再次运行标准 `--raster-tree-smoke`，仍在加入局部点光 fixture 后遇到：

```text
publish ready DDGI staging Volume
DDGI staging publication lost its owner radiance tuple
```

`target/tree-publication-final-raster-smoke.log`。上一轮已在未修改版 `ec1e0498` 验证同一失败，
不属于本轮修复范围。**完整点光/DDGI smoke 不算通过。**

临时 oracle 版本仅省略该点光 fixture，其余风、GPU readback、posed 编辑、年龄、删除、替换与 A/B 循环不改；
隔离 smoke 完成 160 帧并成功退出，验证资源复用没有破坏树 lifecycle。该临时省略未保留到源码或最终构建，
也不用于性能测量或冒充完整光照验收。

## 交接

- `cargo fmt --check`、`cargo check`、完整 `cargo test`、Release build、`git diff --check` 通过。
- 最终 `cargo run --release -- --hidden --mute --auto-exit 0.5` 成功，无 ERROR/panic/VUID。
  日志 `target/tree-publication-final-smoke.log`；`--latest-log` 输出为 `target/tree-publication-final-latest-log.txt`。
- GUI SHA256 前后相同：`c45e9ca4a8bb6fa10b88818721b2ba455532d28c1fddbb47659db5c742367256`。
  camera 已还原，生成文件未修改，无 shader 改动，未启动可见游戏。
- 只提交本工作树/分支；无 push、merge 或对其他 Worker 的操作。
- 大型多树场景的依赖查询规模和用户原存档仍未测；当前区域是保守 AABB，部分实际无影响编辑仍可能重建，
  由精确输出复用层兜底。未来可在 `TreeSurfaceDependencies` 内优化索引，不需要把特殊判断加进编辑工具。
