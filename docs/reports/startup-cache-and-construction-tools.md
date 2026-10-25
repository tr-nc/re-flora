# 启动缓存与独立 Pave / Wall 工具

2026-10-25，本地 main；未发布、未自动启动可见游戏。本文补充而不覆盖
[10 月 21 日的原始启动记录](startup-progress.md)。

## 启动缓慢：影响与根因证据

RTX 3060 Ti / NVIDIA 580.159.04，默认验证层 Release、hidden、mute。
用户反馈每次启动都等待约 30 秒。改动前连续运行是 30.522 / 30.359 秒，
其中 compute pipelines 为 22.573 / 22.892 秒，graphics 为 3.548 / 3.563 秒。

不能把“没有应用显式 cache”直接等同于“之前也一定慢”，也不能把出现进度日志
当作退化起点。10 月 21 日加入 progress **之前**的原始日志已经静默等待
27.991 秒；那次提交增加观察器，没有改缓存、shader 或验证层。

本次进行了真正改变单一变量的诊断：

| 对照 | 结果 |
|---|---|
| 关闭验证层的 Release | 仍约 29.926 秒，`tracer.comp` 单项 19.083 秒；验证层不是主要原因 |
| 不加应用持久缓存，只改 NVIDIA 隐式缓存目录 | 空目录首次 34.395 秒；第二次 3.839 秒，tracer 19,046 → 8 ms |
| 不加应用持久缓存，使用原 NVIDIA 目录，仅将缓存容量环境变量设为 2 GiB | 首次 30.188 秒；第二次 4.076 秒，tracer 19,012 → 59 ms |

已有驱动缓存目录约 1.1 GiB，当前驱动子目录约 939 MiB，其中一个 bin 约
556 MB。隔离目录与提高容量均恢复跨进程复用。这把现象定位到**当前驱动隐式磁盘
缓存的复用/容量状态**，不是进度线程、生成花园或每次重新编译 Rust。
NVIDIA 本机文档 `/usr/share/doc/xorg-x11-drv-nvidia/html/openglenvvariables.html`
也明确指出缓存路径/容量可配置，局部提高容量后其他未设置变量的进程可能恢复默认
限制并清理超限缓存。本次没有删除用户的驱动缓存，没有修改 shell/profile，也没有
把 NVIDIA 专用环境变量作为正式修复。

**引入时间边界**：没有历史目录快照，无法证明缓存在哪一天达到限制或确定某个代码
提交是退化起点。能确认 10 月 21 日 progress 提交前已慢，且相同 shader/驱动在有效
缓存下可以很快。较早体验快与今天每次重新付出驱动成本并不矛盾。

## 修复：设备拥有 Vulkan pipeline cache

`crates/re-flora-vkn/src/context/pipeline_cache.rs` 为唯一缓存所有者，由 `Device`
持有。所有 compute/graphics 创建使用同一个带互斥保护的 cache；快照/销毁遵守同样的
同步边界。首个游戏帧完成时保存，晚建 pipeline 则在设备销毁前再次保存。

- `build.rs` 已负责 Slang → SPIR-V；GPU/驱动相关 pipeline 产物在运行时缓存。
- 缓存只是创建加速器；每次仍把**当前 shader、layout、specialization 与固定管线
  状态**提交 Vulkan，由驱动决定哪些条目适用，不拿缓存替代 shader 或 pipeline 配置。
- BLAKE3 散列完整可执行文件字节，覆盖未提交 Rust、嵌入 SPIR-V、编译选项与依赖。
  不依赖 Git SHA、版本号或时间戳。Linux 读取 `/proc/self/exe`，避免程序运行期间
  可执行文件路径被替换造成身份混淆。
- 构建身份还包含 vendor/device、driver/API version、pipeline cache UUID 与位宽。
  应用格式版本、payload 校验和与 Vulkan header 也在传给驱动前检查。
- 每个可执行文件路径/设备一个槽位；新构建替换旧缓存，不为每次构建无限累积文件。
  不同 worktree/安装路径互不覆盖。按完整二进制失效是保守策略：纯 Rust 改动也会让
  应用缓存失效；驱动自身仍可正确复用未变化的 pipeline。
- 缺失、不兼容、截断、校验失败均冷启动；驱动拒绝已有数据则重试空 cache。
  缓存 API 不可用则回退空句柄创建 pipeline。无法写入只警告，不阻止渲染。
- 文件写入有尺寸限制、唯一临时文件、sync 和原子替换。并发进程可能损失命中，
  但不会发布半个文件。无任何缓存问题可改变当前 shader 的输入。

默认目录：Linux `$XDG_CACHE_HOME/re-flora/pipelines-v1`，未指定则
`~/.cache/re-flora/pipelines-v1`；Windows `%LOCALAPPDATA%/re-flora/pipelines-v1`；
macOS `~/Library/Caches/re-flora/pipelines-v1`。其他平台尚未实机验证。

