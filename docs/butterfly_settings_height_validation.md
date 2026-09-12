# 蝴蝶设置保存、飞行高度与滑杆反馈闭环

2026-09-13，`agent/butterfly-block-flight`；基于已完成的主分支合并 `dd2dde88`。

## 结果与提交

- `e890757e`：移除 Reset B flight controls；蝴蝶设置接入 DebugSettings 的统一保存／加载路径；恢复用户最后调参。
- `b4b02607`：B 改为地形相对高度吸引、无偏上下机动；解决树冠体素锁死；以可见效果明确的高度滑杆替换 Horizontal maneuver tempo。
- Debug panel → Butterflies 保留实时 A/B checkbox，默认勾选 B；不勾选仍是原素材和原 A 飞行动作。

## 保存根因及恢复的数据

不是所有 Debug Settings 的 Save 都失效。通用参数和自定义树设置原本能保存；蝴蝶 A/B 与滑杆另存于 App 临时字段，没有进入 GuiConfigFile，因此成功提示并不意味着它们被写入。
现在 live state 由 DebugSettings 唯一持有，保存同步、启动加载与 GUI 操作使用同一份状态；没有新增第二套配置或写文件路径。其他明确标注为临时的调试实验不在此次持久化范围。

原始用户日志：`target/re-flora-logs/re-flora-20260913-012124.834-563609.log`。
最后调参为 01:24:03.936，点击保存为 01:24:25.438。恢复到 `config/gui.toml`：

| 参数 | 保存值 |
| --- | ---: |
| B 模式 | 开 |
| Shared flight frequency | 5.5 Hz |
| Self-flight speed | 0.35 |
| Vertical movement | 2 |
| Turn sharpness | 1 |
| Wind drift | 1 |
| 原 Horizontal maneuver tempo | 2.75，保留配置兼容，不再显示滑杆 |
| 新 Flight height above ground | 0.08，默认玩家眼高；范围 0.03–0.24 |

旧配置缺少蝴蝶节时仍可加载，使用旧版默认；本仓库启动采用上述明确保存值。没有纳入无关 GUI 漂移。

## 高度与弱滑杆的根因

1. 原栖息中心永久继承出生高度，树叶出生的个体自然围绕树冠飞；上下恢复又有 ±0.20 世界单位死区，远大于默认玩家眼高 0.08。
2. 原上下抽样有正的期望冲量，长时间偏向上边界。平地旧模型 60 秒测试后半段平均离地 0.1717，超过预期近眼高区间；新对称抽样消除系统性上偏，但仍保留不规则短促机动。
3. 初版地形相对修复在平面测试通过，实机却失败：向下 CPU 射线命中木材 5（樱木），并且部分叶出生点在枝干体素里，扫掠距离零把整个速度锁死。没有把这个中间版本算通过。
4. B 现在按共享节拍查询当前位置下方表面，以弹簧／阻尼软吸引回到该表面以上的目标高度，仍使用有界加速度、jerk、速度和积分，不瞬移；出生、数量、颜色、寿命、水平栖息地和风的权威不变。
5. B 视觉粒子把樱木／橡木树冠视为可穿透，避免树枝被当作地面，也允许枝内出生者离开。泥土、岩石及非木材建筑表面仍参与碰撞。材质过滤加入 CPU 射线可选入口，其他调用保持全部实体，A／玩家／音频语义不改。
6. 原 tempo 同时缩短机动持续时间和等待间隔，受平滑与限幅后并非明显、单调的视觉控制。8 个固定种子各 60 秒，tempo 从 0.25 到 4，事件数 301→4440，但平均角路程仅 1.392→1.594 rad/s，平均水平速度反而 0.0679→0.0544。已去掉此滑杆，换成直接控制空间位置的离地高度；原值 2.75 仍保存。

这是游戏表现模型，不是对所有真实蝴蝶行为的生物学定律描述。

## 验证证据

- 保存回归曾红：`butterfly_flight_controls_survive_debug_settings_save_and_reload` 显示 Save 后频率字段丢失；修复后通过。含文件 round-trip、live state、A/B、旧配置与 GUI shape／指针操作测试。
- 长飞、斜坡、实时高度切换、无偏上下采样、材质过滤同 chunk／跨 chunk／枝内起点回归通过；高度切换验证连续积分而非位置跳变。
- `cargo fmt --check`、`cargo check`、`cargo test`：**953 + 4 通过，2 ignored**。
- `cargo test -p re-flora-physics`：**44 通过**。
- `python3 scripts/run_slang_tests.py`：**10/10**；粒子顶点反射测试 **1 通过**。
- release 构建通过；保留 6 项已有编译 warning。生成文件无变化，未手改生成输出；没有改落叶着色、上传布局或 shader ABI。
- 全部 GPU 运行使用 `flock --close /tmp/re-flora-summer-gpu.lock`，hidden、mute，无可见启动。
- 保存修复 smoke：`re-flora-20260913-013256.960-570243.log`，启动正确加载 5.5／2.75／2，正常退出。
- 高度实机：`re-flora-20260913-015003.037-578957.log`，约 85 秒，4000 帧 warmup + 360 帧采集。frame≥2400、不透明度≥0.99 的 928 个高度样本均值 **0.0913**，**96.98%** 在 0.03–0.15；27585 个对应逐帧速度样本零速度数 **0**。高度最大值 0.494 包括新出生、尚在下降的个体，不能把统计说成每只永远贴眼高。
- 截图：`target/butterfly-review/terrain-height-fixed.artifacts-926Vxg/`，已检查 `frame-0000.png`、`frame-0003.png`、`frame-0030.png`。彩色方块在树干周围较低区域移动；序列每 3 帧一张，运动日志逐帧记录。
- A→B→A 实机：`re-flora-20260913-015148.746-579731.log`，frame 806 切 B、926 切 A，正常退出；额外 B→A smoke 也通过。所有最终运行 `failures=0`，无 ERROR、panic 或 Vulkan validation error。
- CPU 日志：`target/butterfly-height-final-{fmt,check,tests,physics,abi,slang,build}.log`。中间失败与对照记录保留在 `target/butterfly-height-red.log`、`target/butterfly-tempo-baseline.log`、`target/butterfly-height-surface-probe.log`。

## 边界与改动文件

未替用户完成主观动态手感验收，也没有做性能前后对照。真实截图／日志不等同人工试玩通过。材质过滤尚不区分天然枝干与玩家搭建的同材质木平台，B 也可穿过后者；其他实体碰撞和 A 模式不受影响。新出生个体会从树冠逐渐下降，保留出生分布，不会立即刷到玩家脸旁。极端地形、密集木建筑及全部参数组合未穷举。

实现文件：`config/gui.toml`、`src/app/core/{mod,particles}.rs`、`src/app/gui_config.rs`、`src/app/gui_config_model.rs`、新增 `src/app/gui_config/butterfly_flight.rs`、`src/particles/{mod,butterfly_flight}.rs`、`src/builder/contree/mod.rs`。报告及历史报告入口另行提交。没有 push、额外 merge、操作其他 Worker 或覆盖无关用户配置。
