# 白石小屋独立场景交接

小屋通过 `--cozy-home` 启动，兼容原来的 `--house-scene`。这是可进入、可种植和可编辑的真实游戏场景；默认花园不会自动生成小屋。按控制器收窄后的要求，仅交付 CLI 入口，没有第二进程、窗口暂停、轮询或场景管理扩展。

玻璃仍使用原有隔离实验的 Sand ID 3 编码。场景禁止加载/保存地形，保留既有存档边界；两扇圆窗仍使用两体素厚的玻璃，并保留碰撞、透射和局部光照语义。

## 玩家入口与真实截图

从当前 worktree 操作，构建统一使用 `CARGO_BUILD_JOBS=2`，游戏运行使用共享 GPU 锁。以下可见运行命令仅供玩家主动试用，本次 Worker 没有执行可见启动：

```sh
CARGO_BUILD_JOBS=2 cargo build --release
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run -- --cozy-home
```

到达镜头处于行走模式，WASD 可沿台阶穿过敞开的圆拱入口。以下是已执行、可复现的隐藏截图命令：

```sh
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --cozy-home --hidden --mute --screenshot cozy-home-exterior target/summer-evidence/exterior-v2.png --screenshot-delay 8 --auto-exit 10
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --cozy-home --hidden --mute --screenshot cozy-home-interior target/summer-evidence/interior-v2.png --screenshot-delay 8 --auto-exit 10
```

- 外景：[exterior-v2.png](../target/summer-evidence/exterior-v2.png)，1600×1000；已自行查看，显示白石、木拱门、双窗、开花攀藤和草花屋顶。
- 内景：[interior-v2.png](../target/summer-evidence/interior-v2.png)，1600×1000；已自行查看，显示两盏暖灯、桌子、两张长凳和木地板。
- 同目录 `exterior-v1.png` / `interior-v1.png` 是第一轮真实截图，保留了构图和外观修正过程。HTML 报告应使用 v2。
- 自动小屋截图隐藏 HUD；普通游玩保留游戏 UI。截图未经绘制或图像生成工具加工。

## 实现与语义

- 重建为四面厚墙、分缝浅色石块、三角山墙、覆土坡屋顶、橡木檐口和圆拱入口，替换旧的土丘壳体。
- 独立 Limestone / Ivy / Petal 体素材质占用原四位类型中的 9 / 10 / 11，普通 Rock、Stucco 与全局 GUI 配色不变。两条颜色查询路径同时覆盖直接着色和 DDGI；新材质保留不透明遮挡、实体碰撞和地形支撑语义。
- 藤蔓包含木质细茎、带尖叶片和五瓣花，经过现有地形事务及发布路径，属于静态体素装饰。屋顶短草采用原有 occupancy 种植路径，78 株花卉通过生产 authored flora 路径植入，保留生长和风动能力。
- 木灯架包围真实 Emissive 灯芯。现有体素光源 provider 从已发布地形发现灯芯，生成局部光源；没有新增悬空点光源或绕开遮挡的室内补光。
- 编辑统计覆盖 0–11 类型；新材料完整进入背包回收、重新放置与收获粒子颜色链。未收集的新材料不扩展默认花园的空背包列表。
- 独立场景选择晨间 `time_of_day=0.36`，让正立面接受日照，只改变本次运行的 authored lighting。

## 验证与证据

执行了：

```sh
CARGO_BUILD_JOBS=2 cargo fmt --check
CARGO_BUILD_JOBS=2 cargo check
CARGO_BUILD_JOBS=2 cargo test
CARGO_BUILD_JOBS=2 cargo test -- --skip patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --hidden --mute --auto-exit 0.5
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --cozy-home --hidden --mute --auto-exit 0.5
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --latest-log
flock /tmp/re-flora-summer-gpu.lock env CARGO_BUILD_JOBS=2 cargo run --release -- --tail-latest-log 200
```

`fmt`、`check` 和两种 release smoke 通过。全量测试遇到既有 PATT 相机夹具失败：它要求整个相机库只有 1 项，而基线 `29cbeff9` 已有 4 项。以基线相机文件复跑相同未修改的测试，得到 `left: 4, right: 1`；证据是 `target/summer-evidence/baseline-patt-test.log`。排除该项后 917 passed、0 failed、1 ignored、1 filtered out。没有修改 PATT 测试或它的场景。

真实生产路径证据位于 `target/summer-evidence/final-house-smoke.console.log`：

- `[HOUSE_SCENE] base_y=116`，完整房间与 `glass_panes=2`。
- `[HOUSE_SCENE][ROOF] living_grass=true`；`[GARDEN] authored_flowers=78 substrate_validated=true`。
- `[WINDOWS] glass_collision_panes=2 optical_mode=GlassExperiment`。
- `[WALK] production_capsule_translation=Vec3(0, -0.0006092428, -0.21875), grounded=true`，验证穿过门洞。
- `[APPROACH] from_z=420 steps=120 final_voxels≈(173.529,136.534,307.615)`：真实玩家胶囊从屋外地面出发，经台阶进入室内，未修改玩家实际位置。
- 两盏灯的 provider 各发现 72 个发光体素；`[LOCAL_LIGHT][LIVE] count=2 capacity=8 overflow_count=0`。
- `[GLASS][RESOURCES] enabled=true`，未启动隔离玻璃测试场景；默认 smoke 对应 `enabled=false`。
- 所有完成的截图及 smoke 都以 `[SHUTDOWN] phase=complete failures=0` 结束。

测试及构建输出在 `target/summer-evidence/cargo-*.log`。当前 worktree 的原生日志目录是 `target/re-flora-logs`；已使用内建 `--latest-log` 和 `--tail-latest-log` 读取。`config/gui.toml` 在运行前备份于 `gui.before.toml`，最终恢复并按字节验证。

## 文件与集成边界

- `src/app/core/house_scene.rs`：实体设计、屋顶种植与生产碰撞检查。
- `src/app/core/loading.rs`：加载完成后接入小屋种植。
- `src/app/core/mod.rs`：两处小屋高度状态初始化，以及小屋自动截图的 HUD 门控。
- `src/cli.rs`、`config/camera_snapshots.toml`：独立入口别名与三个镜头预设。
- `shader/slang/voxel_types.slang`、`shader/slang/tracer_material.slang`：三个独立材质及直接/DDGI 调色。
- `shader/slang/chunk_writer_types.slang`、`src/builder/plain/mod.rs`：12 类型编辑统计和删除限额。
- `src/app/core/voxel_backpack.rs`、`src/app/core/particles.rs`：新材料回收及收获粒子。
- `src/auto-generated/gpu_structs.rs`：由 `cargo check` 生成；仅 EditStats 的两个数组由 9 扩至 12。没有手工修改生成文件。
- 本文档。

共享文件均只作以上必要修改，由控制器负责与其它 Worker 的语义集成。没有改动其它 worktree、全局配置，也没有 push、tag 或 release。

## 尚未覆盖

尚未进行玩家可见的手动游玩、手工拆墙重建的 UI 验收和性能验收；截图运行的 FPS 不作为性能结论。藤蔓为静态地形装饰，没有独立藤蔓生长模拟。玻璃隔离场景仍不可持久化。普通编译保留了旧土丘 helper 在当前场景替换后不再使用的告警，没有为消除告警删除其它能力。
