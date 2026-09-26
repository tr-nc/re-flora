# 天空与太阳照明控制

本分支实现提交：`e0cd8f2b`，基于 `29cbeff9`。只修改 `agent/sky-sun-controls`，未合并、推送或发布。

## 操作与默认值

按 **R** 打开 **Debug Panel → Atmos → Scene lighting**：

| 控件 | 唯一配置 ID | 范围 | 默认 | 语义 |
| --- | --- | --- | --- | --- |
| Sun Lighting Strength | `sun_luminance`（复用） | 0–10 | 2 | 太阳直射、树叶透射，以及太阳经表面反弹的间接光 |
| Sky Lighting Strength (1 = original) | `sky_light_strength`（新增） | 0–4 | 0.5 | 天空照亮场景的倍率；0 关闭该光源，1 恢复原天空照明 |
| Sun Disk Brightness (appearance) | `sun_display_luminance`（复用） | 0–10 | 1.65 | 位于下方 Sky appearance & time；控制可见日盘，不代替场景照明 |

太阳原本已独立可调，但名称 Sun Luminance 与 Sun Display Luminance 不够清楚。天空原本没有照明倍率，所有漫反射传输固定取 `getSkyColor × π`。新默认保留太阳 2，将天空填充光减半，以增强日照/阴影层次；1 是随时可恢复的原版基准。2 与 0.5 是不同定义下的艺术控制值，**不代表实测太阳/天空能量比为 4:1**。

改值逐帧应用，无需 Save 或重启。DDGI 需要等待完整场发布后稳定；快速拖动期间遵循既有不可变 in-flight / latest-wins 规则，不原地修改正在构建的快照。面板顶部 **Save** 原子写入当前 worktree 的 `config/gui.toml`，显示 Settings saved；重启重新加载。旧文件缺少天空控件时，从编译时同一份 config schema/default 补入，不覆盖已有太阳值或天空值。

## 数据流与支持范围

`config/gui.toml → generated GuiAdjustables → freeze_render_frame_inputs → AuthoredEnvironmentLighting::observe`。

天空倍率进入 Authored Lighting identity，变化立即发布为 `TransportInputStep`，重置 DDGI 辐照历史。下游从同一权威派生：

- 实时 `U_SunInfo`：地形路径追踪 sky miss、Legacy flora 环境项、玻璃离屏表面回退。
- 不可变 `DdgiRadianceSnapshot → U_DdgiRadianceSun`：全局天空积分、probe sky miss 与后续反弹。DDGI 默认地形、树叶、flora 从共享 publication/cache 接收结果。
- 太阳的既有权威和树叶透射路径保持原样。天空倍率不乘最终整幅间接光，因此不会错误压低太阳反弹、局部灯或自发光。
- `environment_lighting.slang` 的 consumer 不需要再乘倍率；在那里缩放会重复缩放天空并错误缩放其它光源。
- Path Tracing Ambient Override 是已有的显式诊断项，仍独立叠加；验收使用其默认黑色。

**可见天空与反射语义**：天空渐变仍来自 `getSkyColor`，背景和镜面中看到的天空保持同一可见外观；天空倍率用于照亮场景，不是曝光或背景调色。镜面中已着色的地形、树叶和 flora 随其实际光照改变。程序云及云阴影现已移除；下方带 `--no-clouds` 的命令是保留的历史记录，该参数现在是兼容性 no-op。现有玻璃离屏不透明表面回退仍是近似环境项 `0.18`，现在也使用权威天空倍率；未把这项近似升级为完整 DDGI/路径追踪。既有装饰性 terrarium 边框/高光不属于本次物理材质重写范围。

## 验证与证据

所有产物在本 worktree `target/summer-evidence/`；所有 app/GPU 启动使用 `flock /tmp/re-flora-summer-gpu.lock`，所有 cargo 使用 `CARGO_BUILD_JOBS=2` 和默认 target。

