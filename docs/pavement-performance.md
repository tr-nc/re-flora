# 铺路 Release 性能基线

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
独立固定成本。本次基线只定位成本，尚不声称完成优化。

证据：`target/priority-optimization/pavement-baseline{,-repeat}.{json,log}`；
带相同测量代码的原始 Release 已复制到 `target/priority-optimization/pavement-before`。
验证：54 个 pavement Rust 测试、19 个性能解析/原生守卫 Python 测试、原有 native
pointer fixture、普通 hidden/mute 运行、`cargo check` / Release build 通过；
Pyright（Python 3.12）无错误。Ruff 仍报告旧代码的 EXE001、SIM102、I001、RUF100，
未夹带无关格式修复。没有 GUI 配置或生成文件变化。
