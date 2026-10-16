# 铺路 Release 性能测量与预检合并

## 可重复的测量 interface

```sh
uv run --python 3.12 scripts/perf_suite.py run pavement-drag \
  --output target/perf-pavement/baseline.json
```

`config/perf_scenarios.toml` 的 `pavement-drag` 使用真实 Winit pointer routing、
平坦土壤、Slabs / Brick 两条各 1.3 world unit / 130 步的持按路径。
日志必须含两个完整案例、材质 CRC、释放静音及成功退出标记，且无致命诊断。
比较器要求两次运行的材质结果和步数完全相同；不把做更少工作当作优化。
普通 pointer fixture 和玩家花园不变，只有 opt-in `RE_FLORA_PAVEMENT_REVIEW=perf`
使用此较长路径与相机。

`RE_FLORA_PAVEMENT_PERF_REPORT=1` 在生产 advance/release interface 记录耗时、
新增石块和当前 gesture 的体素 receipt 容量，并记录 atlas transfer/wait。
`advance` 是路径预检、发布、声音生命周期的 CPU wall time，**不是整帧时间**，
不包含之前的 pointer ray sampling；`publish_step` 只选实际新增石块的 advance。
Release 另外计量。没有调高每帧行数，也不改变输入、材质或撤回语义。

`voxel_receipt_bytes` 只计 `VoxelUndo` / `VoxelDiff` vector 的 capacity，
不含 flora、ownership HashSet、allocator 开销或进程/GPU RSS；统计在工作计时结束后
执行。它是历史记录的局部占用证据，不是完整内存基准。普通运行不遍历这些统计。
每次运行只有两个 release 样本，需多轮、顺序反转比较，不能据此外推所有设备。

## 初始结果（2026-10-16，Apple M4 Pro / macOS）

未优化的 Release 连续两次测量：

| 指标 | 第一次 median / p95 | 第二次 median / p95 |
| --- | --- | --- |
| advance（260 samples） | 7.921 / 17.844 ms | 7.800 / 18.072 ms |
| 实际发布步骤（42 samples） | 17.288 / 21.491 ms | 17.326 / 21.876 ms |
| release（2 samples） | 41.760 / 43.600 ms | 41.774 / 43.366 ms |

第一次活动区间包含 4178 次 atlas read/wait，合计 1989.119 ms；145 次 voxelizer
GPU dispatch/wait 合计 31.900 ms；44 次 Visible Terrain Publication 合计 143.860 ms。
Atlas 统计包含两个结果 witness read，但不含场景构建期。
两种材质分别保留 26 / 27 个 course，峰值 voxel receipt 容量 643968 / 529472 bytes，
owned voxel 数 23864 / 25796。尚无证据要求重设计整个异步 GPU 或历史存储系统。

优先验证的解释：逐石预检的小 readback 往返（预计合并 transfer 后字节相同但等待
次数下降）；其次是 CPU ground/footprint 计算；Visible Terrain Publication 是
独立固定成本。基线阶段只定位成本；后续优化验收如下。

证据：`target/priority-optimization/pavement-baseline{,-repeat}.{json,log}`；
带相同测量代码的原始 Release 已复制到 `target/priority-optimization/pavement-before`。
验证：54 个 pavement Rust 测试、19 个性能解析/原生守卫 Python 测试、原有 native
pointer fixture、普通 hidden/mute 运行、`cargo check` / Release build 通过；
Pyright（Python 3.12）无错误。Ruff 仍报告旧代码的 EXE001、SIM102、I001、RUF100，
未夹带无关格式修复。没有 GUI 配置或生成文件变化。

## 预检合并：影响范围

Slabs / Brick 的活动候选与发布预检会按每块石头同步读取 atlas；一帧可能重新检查
多排 lookahead，许多小 transfer 的提交/等待进入交互关键路径。Dirt 不走石块
atlas 预检。仅在上述 M4 Pro Release 场景量化了性能，不外推所有 GPU 或完整帧率。

## 根因