- `cargo fmt --check`：通过。
- `cargo check`：通过，110 个 Slang shader 编译/生成成功。
- 首次 `cargo test`：记录了新增 uniform 尺寸、模型哈希、Legacy 公式契约的待更新项，已修正。
- `cargo test -- --skip app::core::environment_lighting_test_scene::tests::patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`：主程序 **917 passed / 1 ignored / 1 filtered**，另一个测试二进制 **4 passed**。
- 排除项原始失败为 PATT 存档断言 `left: 4, right: 1`；该场景源文件未修改。完整原始失败保留于 `cargo-test.log`，没有宣称全套无排除通过。
- 新/扩展测试覆盖天空快速连续改值立即推进 transport、保留旧快照、太阳值不随天空改变、三个控制独立保存/加载、旧配置加载并保存显式 0、GUI 值完整冻结到 frame input。
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture scripts.tests.test_validate_ddgi_radiance_lifecycle`：**65 passed**。
- `flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5`：正常退出，`[SHUTDOWN] phase=complete failures=0`。使用同一 worktree `--latest-log` / `--tail-latest-log 200` 核对，runtime_log_diagnostics 无致命错误。见 `hidden-smoke.app.log`、`hidden-smoke.tail.log`、`hidden-smoke.latest-log.txt`。

### 固定场景真实截图

`capture_lighting.py` 逐次加载独立保存值，用 release 二进制执行：

```sh
flock /tmp/re-flora-summer-gpu.lock target/release/re-flora \
  --hidden --mute --no-clouds --no-god-rays --no-lens-flare \
  --foliage-shadow-bench target/summer-evidence/<variant>.toml \
  --foliage-shadow-bench-warmup-frames 300 --foliage-shadow-bench-frames 2 --auto-exit 60
```

固定树种子、相机、60 Hz 动画、1600×1000、dither=0，保存真实 swapchain keyframes。截图不是生成图。稳定名称：

| 文件 | 太阳 | 天空 | 地形 / raster |
| --- | --- | --- | --- |
| `ddgi-original.png` | 2 | 1 | DDGI / DDGI |
| `ddgi-half-sky.png` | 2 | 0.5 | DDGI / DDGI |
| `ddgi-no-sky.png` | 2 | 0 | DDGI / DDGI |
| `ddgi-no-sun.png` | 0 | 0.5 | DDGI / DDGI |
| `path-original.png`, `path-half-sky.png` | 2 | 1 / 0.5 | path reference / DDGI |
| `legacy-original.png`, `legacy-half-sky.png` | 2 | 1 / 0.5 | DDGI / Legacy |

`capture-manifest.json` 保存全部命令、GUI 值、原始图路径、SHA-256 和运行日志。复跑 debug 对比保存在 `capture-debug-manifest.json`，日志明确显示 `ready=true, state=Converging, update_epoch=29, radiance_revision=1, mixed_in_flight=false`；不能把这些 300 帧截图称为最终收敛验收。

`analyze_lighting.py` 复用仓库 PNG 解码器核对原始像素，数据见 `screen-luma-summary.json`：

| 地形路径 | 固定地形窗口，天空 1 → 0.5 | 固定天空窗口 |
| --- | --- | --- |
| DDGI | 111.456 → 98.126 | 逐像素完全相同 |
| path reference | 118.905 → 107.679 | 逐像素完全相同 |
| DDGI + Legacy raster | 111.456 → 98.126 | 逐像素完全相同 |

这是最终 8-bit 截图的平均亮度，窗口坐标记录在 JSON；植物区域会混入叶隙背景，不能当作纯材质能量测量。关闭太阳后保留天空填充光，关闭天空后保留直射和太阳反弹，截图可直接审阅。

另以现有 `--house-scene --screenshot house-overlook ... --screenshot-delay 3 --auto-exit 6` 保存 `house-original.png` / `house-half-sky.png`，用于审阅原版房屋、地形和玻璃场景。此处是本分支已有房屋，不是其它 Worker 的夏日房屋成果。

## 生成输出与集成边界

- `cargo check` 生成 `src/app/generated/gui_adjustables_gen.rs` 和 `src/auto-generated/gpu_structs.rs`。`SunInfo` 复用末尾 padding，尺寸不变；`DdgiRadianceSun` 32 → 48 bytes。
- RFIRR v10 测试 fixture 仅重算 bytes 48..56 的编译天空模型 FNV-1a64，值为 `0xee539346303c0d76`，其余字节逐字相同。见 `fixture-regeneration.txt`。这属于测试夹具同步，不改变捕获格式。
- 运行前保存完整 GUI config，所有截图脚本用 `finally` 恢复；初始 worktree 干净，最终只保留有意的 schema/默认值改动。
- 共享文件为 config、GUI renderer/loader/generated、frame input、tracer/buffer updater、环境权威、DDGI resources/runtime 与必要 shader 入口。`tracer.slang` 只给 sky miss 增加参数；`flora_shadow.slang` 只让 Legacy ambient 接入倍率。没有修细树枝黑像素。

没有启动可见游戏，因此人工拖动/点击 Save 的交互观感尚未手工验收；实时传递由生产数据流与 CPU 状态测试验证，保存由真实临时文件 round-trip 测试验证。没有进行跨 GPU、完整 R13/E2、最终 DDGI 收敛、玻璃全路径或性能预算验收，也不据截图运行时间给出性能结论。用户批准视觉后，性能阶段可独立进行。
