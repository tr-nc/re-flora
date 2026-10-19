# 单喷口旋转喷流

本轮替换 `acfe9476` 的三臂方案，保留原有 `sprinkler_rotary` 设置 ID、保存路径和运行时 A/B。
**Debug → Sprinkler Motion → Sprinklers: rotating single-nozzle head (A/B)**：
勾选为单喷口持续旋转，取消为原交替 X/Z 双向喷水。`Sprinkler Mist` 的方向雾开关仍独立；
取消它会回到无方向的旧椭球雾，不能用来检查单向喷流。

## 现象、原因与引入点

- **影响范围**：三臂旋转模式的方向水雾沿轨迹呈离散团块；不是普通水滴粒子开关的问题。
  原交替模式和无方向旧雾不是本次形状修改的目标。
- **根因**：旋转模式沿用每喷口 12 个等间隔年龄样本，再把轴向支持换成 XZ 圆截面。
  邻接年龄的空间间距既包含向前运动，也包含出生时喷口方向的变化；旧半径没有覆盖后者。
  圆胖的初始支持又掩盖了喷口应当很细、沿程展开的形状。单纯放大雾团无法解决形态问题。
- **引入点**：`acfe9476`（2026-10-19，`add rotating three-arm sprinkler mode`）。
  修改前对真实生产函数组成的自由空气轨迹采样：0.25/0.5/0.75 秒处，样本间密度只有邻居均值的
  0.0026/0.0737/0.0216；1.0–1.75 秒处为零。基线截图与诊断输出在
  `target/sprinkler-shape-review/`。这证明离散采样造成缺口，不以启动成功代替形状检查。

## 设计与不变量

- **一个物理出口**：[CPU pose](../src/sprinkler.rs)、[Slang pose](../shader/slang/sprinkler_animation.slang)
  都移除旋转模式的喷口索引；[网格](../src/tracer/sprinkler_mesh.rs)只保留一根弯臂及其喷嘴。
  底座固定、头部刚体旋转，缓存／原生回退仍共用同一姿态。周期仍约四秒。
- **连续喷流而非团块**：[sprinkler_spray](../shader/slang/sprinkler_spray.slang)统一拥有生产／测试使用的
  出生时间分区、出口、初速度、光学量、轨迹切线和支持形状。沿用每实例 48 个槽位，不增加 emitter。
  单出口使用全部槽位，年龄二次分布增加喷口附近精度。椭球长轴沿当前喷流的年龄切线，支持长度随
  相邻年龄的空间间距变化；横向／竖向宽度独立从喷口内径展开，不靠把整个雾团变胖来填洞。
- **更接近喷水而不是烟**：向前／向上初速度、阻力与下落组成弧线；出口附近较粗的水对应较低
  光学散射量，下游细雾逐渐增加。旋转后的历史水仍按出生方向飞行。碰到地形即沉积、结束该样本，
  不再让旧水沿地面滑成一圈烟。未实现独立逐滴流体或高频亮点粒子，仍是风格化连续体积喷雾。
- **正确的体积／遮挡**：光学量按年龄区间宽度与椭球体积归一化，不随细分数量倍增。
  光线转入正交局部坐标后做精确线积分，深度和地形裁剪的射线参数不变；tile bounds 与近地形查询
  使用旋转支持的保守 AABB。所有槽位逐帧写入，包括被地形终止的零密度样本，切模式不留下旧雾。
- **土壤也只向前浇**：[wetting](../shader/slang/terrain_wetting.slang)使用一个单向 60° 湿斑中心扇区，
  不保留隐藏的另两个浇水方向。把原 180° 输入集中到 60°（扇区内剂量乘三），保持完整一圈的
  预期输入；半径、湿斑尾缘、吸收、四级湿度上限与用户倍率不变。这不是任意大帧间隔的角度积分。
