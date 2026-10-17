# 园艺喷水器：共享小物体表面预缓存

## 目标与调研

喷水器使用圆润的园艺工具造型，并像苹果、蝴蝶和花头一样拥有独立的 **N×N 模型采样分辨率**。这不是全屏像素大小，也不只是离散相机角度。

一手来源：

- [共享模型几何](../../src/tracer/model_geometry.rs)、[表面缓存](../../src/tracer/model_surface_cache.rs)：CPU 按视角预烘焙占用、位置、法线和材质；运行时重建小面片，保留实时光照和深度。
- [场景像素采样契约](../scene-pixel-sampling.md)：模型表面采样之后仍经过统一的场景后处理，两层设置互不替代。
- [已有割草机](../../src/tracer/mower_resources.rs)：沿用鼠尾草绿、奶油色和暖色配件的工具配色。
- [Haws 官方材料说明](https://haws.co.uk/pages/choosing-the-perfect-metal-indoor-can-for-you)：涂漆钢、铜、黄铜及可拆喷头提供形态参考，不复制品牌或模型。
- [GARDENA 官方喷水器](https://www.gardena.com/int/products/watering/sprinklers/comfort-circular-sprinkler-vario/900967401.html)：借鉴低位稳定底座和清楚的出水结构，不照搬产品外观或灌溉参数。

造型仍是视觉候选，最终观感需要玩家确认。

## 原先的接线缺口

- **影响范围**：`73c64384` 实现了新模型，但喷水器仍走直接三角形路径，没有苹果／花头那样的对象采样控件。
- **根因**：误把复用全屏后处理当作复用对象级表面预缓存；没有注册缓存 kind，也没有把模型送入共享缓存绘制。
- **引入时间**：新园艺造型提交 `73c64384` 已存在这个范围遗漏；更早的方块喷水器本来也不在对象缓存中。
- **干净修复**：保留造型，接入现有几何、异步缓存、逐帧资源和绘制所有者；只增加喷水器自己的设置，不改场景像素参数。

## 当前实现

- [作者网格](../../src/tracer/sprinkler_mesh.rs)：卷边绿底座、奶油色柱身、黄铜套环、圆顶分水头、铜管、喷嘴内孔及顶部调节螺钉，约 1.3k 个三角形。
- [几何注册](../../src/tracer/model_geometry.rs)：按运动拆成六个刚性部件（固定底座、升降头、四根喷嘴），各自烘焙全部表面，避免把随后会露出的表面永久遮掉。六部分共用整件模型的取景中心、半径和网格尺寸；N 指整件喷水器的采样密度，不是每个小零件各自放大到 N。
- [缓存](../../src/tracer/model_surface_cache.rs)：喷水器是独立第五类，复用方向库和异步任务。修改喷水器分辨率只使这一类缓存失效；缓存未就绪或关闭时走同一模型的原生三角形回退。
- [顶点路径](../../shader/slang/sprinkler.vert.slang)：缓存及回退共享视角选择，部件升降和活动喷嘴伸缩实时计算；部件平移同时作用于缓存表面位置和面片投影。没有烘死动画或新增离线 RGB 贴图。
- [实例发布](../../src/tracer/sprinkler_resources.rs)：放置操作只暂存姿态，等待帧槽 fence 完成后才写对应 GPU 缓冲；逐帧描述符由共享模型绘制路径管理，避免删除／重放时覆盖飞行中的数据。
- [尺寸与姿态](../../src/sprinkler.rs)、[GPU 动画](../../shader/slang/sprinkler_animation.slang)：保留 tick、每实例相位和每秒交替一对喷嘴。水滴的物理出口与网格运动仍共用尺寸及姿态；视角量化和粗采样会带来正常的视觉离散误差，不改变物理浇水范围。
- [水滴](../../src/app/core/placeables.rs)：从活动出口产生，弹道按实际高度／偏移求解，保留上一实现的细小柔和青白水珠及其浇水、放置、删除行为。
- 保留实时照明、阴影接收、编辑预览染色和场景深度。仍是 **Raster Consumer**：没有新增太阳投影、DDGI 遮挡或碰撞体。

## 游戏内控制

Debug → **Model Pixel Pre-cache**：

- **Sprinklers: use model pre-cache**：缓存开关；关闭使用原生三角形。
- **Sprinklers: pre-cache resolution (N x N)**：8–32，默认 32。降低会使喷水器明显粗粒化，细喷嘴可能只剩几个采样格。

设置来自 `config/gui.toml` 的声明式字段，可随 Debug 设置保存；旧配置缺字段时补默认值，读取不写盘。不改苹果、蝴蝶、花头、工具袋的分辨率或 `scene_pixel_ratio`。

## 验证结果与边界

本轮：

- `cargo fmt --check`、`cargo check`、Release 构建通过。
- 全量 Rust：1641 app、4 collision、1 integration 通过；既有显式依赖测试按策略跳过。
- 38 个 Slang CPU 测试通过。
- 显式 300 组 Slang／Rust 姿态对照通过，额外检查每组六个部件的运动平移；普通测试覆盖缓存 8/16/32 的采样、共同网格、材质、源几何保真、配置迁移／保存、模型和全屏设置隔离。
- NVIDIA 硬件运行被另一个游戏的显存占用阻塞，启动分配失败；没有关闭用户游戏，也没有把此运行记为通过。
- 以 **llvmpipe 软件 Vulkan + X11** 运行真正的隐藏／静音 Release 程序：32 采样 smoke、8/16/32 截图、七阶段缓存／分辨率切换、三实例放置／删除／重放。后两组达到 180 帧，Khronos 同步验证无 ERROR／VUID／hazard，退出成功。配置和相机文件哈希不变。
- 软件 Vulkan 的 Wayland 诊断在销毁 surface 时崩溃，调用栈落在 `wsi_wl_surface_destroy → wl_proxy_destroy`；没有把这次失败记为通过，也没有确认其引入时间。X11 是本轮验证环境选择，不是对此问题的产品修复。
- 硬件九场景复验、最大 256 实例性能验收和玩家外观认可尚未完成。软件 Vulkan 的耗时不是硬件性能证据。

本轮截图及日志在 `target/sprinkler-cache-review/`：

- `software-pixels-{8,16,32}.png`：已查看，喷水器采样粗细可见，周围花头使用原有设置。
- `software-{controls,lifecycle}.log`：逐阶段缓存状态、180 帧检查和正常退出。
- `software-x11-smoke.log`、`rust-tests.log`、`slang-tests.log`、`pose-parity.log`：对应验证。

`target/sprinkler-review/` 的旧五场景截图／报告属于 `73c64384` 原生网格验证，不能冒充本轮缓存路径的验收。

## 复验

```sh
cargo fmt --check
cargo check
cargo test
cargo test sprinkler_shader_pose_matches_water_outlets -- --ignored --nocapture
python3 scripts/run_slang_tests.py
cargo build --release
cargo run --release -- --hidden --mute --auto-exit 0.5
node scripts/validate-sprinkler-model.mjs
cargo run --release -- --tail-latest-log 25
```

脚本扩展为九场景：near、garden、orbit、lifecycle、near-dense、pixels-8/16/32、controls。它强制启用 Khronos 同步验证，检查实际缓存绘制／回退、完整夹具、PNG、退出和保存文件哈希；用 `--help` 查看依赖及运行时长恢复方式。

手动体验用 `cargo run --release`，按 **X** 放置，再到上述 Debug 分组调节。不会自动启动可见游戏。