诊断变量：

- `RE_FLORA_PIPELINE_CACHE_DIR=<dir>`：覆盖缓存根目录，支持隔离验证。
- `RE_FLORA_PIPELINE_CACHE=off`：禁用应用持久缓存，保留进程内 cache。

普通日志包含 `[PIPELINE_CACHE] build_key`、hit/miss 原因和保存结果。
删掉应用 cache 可触发冷启动，无需修改用户 GUI 设置。

## 实测与失效验证

应用显式缓存、默认驱动环境、验证层开启：

| 阶段 | 冷启动 | 热启动 1 | 热启动 2 |
|---|---:|---:|---:|
| compute pipelines | 22.105 s | 0.123 s | 0.121 s |
| graphics pipelines | 3.430 s | 0.085 s | 0.082 s |
| 启动总计 | 29.927 s | 4.211 s | 4.120 s |

禁用 NVIDIA 隐式磁盘缓存，显式应用缓存仍正常加速：独立原生回归冷启动
34.654 秒、热启动 3.933 秒，pipeline 合计 29.442 秒 → 0.105 秒。
剩余主要开销是地形/模型准备，不是 pipeline。不同运行的地形准备有波动，以上不是
跨设备性能保证。

原生回归入口（不加入常规 `cargo test`）：

```sh
cargo build --release
node scripts/check-pipeline-cache.mjs
```

脚本使用独立目录，真实跑冷/热启动、错误构建身份、损坏 payload、截断、恢复命中、
不可写路径和禁用持久缓存；断言 ready、正常退出与无 Vulkan errors。
保存日志而不设跨 GPU 的固定性能门槛。本机各分支全部通过。

还实际临时修改了 egui fragment shader 输出，`cargo check` + Release 重建后身份
从 `b8dd2593…` 变成 `f3ee7011…`，明确报告构建变化 miss；临时修改 Rust 的退出消息
也改变身份（`2a1b5636…`）。两项探针均已撤销、重新 check/build，最终产物恢复原身份。
这不仅是模拟 key 改动的单元测试。热缓存不会掩盖新的源码构建。

最终默认目录首次准备后，下一轮普通 Release 启动 **4.044 秒**，命中已预热缓存。
没有保留关闭验证层的诊断产物。

## Pave 与 Wall

工具栏现在有独立 **Pave (V)** 和 **Wall (J)**，选中的工具是配方类型的唯一来源。
两个工具共用已有路径/地形发布机制，但不再由 Debug checkbox 切换工具身份。

- Tab 分别切换铺路 surface / 墙 material。
- Shift + wheel 分别调整铺路宽度 / 墙厚度；Ctrl + wheel 只对 Wall 调墙高。
- 预览、提示、左键拖动、材质面板避让和切换时取消旧手势都覆盖两种工具。
- 旧 `low_wall_enabled` 设置在加载边界退休；已保存的两类材质、宽高与共享路径
  参数保留。加载不写回文件。Debug 的相关控件不再被旧模式条件禁用，仍通过统一
  Save 保存，在同一个 Pavement & Walls 调参组中。
- 生成字段由 `cargo check` 再生成；没有手改生成文件。

验证包括工具选择/手势路由测试、旧 true/false/缺失模式迁移、保存/搜索回归、
13 个文字工具的窄屏布局测试。真实原生 wall fixture 通过三个材质/两种高度、冻结
配方、保护已有地形和声音生命周期；真实 Winit 合成 pointer fixture 通过 Slabs 和
Brick 拖动、预览、冻结宽度与释放。这不是人工视觉/OS 鼠标验收。

## 验证状态与已知基线

- `cargo fmt --check`、`cargo check`、`cargo build --release` 通过。
- vkn **75 tests passed**；wall/pavement/text-toolbar 聚焦测试分别 14 / 65 / 4 passed。
- 全量 app 测试 **1699 passed / 9 failed / 14 ignored**；collider 4 passed。
  9 项失败已在改动前 `a594d21e` 的独立临时 worktree、相同用户苹果设置下复现：
  7 项 raster-tree 旧 fixture、1 项 sprinkler sampling 默认值、1 项苹果默认值。
  没有把这些无关失败改掉或冒充全绿。由于 app 测试返回失败，后续 integration targets
  未由全量命令执行；本次独立原生脚本另行通过。
- 最终默认验证层 hidden/mute smoke 正常退出，无 ERROR/VUID/panic。
- 用户现有 `config/gui.toml` 苹果 0.8 和浮点格式改动原样保留、未提交。
- 临时 shader/Rust 探针、无验证层诊断构建和 baseline worktree 均已清理/替换。

本机诊断日志：`target/startup-diagnosis/`；原生回归：
`target/pipeline-cache-check/run-pX4b3L/`。这些机器本地证据不是可移植测试资产。
