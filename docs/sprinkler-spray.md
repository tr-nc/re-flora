# 单喷口：共享喷流与落地浇水

**Debug → Sprinkler Spray & Motion**。勾选
`Sprinklers: rotating single-nozzle head (A/B)` 使用单口旋转模式；取消仍是原交替 X/Z 模式。

三个新增滑杆均为声明式、可保存设置，旧配置只补缺项，不覆盖已有值：

| 控件 | 范围 | 默认 | 行为 |
| --- | --- | --- | --- |
| Spray angle (degrees) | 10–90° | 40° | 横向全开角；同时改变可见支持和落地分布。原固定宽度约等于 20° |
| Rotation speed (RPM) | 0–60 | 15 | 15 RPM 为四秒一圈；0 停转但可继续喷水；改速不跳转喷头当前朝向 |
| Pulse duty (%) | 0–100% | 50% | 每秒四次开关；50% 约开 125 ms／关 125 ms；100% 连续；0% 停喷但不停止旋转 |

`Mist & Scattering` 的 density 只影响能否看见水雾，不控制浇水。
方向雾开关仍独立：关闭时显示旧的无方向椭球雾，**它是视觉对照，不是共享喷流的形状预览**。
`Soil Wetting` 的 wetting speed 控制入水量，wet patch scale 控制四级湿度随机量化的空间相关性，
drying speed 控制后续蒸发；它们不再为旋转模式定义另一套浇水半径或扇区。

## 确认的两次问题

### 原三臂雾团断裂（历史修复 d6369a78）

`acfe9476` 的旋转模式每喷口只有 12 个等间隔年龄样本，XZ 圆形支持忽略出生方向变化造成的
样本间距，部分年龄的样本间密度为零。单纯放大雾团会掩盖喷口应细、沿程展开的形态。
修复为一个真实喷口、48 个二次年龄分区、沿喷流年龄切线定向的有限支持；碰地结束，不沿地面
滑成烟圈。基线与视觉证据保留在 `target/sprinkler-shape-review/`。

### 可见喷雾与实际土壤不同模型（本次修复）

`d6369a78` 的空中水按历史方向、阻力和下落飞行，但土壤仍调用当前方向的独立 60° 扇区和
固定 0.30 半径。这不是只缺一个角度偏移：喷幅、转速、风、地形和飞行时间均可能改变落点。

调用当时生产函数的复现 `target/sprinkler-coupling-review/alignment-before.slang` 显示：
平地首次接触约 **0.7755 秒**，距离 **0.3062**，64 个湿斑种子在可见落点的旧浇水覆盖均为 **0**。
该基线程序对应旧提交；新代码已删除旧旋转扇区 API，不能在新版本上用它代替新回归。

## 共享模型与所有权

1. [sprinkler_spray.slang](../shader/slang/sprinkler_spray.slang)拥有年龄分区、阀门积分、
   出生角、出口／发射速度、拖曳／下落、喷幅、沿轨迹支持和散射权重。喷头网格和水源使用
   [sprinkler_animation.slang](../shader/slang/sprinkler_animation.slang)同一个出口与朝向。
   [SprayClock](../src/sprinkler.rs)积分世界时钟增量，维护有界旋转／脉冲相位；暂停不补转，
   tick 回绕不跳变，改 RPM 不以新速度重算整段运行时长。
2. [prepare](../shader/slang/sprinkler_mist_prepare.comp.slang)每实例仍使用 48 个年龄区间，
   一次求轨迹与真实体素地形的首次相交。接触落在某个区间内时，同时发布其空中可见部分和
   `SprinklerWaterContact`。光学消费者和土壤消费者读同一份结果；没有第二个 emitter registry。
   整个 prepare 在地形绘制之前运行，即使 density 为零或选择旧雾外观也不能关闭实际浇水。
3. [contact](../shader/slang/sprinkler_spray_contact.slang)将**同一归一化有限支持**投影到接触面。
   复用精确核线积分，平面上的面积积分为一；加宽会摊薄单位面积水量，不会凭空增加总水量。
   age-bin 条件只选择当前到达边界，不把整条空中喷流竖直投影为湿区。
4. [wetting](../shader/slang/sprinkler_wetting.comp.slang)从接触数据计算每个土壤体素的入水密度，
   检查地形遮挡，仅写首次接触表层。后续深入土壤由原有垂直迁移机制负责。
   独立的旧旋转扇区、三倍剂量补偿和 CPU 当前轴适配已删除；原交替模式和手动 Water 保持原路径。
   固定 `INPUT_AREA` 只校准源的总入水量，不限定空间范围。
5. 保留原四级湿度、相关 Poisson 量化、湿斑尺度、材质与状态位。一个体素只有一个 invocation，
   实例间依次 dispatch；不用对 R8 atlas 做不支持的原子操作，也不让重叠喷头并发丢失更新。
   接触／边界缓冲区和雾缓冲区一样归 acquired frame slot 所有，resize 和移除不会借用在途资源。

