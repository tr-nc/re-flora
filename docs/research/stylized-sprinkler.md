# 园艺喷水器：光栅模型与共享像素后处理

## 调研与选择

用户目标：替换方块喷水器，使用现有光栅／像素色彩后处理，做出适合 Re: Flora 的园艺器具，并在真实游戏中完成验证。

一手来源：

- [现有渲染资源](../../src/tracer/sprinkler_resources.rs)、[管线注册](../../src/tracer/pipeline_builder.rs)：旧模型是 3 个立柱方块和 5 个橙色十字头方块，**已经使用 indexed raster draw**，不是需要改成光栅的 Contree 地形。适合替换几何，不需要新增模型缓存、屏幕覆盖层或独立后处理。
- [场景像素采样契约](../scene-pixel-sampling.md)、[场景像素颜色解析](../../shader/slang/scene_pixel_style.slang)：场景统一控制像素尺寸、密集源采样和对比度颜色选择；不能为喷水器另加一套量化或擅自开启用户关闭的全局开关。
- [已有割草机模型](../../src/tracer/mower_resources.rs)：鼠尾草绿、奶油色和暖色小部件已经用于游戏中的园艺工具。采用这个配色家族，而不是写实镀铬或旧款救援橙塑料。
- [Haws 官方园艺水壶说明](https://haws.co.uk/pages/choosing-the-perfect-metal-indoor-can-for-you)：涂漆钢、铜、黄铜和可拆细喷头是实际园艺器具的材料／形态语言。这里借鉴温和的涂漆主体与暖金属接头，不复制品牌、贴图或产品模型。
- [GARDENA 官方圆形喷水器说明](https://www.gardena.com/int/products/watering/sprinklers/comfort-circular-sprinkler-vario/900967401.html)：低位稳定底座、可维护喷头和清楚的出水结构可作为功能性参考；不是照搬它的塑料外观或真实灌溉参数。

美术决定不是外部来源证明的“最佳造型”：本次选择小型、圆润、玩具般的四向园艺喷头，用大色块与现有花草协调，最终观感仍需玩家确认。

## 已实现

- [几何作者](../../src/tracer/sprinkler_mesh.rs)：低矮卷边绿底座、奶油色柱身、黄铜套环／内活塞、圆顶绿分水头、四根铜管、加厚喷嘴唇口／深色内孔、顶部奶油色调节螺钉。截面旋转生成约 1.3k 个三角形，使用真实面法线，不再拼体素。所有实例共享同一个静态网格。
- [硬件尺寸与 CPU 姿态](../../src/sprinkler.rs)：喷嘴高度、伸出长度、升程与运动由同一模块供几何和水滴消费。保留原来的世界 tick、每秒交替一对喷嘴及每实例相位。
- [GPU 动画](../../shader/slang/sprinkler_animation.slang)与 CPU 用相同的明确 half-up 取整规则；[顶点着色器](../../shader/slang/sprinkler.vert.slang)仍复用太阳阴影接收、DDGI 环境光、编辑预览 tint、场景深度与最终后处理。无需额外 render pass、纹理或 descriptor。
- [水滴发射](../../src/app/core/placeables.rs)：水滴从活动喷嘴随升程／伸缩运动的出口产生，不再从远离模型的随机大范围起点出现。弹道初速度使用实际出口高度与相对底座的目标半径求解。
- 真实截图发现旧的一体素水滴遮挡小模型，因此仅将**喷水器**水珠调整到 0.28 体素、柔和青白色和较低透明度。粒子数量、喷射仰角、重力、更新频率及原有小幅漂移保留，其他粒子不改。
- 保留 `X / Spray` 放置、刷除、草生长避让、湿度作用、实体 ID／可选灯光发布和容量限制。没有修改 GUI 配置、存档格式或添加调参按钮。

共享光栅路径写真实场景深度，因此与地形／植物遵循现有遮挡。它仍是 **Raster Consumer**；没有新增 DDGI 遮挡、碰撞体或太阳投影几何，不能把接收阴影宣称成投射阴影。大幅不平整地面上的底座仍可能局部埋入，放置／地形接触规则没有重做。

## 真实视觉迭代

1. 初次截图：大块饱和蓝色水滴盖住新造型，遂只调整喷水器的水珠外观。
2. 平地截图发现测试夹具把地面命中位置向下取整，恰好埋住底座。夹具改为使用经校验的真实地表命中位置；没有更改玩家放置行为。
3. 最终查看 2880×1620 原生游戏截图：近景能看见卷边底座、喷嘴孔、柱身与头部；远景保留绿色底／暖色四向头的辨识度；绕视角和密集／对比度解析模式正常。细水珠仍是现有 billboard 方形粒子，在相机极近处可能变大；不是新流体或连续水柱系统。

截图和机器报告位于本地 `target/sprinkler-review/`：

- `near.png`：默认场景像素设置，近景。
- `garden.png`：与四种已存在的花一起观察较远距离。
- `orbit.png`：实时绕视角。
- `lifecycle.png`：三实例放置／删除全部／重新放置后的实际画面。
- `near-dense.png`：现有 8×8 场景像素、2×2 密集源、对比度颜色解析。
- `summary.json`、对应 `.log`：每个场景至少 180 帧、实际图像尺寸、验证结果和保存文件哈希。

这些是技术正确性与视觉候选的证据，不等于用户已批准外观，也不是最大 256 实例的性能验收。

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

`validate-sprinkler-model.mjs --help` 给出依赖、输出、错误恢复和运行时长控制。它强制启用 Khronos 同步验证，检查实际 layer 插入、零 ERROR／VUID／同步 hazard、正常退出、完整夹具和 PNG，并比较 `config/gui.toml` 与 `config/camera_snapshots.toml` 的前后哈希；不自动打开可见游戏。

本次验证：

- `cargo fmt --check`、`cargo check`、`cargo build --release` 通过。
- 全量 Rust：**1637 app + 4 collision + 1 integration 通过**，无失败；8 app、3 clipboard、1 desktop integration 测试按原策略／显式工具依赖跳过，其中新增的姿态对照另行执行。
- 显式 **300 组 Slang CPU 后端／Rust 姿态对照**通过：多个 tick 长度、半值取整、相位、换向点、循环及 u32 wrap。普通单元测试另覆盖实际发射接口的 5760 颗水滴出口／理想弹道边界，以及网格法线、非退化三角形、边界和动画后唇口对齐。
- **38 个 Slang CPU 测试**全部通过；编译器既有 `#pragma once` 警告不影响结果。
- 普通隐藏静音 Release smoke 通过；五组真实原生场景全部完成 180 帧检查，Vulkan 同步验证零错误，shutdown `failures=0`。
- 保存的 GUI／相机文件未变化；shader-derived Rust 生成文件无差异。
- 没有用单测／调试版宣称性能，也未进行大规模性能验收。

手动体验：运行 `cargo run --release`，按 **X** 后左键放置；无需额外开关。
