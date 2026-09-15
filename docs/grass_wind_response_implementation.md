# 草与叶片风响应：实现与验收（2026-09-16）

## 原因与选择

原惯性路径只把风场平均推力送入阻尼振子，并绕过旧的 grass vibration。
恒定输入因此必然趋于静态平衡。研究见 [一手研究与边界](research/grass-wind-response.md)：
野外持续平均风仍有湍流；植物固有频率很重要，但不能把所有植物都认定为风越大频率越高。
据此保留平均偏转，额外用连续、有界、带积分时钟的细尺度激励驱动局部状态。
这代表风场未解析的扰动，是艺术模型，不是新增风场或逐顶点随机位移。

- 四类地表植物共用一套振幅和频率曲线，包括高草、短草、Lavender 和 Ember Bloom。
- 沿用原有根部高度权重：根部零位移、顶部位移更大，不整体平移模型。
- 复用已有响应状态的 torsion/clock 槽位；特殊植物沿用 lifetime ID 状态。
  普通高草与短草各使用响应网格，果实仍固定为 field 1，脱落交接不变。
  未改植物生成、分布、数量或 world tick。
- 局部响应每帧读取连续状态；原整体偏转仍保留离散姿态节奏。
- canonical 非 LOD 体素提供模型高度、等密度模态质量和根截面惯性代理。
  重复位置去重；顶部体素按弯曲模态平方加权。相对频率使用刚度/质量代理，
  再压缩差异供艺术表现。不是材料标定，不推断真实密度、弹性模量或个体生长后的材料性质。
  当前四模型系数依次约为 0.4643、1、0.3842、0.1387；无品种专属滑杆。
  图表表示共用的频率上界，较慢模型在该上界内自然减速。

## 调节位置

Debug → Wind → Grass Response / Leaf Response，各包含：

- Amplitude Response：Scaling 为 voxel 位移包络上限；曲线控制风强对应的包络。
- Frequency Response：Scaling 为 Hz 图表上限，设 1 就显示最大 1 Hz。
  拖蓝色端点调整起止风强与响应值，拖中间金色点调整曲线形状。

草的频率曲线默认水平：先体现模型差异，不强迫风越大频率越高。
可拖成上升、下降或饱和形状。草静态造型仍在 Flora → Ground Plants → Rest Shape；
旧振动仅收纳在明确标注 inertia off 的分类里。共用机械设置和声音风响应仍在 Wind。

频率迁移把旧 multiplier × 24 转为显式 Hz ceiling，端点坐标保持旧存储形式，
渲染上传时除以 24，所以此前叶子曲线的实际 Hz 不变。
以 7fa03834 为基线逐项比较：所有无关参数和音频配置完全相同。
新增参数均走声明式保存；四种曲线适配器都有真实 egui 拖动及 Save/load 测试。

## 验证

- cargo fmt --check、cargo check；完整 cargo test：989 主测试 + 4 库测试通过，2 ignored。
- python3 scripts/run_slang_tests.py：15 项通过。
- 覆盖恒定非零风持续运动、静风衰减、包络有界、120/240 Hz 步长差异、
  零时间重调频率连续性、较高/顶部更重模型响应更慢、重复体素不重复计重。
- CPU/GPU ABI：响应状态仍 112 字节，输入仍 32；push constants 80 → 128 字节，
  新增三组 grass 曲线向量，大小和偏移测试通过。
- 持锁 release hidden muted + RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1：
  四类植物使用真实模型系数，在恒风后半段仍有非零运动范围；
  叶片频率/生命周期/重映射回归通过。日志：
  target/re-flora-logs/re-flora-20260916-004743.618-50522.log。
- 持锁 --authored-flora-bench：四种植物两级 LOD 实际绘制通过，
  重建、移除、重种、开关惯性、可见性和 LOD 生命周期回放通过，state_resets=1。
  日志 target/re-flora-logs/re-flora-20260916-004830.922-50851.log。
  5120×2880 草与特殊植物场景截图 /tmp/grass-response-scene.png 已检查。
- 上述运行正常退出、failures=0，无 ERROR/panic/VUID。
  保留已有多蝴蝶 atlas warning；诊断运行有物理 hitch warning，不作为性能结论。
  截图证明绘制，时序运动由数值轨迹验证；主观美观及性能预算未验收。
- 另做无诊断开关的持锁 release hidden muted 0.5 秒 smoke，通过：
  target/re-flora-logs/re-flora-20260916-004947.815-52863.log，failures=0。
- 实机继续使用本地 PetalSonic override 保留此前爆音修复；最终 cargo check
  恢复 registry lockfile，不提交依赖变化、不发布音频库。

生成文件仅由构建再生：src/app/generated/gui_adjustables_gen.rs、
src/auto-generated/gpu_structs.rs。共享文件重叠涉及 tracer/mod.rs、
tracer/resources.rs 与 GUI，但没有改 ParticleInstanceGpu、粒子上传或落叶光学。
未自动打开可见游戏，未 merge/push 或管理其他工作树。