在 15 RPM 时，约 0.78 秒的飞行会产生约 **70°** 的出生方向滞后，不能按当前喷头轴浇水。
空中的弯曲喷流和逐时落点自然来自这个时间关系。**空中呈螺旋状并不意味着其整个投影立即湿透**：
只有接触土壤的部分加水。平地连续转很多圈后，累计湿痕趋于环带，这是落地累积而非另一张圆形贴图。

## 近似边界与成本

这是有界的解析密度场，不是逐滴流体模拟。沿用 48 个年龄样本，在可变地形上用有界短段 sweep
求中心轨迹接触，再投影有限宽度；不声称覆盖每个微小水滴各自不同的碰撞时间。
固定控制值下，水按出生时方向飞行，周期性开关沿年龄传播。

**运行时改喷幅、RPM 或占空比会重新求当前场，包括已在空中的部分**，而不是为每滴水保留旧参数。
例如将占空比设为零会立即清掉当前水场。喷头当前角度仍连续，两个消费者仍使用同一个新场。
喷头基座目前是静态放置物；尚未实现搬运中的逐滴源位置历史。极低帧率下的整帧空间积分、
更精细的首次接触和跨调参历史，可在共享 source/contact 边界继续改进，不能再另拟合土壤扇区。

新接触结构为 64 bytes，实例仍为 16 bytes、雾 lobe 仍为 64 bytes；生成文件由 `cargo check`
更新。水量写入仅遍历接触 AABB，并跳过无接触区间，避免在整个喷射半径立方体里运行另一套水模型。
本轮优先验证形态与耦合，不以 debug 测试或带同步验证的 GPU 时间宣称大量喷头性能验收。

## 验证与复现

```sh
cargo fmt --check
cargo check
cargo test
python3 scripts/run_slang_tests.py
cargo test sprinkler_shader_pose_matches_water_outlets -- --ignored --nocapture
cargo build --release
node scripts/validate-sprinkler-model.mjs --spray --seconds 8
cargo run --release -- --hidden --mute --auto-exit 0.5
```

- [共享接触契约](../shader/tests/sprinkler_contact_contracts.slang)：0／15／60 RPM ×
  10／40／90° 的真实飞行落点都有正入水密度；无关源头下方及空气／深层土壤为零。
  接触核面积积分最坏误差 **0.000013**；风导致的落点变化符合相同传输公式。
  0／10／50／90／100% 阀门在 30／60／120 Hz 下验证输入；50% 测得 0.4985–0.5000，
  0% 与 100% 精确。不同年龄、不同当前角度下同一次出生的发射方向一致。
- [连续性回归](../shader/tests/sprinkler_spray_continuity.slang)：默认加宽连续模式的密度谷值
  **0.9531**，高于 0.6 守卫。间歇模式有意保留阀门关闭，不把空隙强行接成连续雾。
- Rust 全套 **1649 app + 4 collider + 1 integration** 通过；46 项 Slang CPU 测试和
  显式运行的 **660** 个 CPU／Slang 姿态案例通过。
  新控件通过统一实际保存／重载测试，另测缺字段迁移、保留保存值及不在加载时写盘。
- `--spray` 九个真实隐藏静音 Release 场景通过，全部到 frame 180，Vulkan 同步验证零错误。
  覆盖连续／50%／0%、density=0、实时 10–90°／0–60 RPM／0–100% 调参、四面薄墙、
  60 RPM、resize、重叠放置／移除。GPU 接触读回与实际土壤字节检查：关阀／墙挡时零湿体素；
  density=0 仍有真实湿土；调参关阀期间土壤字节不变，空中 lobe 和接触同时为零。
  日志与截图：`target/sprinkler-coupling-review/`，套件结果见 `summary.json`。
- 另跑原方向雾 `mist-flow`、旋转悬崖 `rotary-cliff`、原浇水 `wet-12` 三个 Release 场景，
  均到 frame 180、正常退出、日志无错误。悬崖有 10 个活跃中心落到源头下方；原模式七次
  土壤试验整足迹均值 0.114311。标准隐藏静音 smoke 也通过，最终日志：
  `target/re-flora-logs/re-flora-20261020-022418.797-65186.log`，`failures=0`。
- `cargo fmt --check`、`cargo check`、`git diff --check`、最终 Release 构建通过。
- 用户已有 GUI 值保留；套件核对 GUI／相机哈希不变。新增三项之外没有默认值修改。
  没有自动启动可见游戏。逆光喷幅截图为 `back-wide.png`，间歇序列为 `pulse-sequence.*.png`。

视觉真实感仍需玩家确认；前／俯视比逆光淡，不能把这些正确性结果当成最终美术或人口规模性能验收。