- **保存与兼容**：只更名已有声明式 checkbox，无新 App-only 设置、无新 save hook。
  用户 `config/gui.toml` 的湿斑尺度 4.5、浇水速度 0.3、干燥 0.2 等原样保留，不纳入提交。
  生成布局来自 `cargo check`：lobe 增加正交朝向与模式标记，由 48 变为 64 bytes；实例仍 16 bytes。

## 回归入口

```sh
cargo fmt --check
cargo check
cargo test
cargo test sprinkler_shader_pose_matches_water_outlets -- --ignored --nocapture
python3 scripts/run_slang_tests.py
cargo build --release
node scripts/validate-sprinkler-model.mjs --rotary --seconds 8
cargo run --release -- --hidden --mute --auto-exit 0.5
```

[连续性测试](../shader/tests/sprinkler_spray_continuity.slang)直接调用生产 source／shape 函数，
在 48 槽位、12 个时间／相位／风／tick 长度组合上检查喷口接续、沿程展开、密度谷值、下落、
细分光学量、正交坐标变换、积分及保守边界。与旧场景同类的样本间密度谷值约 **0.9415**，
高于邻居均值的 60% 守卫，旧方案最低为零。
[单口契约](../shader/tests/sprinkler_rotary.slang)验证仅一个活跃源、60° 开口及旋转一圈的剂量守恒、
背后无水、2/4/12/32 尺度的湿斑尾缘和 Poisson 分布。Rust 网格测试检查只有一根臂、旋转后的
喷口边缘包围真实出口；CPU/Slang 660 个姿态案例继续通过。

隐藏 Release 视觉检查可使用 `RE_FLORA_SPRINKLER_REVIEW=rotary-mist`；
另有 `rotary-mist-back`／`rotary-mist-front`／`rotary-mist-top` 用于顺逆光与俯视。
与现有 fixture 一样不保存 GUI、相机或地形；截图用于外观审阅，不是性能基准。

## 最终验证

- `cargo fmt --check`、`cargo check`、`git diff --check`、最终 `cargo build --release` 通过。
  Rust 全套 **1647 app + 4 collider + 1 integration** 通过；45 项 Slang CPU 和 660 个跨语言姿态案例通过。
- 16 个旋转套件隐藏静音 Release 场景全部到 frame 180，Khronos 同步验证零错误；覆盖
  模型／缓存／原生回退、8/16/32、实时开关、方向雾 on/off/旧模式、粒子关闭、resize、放置／移除、
  墙、悬崖与真实土壤。墙后中心数为 0；悬崖 36 个活跃样本中有 10 个低于源头。
  运行时切换读回全部活跃样本的模式标记正确，无遗留三嘴或旧朝向。
- 七组真实土壤试验的整圆湿度均值为 **0.13691 / 0.11431**（单口／原模式），差 0.02260、
  配对标准误 0.00959，低于既定容差 0.05834。它是随机四级土壤状态统计，不是单图逐像素守恒。
- 另跑顺光／逆光／俯视三个序列，各 4 张截图，均到 frame 180、正常退出、无错误日志。
  已查看模型近景、常规视角、逆光和俯视序列：喷口有细喷流、沿程展开，未见原来的断团和整圈地面雾；
  逆光可见度明显较强，顺光／俯视较淡。截图在 `target/sprinkler-shape-review/`，
  套件统计在 `target/rotary-sprinkler-review/summary.json`。
- 标准 `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过，使用同 worktree 的
  `--latest-log`／`--tail-latest-log` 检查正常退出及 `failures=0`，完整日志无 ERROR/panic/VUID。
  最终 smoke 日志：`target/re-flora-logs/re-flora-20261020-010726.626-36126.log`。
- GUI／相机文件在套件中哈希不变；另验证用户 GUI 文件除 checkbox 文案外逐字节保留。
  没有启动可见游戏。生成文件变更仅为上述 lobe ABI 与 checkbox 文案。

视觉是否达到玩家心中的真实感仍需玩家确认；本轮不宣称精确流体、大量实例的性能验收或逐水滴模拟。
