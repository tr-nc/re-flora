# 三臂旋转洒水器：初版调研与历史实现记录

> 三臂方案已按玩家反馈替换为单喷口。当前行为与验证见
> [单喷口旋转喷流](../sprinkler-spray.md)。下文保留初版调研与当时的验证证据，
> 其中三臂、3×60°、4×12 旋转雾槽位及第四口清零均不是现行契约。

## 一手参考

1. [Orbit Brass 3-Arm Adjustable Sprinkler with Wheel Base 58180](https://www.orbitonline.com/products/brass-3-arm-adjustable-sprinkler-with-wheel-base)。官方说明为三臂旋转、360° 覆盖、可调喷嘴；[官方产品图](https://www.orbitonline.com/cdn/shop/products/58180N_600-2-1_d7d76e40-c789-4dfc-ab8b-f95957d0d814.jpg?v=1741884205)可见黄铜中心轴、三个同向弯曲的细喷臂、深色喷嘴及绿色轮式底座。已查看原图，不从其他商家的参数推断形状。
2. [GARDENA Comfort Circular Sprinkler Mambo](https://www.gardena.com/int/products/watering/sprinklers/comfort-circular-sprinkler-mambo/900979001.html)。官方确认可独立调节喷嘴、360° 配水和稳定的滑橇底座。借鉴低重心和可读的喷头结构，不照搬品牌配色或实际米制喷洒参数。
3. [Hunter MP Rotator](https://www.hunterirrigation.com/irrigation-product/mp-rotator/standard-mp-rotatorr-nozzle) 是另一条多股旋转喷流路线；本轮仅搜索结果可用，详情页抓取为 403，未据此确认内部机构或采用具体外形。

## 选型

采用原创的三臂花瓣式转头，而不是把原四嘴头整体旋转：鼠尾草绿圆矮底座、奶油色立柱、黄铜轴承及三个同向弯曲的铜管，末端有厚喷口和暗色内孔。保留花园工具的柔和材质区域；省略实物的轮子和品牌标志。三臂轮廓能在低分辨率下区分新旧模式。

这是风格化建模与运动参考，不是水压/流体或真实灌溉工程复刻。初始一圈约 4 秒是游戏美术选择，需由玩家试用确认。

## 行为契约

- 新增可保存的运行时 `sprinkler_rotary` A/B checkbox。默认 false：原交替 X/Z 喷头；true：三臂持续绕竖轴旋转。旧存档只补缺字段，已保存的 true/false 保留；不自动覆盖用户尺度、速度或干燥倍率。这不是恢复已退役的土壤 A/B：两种喷头始终使用连片湿斑。
- 模型、水雾历史与实际灌溉共用世界时钟、每实例相位和同一组出口/方向公式。使用既有世界 tick 的小数余量实现连续运动，暂停保留余量；不引入独立计时器或另一个 emitter 注册表。
- 旧模式几何与运动保留。新底座固定、转头刚体旋转；两种模式都接入现有对象级 8–32 N×N surface cache 与原生回退，灯光/深度继续 live，不改变全屏像素化。
- 水雾按出生时的旋转角重建过去三秒，而不是拿当前角度旋转整片历史。保持既有地形约束、体积归一化和总视觉源量；不增加可见水滴。原 ellipsoid A/B 仍是原无方向雾，不能声称它也显示旋转喷流。
- 三个喷口同时工作。灌溉使用三个随头转动的单向扇区，中心仍受角度约束，合法湿斑外缘可跨角边。瞬时总许可角保持 180°（3×60°，对照原 2×90°），原半径/水量倍率不随喷口数放大。
- 视觉密度为零或普通粒子关闭不影响真实浇水。湿斑尺度、浇水速度、干燥 0.2、其他模型采样及花头 1.11 独立保留。

## 实现与所有权

- 设置位于 **Debug → Sprinkler Motion → Sprinklers: rotating three-arm head (A/B)**；[声明配置](../../config/gui.toml)及[旧设置补缺](../../src/app/gui_config_loader.rs)沿用现有 Save 流程。开关作用于现有与新放置的喷水器，不改变实例 ID 或其他采样设置。
- [WorldClock](../../src/game_time.rs)公开已有的 sub-tick fraction；[逐帧快照](../../src/app/core/render_frame_input.rs)将它与 tick 一起上传。[Rust](../../src/sprinkler.rs)／[Slang](../../shader/slang/sprinkler_animation.slang)定义同向弯喷臂的出口与朝向：第 n 个喷口相差 120°，出口方向比其初始径向偏转 -0.65 rad。先对整数 tick 取周期余数，再转 float，避免长时间运行后的浮点时间退化。
- [网格](../../src/tracer/sprinkler_mesh.rs)独立创作三臂模型。[几何源](../../src/tracer/model_geometry.rs)同时提供原模式六个刚体和新模式两个刚体；`range.w` 标记模型与旋转头。各模式的零件共用各自整物体边界，不单独放大杆件的采样网格。新模型采用绕轴不变的包围球；[顶点着色器](../../shader/slang/sprinkler.vert.slang)只选择对应模型，转头以 rigid frame 变换位置和法线，底座不转。缓存与 native 回退共用此变换；切模式不重新注册实例。
- [水雾 prepare](../../shader/slang/sprinkler_mist_prepare.comp.slang)沿用 4×12 的有界槽位。新模式三臂同时工作，每臂源量乘 2/3，第四臂清零，和原两个工作的喷口保持相同总发射源量。每个年龄段使用出生时的角度。任意 yaw 使用 XZ 等面积圆截面：半径取原径向／横向半径的几何平均，保持支持体积及归一化源量，不宣称是旋转椭球或精确流体。地形 sweep、有限支持裁剪、光照、深度及 frame-slot 所有权不变。
- [真实灌溉](../../src/app/core/moisture.rs)仍每实例每帧记录一次 brush，不按三个喷口重复加水。[湿斑中心规则](../../shader/slang/terrain_wetting.slang)在旋转模式使用三个单向 60° 扇区；原模式仍为相反的两个 90° 扇区。`wet_patch.y` 使用现有 push-constant 预留分量选择图案；`x` 仍是必填的相关尺度。保守 broad phase 可以多算候选，但不得裁切有效尾缘。接收者仍受原半径 0.30、径向衰减、吸收、材质与四级湿度上限约束。
- 只由源码和 `cargo check` 更新生成的 GUI／uniform 布局，不手改生成字段。实例仍是原 16-byte pose；没有新 emitter registry 或普通帧同步 GPU 回读。显式 review 的模式检查、土壤和 lobe 读回不进入正常游戏路径。

## 验证结果

- `cargo fmt --check`、`cargo check`、`cargo build --release`；Rust 全套 **1646 app + 4 collider + 1 display integration** 通过，已有 ignore 保留。独立的显式 Slang CPU/Rust 对齐覆盖 **660** 个旧／新姿态案例，包括小数 tick、不同 tick 长度、相位、长 uptime、全部出口与旧零件平移。
- **44 Slang CPU** 测试通过。[旋转契约](../../shader/tests/sprinkler_rotary.slang)覆盖三臂 120°、出生角历史、180° 总扇区、等体积支持、2/4/12/32 尺度下互补扇区的四级 Poisson 分布与跨边湿斑。每尺度 32768 样本；无阈值加水或密度补偿。
- 保存测试覆盖缺字段／显式 false／显式 true，无写入加载与保存再加载，且保留湿斑尺度、浇水速度、干燥、mist A/B 和花头大小。整物体采样测试覆盖 8/16/32；两个新刚体共享同一网格尺度，并验证完整旋转不越过缓存边界。
- 首轮 **14 个 RTX 3060 Ti/X11 隐藏静音 Release 场景**通过，包括新模型近景／俯视、8/16/32、cache/native、开关循环、雾 on/off/原椭球、粒子关闭、resize、放置／移除和两组真实土壤试验。均到 frame 180，无 Khronos 同步验证错误，正常退出。近景及俯视截图已查看；不是离线替代渲染。
- 切换时的实际 GPU lobe 读回：旋转模式第四嘴为 **0** 个 live lobe；切回原模式恢复 **9 / 8** 个，避免只验证 GUI 值。原生／缓存切换的其他四类模型采样设置保持不变。
- 首轮七组真实土壤试验的整圆足迹均值：旋转 **0.11955**，原模式 **0.11431**，差 **0.00524**、配对标准误 **0.00562**；满足既定 `4×SE+0.02` 检查。它是湿度等级单位的均值，不是独立湿体素数量，也不是单张图像逐像素守恒。
- 最终扩展的 **16 个旋转套件场景**也全部通过：新增单体素墙和悬崖。GPU 读回分别为 **36 live lobes、中心越过墙后 0**，以及 **36 live lobes、中心低于悬崖源头 9**；墙跨越整个雾场，不能把绕过有限墙端部误判成穿墙。最终旋转／原模式土壤均值 **0.11845 / 0.11431**，差 **0.00414**、配对标准误 **0.00792**；仍通过既定检查。

- 另跑完整 **15 个旧湿斑 + 6 个蒸发**场景通过，因此最终验证共 **37 个**实际 Release 场景，全部到 frame 180、零 Vulkan 同步验证错误、正常退出。旧模式整圆均值在 2/4/12/24 尺度为 **0.13774 / 0.13350 / 0.11431 / 0.13536**；X/Z 角外累计湿度 **5807 / 2765**，仍有湿斑尾缘跨角边而不越过半径。干燥 0/0.2/1/3 最终均值 **3.00000 / 2.89417 / 2.45400 / 1.44889**；暂停片段字节相同、恢复后继续变干，蒸发为零时真实灌溉仍正常。

- 标准隐藏静音 `--auto-exit 0.5` 成功到首个游戏帧并正常退出；日志无错误。GUI／相机 SHA-256 全程不变；另验证用户 GUI 文件除新增 checkbox 外逐字节相同。验证脚本的 help、互斥套件、非法时长／缺参数／未知参数及恢复提示检查通过，34 个本地 Markdown 目标有效。新 native shader 数量仍为 136，更新的生成字段来自 `cargo check`。

命令：

```sh
cargo fmt --check
cargo check
cargo test
cargo test sprinkler_shader_pose_matches_water_outlets -- --ignored --nocapture
python3 scripts/run_slang_tests.py
cargo build --release
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 \
  node scripts/validate-sprinkler-model.mjs --rotary --seconds 12
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 \
  node scripts/validate-sprinkler-model.mjs --wet --seconds 10
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 \
  node scripts/validate-sprinkler-model.mjs --dry --seconds 10
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 \
  cargo run --release -- --hidden --mute --auto-exit 0.5
```

产物：`target/rotary-sprinkler/` 的构建／Rust／Slang／姿态／保存测试和首轮 `initial-14-summary.json`；`target/rotary-sprinkler-review/` 的真实游戏截图、日志和土壤统计。最终 16 场景以 `summary.json` 为准，不能将未完成或失败的运行计为通过。

## 边界与外观验收

灌溉按每帧当前姿态采样，不是任意大帧间隔下的解析角度扫掠积分；雾使用既有当前风场重建近期历史，不是储存真实水滴的流体模拟。默认约四秒一圈，原 ellipsoid mist A/B 仍无方向，独立关闭 mist 不停止转头或浇水。没有新增硬件阴影投射／碰撞或折射玻璃输运。

此轮不是新性能基准，不接受任意 256 实例近距离重叠；既有极端重叠限制仍有效。正确性通过不替代玩家的旋转速度、轮廓和雾带观感确认。用户保存的尺度 **4.5**、浇水 **0.3**、干燥 **0.2**、花头 **1.11** 保留；提交的常规尺度／浇水默认值仍是 12／1。
