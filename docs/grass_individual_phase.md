# 逐株相位与 Wind 菜单（2026-09-16）

## 根因与修复

按 diagnosing-bugs 流程建立红/绿回归：调用实际渲染采样函数，在相同网格状态下
传入两株不同的 seed。旧采样直接返回 torsion.xy，两个结果完全相同，测试退出码 1。
调用处的 seed 本来就是各株根坐标的稳定哈希，LOD、完整模型和光照缓存一致；
丢失逐株差异发生在局部运动采样，而非 world tick、频率滑杆或种子生成处。

复现命令：

```sh
slangc shader/tests/grass_individual_phase.slang -std 2025 -I shader/slang -target executable -o /tmp/grass-individual-phase-test
/tmp/grass-individual-phase-test
```

修复后该命令退出 0。邻株轨迹差异、重复采样确定性和 uint 时钟绕回连续性也受测试保护。

- 网格/实例共享的是平滑的风强包络及积分频率时钟，不再共享最后算出的局部位移。
- 渲染时，各株稳定 seed 给两条连续噪声轨迹提供独立相位偏移和序列。
  四角网格采样先以该株 seed 求值，再插值；不能先插值得到一坨共享的局部位移。
- 同一株所有体素、两个 LOD 和光照缓存使用同一采样函数；不是逐顶点杂乱抖动。
- 包络由阻尼响应更新，局部位移用连续、限幅的程序轨迹表达，不再宣称它是每株独立求解的
  二阶机械振子。模型高度/质量分布仍通过已有频率系数生效；参数、根部权重和数量不变。
- 原整体顺风弯曲仍允许空间相关，这是同一阵风的共同作用；独立化的是局部颤动。
- 无新增植株状态表/生成器，状态与 push constant ABI 不变。

## 菜单

Wind 下只有 Generation 和 Response 两个一级子菜单：

- Generation：已有持续风、手动 Wind Item 等生成设置。
- Response：共享机械设置、草振幅、草频率、叶振幅、叶频率、树声响应。
  四张曲线直接显示，不再各自套折叠菜单。
- 无惯性渲染分支仍实际使用原振动/摆动，因此保留代码和保存参数，
  将 Legacy 改成 Direct grass vibration / Direct leaf motion (inertia off)，
  仅在惯性关闭时直接显示，仍不加子菜单。

config/gui.toml 无差异，用户保存的参数完整保留。没有新增保存接口或临时 GUI 副本。

## 验证与范围

- cargo fmt --check、cargo check、cargo test：991 主测试 + 4 库测试通过，2 ignored。
- 16 项 Slang CPU 测试通过，包括真实逐株采样、恒风持续运动、静风衰减、
  120/240 Hz 一致性及零 dt 调频连续性。
- 持锁 release hidden muted，带全植物验证和 authored-flora-bench：
  target/re-flora-logs/re-flora-20260916-011949.506-62720.log。
  四模型 GPU 包络和时钟正常；全物种两级 LOD、移除/重种/重建/开关生命周期通过，
  failures=0，无 ERROR/panic/VUID。原有 atlas warning 和诊断物理 hitch 保留。
- /tmp/grass-individual-phase-scene.png 已检查；截图只证明绘制，不代替连续动作美观验收。
  逐株差异由使用实际 shader 采样函数的数值轨迹测试证明；性能尚未独立验收。
- 改动范围：grass_sway_sample、vegetation_response_sample/solver、
  对应 CPU/GPU 验证，以及 gui_config、debug_groups、flora_groups。
  没有生成文件差异；音频本地 override 仅用于试玩/隐藏验证，Cargo.lock 最终无差异。
  未 merge、push、发布、管理其他工作树或自动打开可见游戏。