同一排准备期间没有 atlas 写入，各石块的保护区域虽然独立，却各自提交并等待 GPU。
等待粒度与只读合同不匹配。不能把整个排的旧快照当作逐石 before-image：相邻石块
可能共享体素边缘，撤回必须保留真实写入顺序。

## 引入时间

逐石预检及 `prepare_pavement_course` 路径可追溯至 `b12e36fe`（2026-10-11）。
即时逐石 before-image 的合同在 `190c9249`（2026-10-13）已存在，不是本次
EditHistory refactor 新增的等待。本次测量基线为 `ccb7fcd0`；没有对历史性能
做同场景二分，不能量化各历史提交分别引入了多少延迟。

## 干净修复

- `PlainBuilder::read_chunk_atlas_regions` 拥有一次同步 transfer 的完整合同：
  先验证所有区域、打包对齐，再一次提交/等待，按请求顺序返回独立快照。
  单区域旧 interface 委托给同一实现；空区域仍返回空数据，越界/溢出在提交前拒绝。
- 内部 `AtlasReadPlan` module 隐藏 R8 打包/偏移，生产 mapped-buffer adapter
  与确定性内存 adapter 经过同一 `read_results` seam。Image module 统一拥有
  image/buffer barrier 和多区域 copy；不另写一套同步协议。
- `prepare_pavement_course` 合并一排的预检，仍使用每块石头原来的精确保护范围、
  深度和 ownership。不会用更大的合并 AABB 改变判定，也不跨帧缓存过期数据。
- 每次 stamp 的即时 before / after、receipt、外来体素保护、失败后的可见发布、
  flora 恢复和撤回次序保持不变。临时快照只在该排预检期间持有；未引入长期缓存。

### Release 顺序反转比较

```sh
uv run --python 3.12 scripts/perf_suite.py run-ab pavement-drag \
  --baseline-binary target/priority-optimization/pavement-before \
  --candidate-binary target/priority-optimization/pavement-after \
  --output-dir target/priority-optimization/pavement-ab
```

A,B,B,A；A 是 `ccb7fcd0` 的实测二进制，B 是本次 batched-read 候选；两者包含
完全相同的测量代码。报告的 checkout commit/dirty 字段描述运行时工作树，
不能当作复制二进制自身的版本。两个材质 CRC 在全部运行中一致：342710807 /
3557010619；课程、ownership 与 receipt 容量完全一致。

| 指标 | 基线 median / p95 | 候选 median / p95 | median 改变 |
| --- | --- | --- | --- |
| advance（各 520 samples） | 8.463 / 19.297 ms | 6.399 / 16.407 ms | -24.39% |
| 实际发布步骤（各 84 samples） | 18.078 / 23.327 ms | 15.541 / 19.871 ms | -14.03% |
| release（各 4 samples） | 43.671 / 45.617 ms | 41.313 / 42.067 ms | -5.40% |

每次两条路径的 atlas transfer/wait 从 4178 减至 1709（-59.10%）。两轮累计
等待的均值从 2124.576 降至 1643.816 ms（-22.63%）。三个场景专用 5% 回归预算均
通过；release 样本仍少，不作统计显著性或零卡顿声明。同步等待并未全部消除，
也没有为了本轮结果重设计逐石事务为全异步调度器。

最终验证：1606 app + 4 collision、65 VKN 测试通过，7 个既有 ignored；
`cargo fmt --check` / `cargo check` / Clippy（既有警告）/ `cargo build --release`
完成。六个真实铺路/石块验证入口全部通过，包括重叠撤回、外来地形、植被身份、
取消、保护目标、弯道和存档重载；普通 hidden/mute、raster-tree、rooftop Soil
stability 与 50% Glass GPU readback 均无致命诊断。GUI 配置和生成文件没有变化。
未进行跨设备性能验收或完整 RSS 测量。

证据：`target/priority-optimization/pavement-ab/{comparison,readback-summary}.json`、
各轮 `.log` 及 `final-*.log`。
