# 风与树叶控件定稿（2026-09-14）

## 如何调节

| 目标 | Debug 中的位置 | 含义 |
| --- | --- | --- |
| 树叶声音随风响应 | Wind → Tree Wind Response Min / Max | Min 越高，需要越大的真实风才开始出声；Max 决定达到完整响应的风强 |
| 声音起落速度 | Wind → Wind Audio Attack / Release Decay | 0 慢、1 快；Fast 仍有平滑时间，不是瞬时开关 |
| 声音材质 | Wind → Procedural Tree Sound | 合成声音的干燥感、颗粒、树枝、空气感等，不改变场景风和动画 |
| 树叶原地抖动 | Flora → Leaves → Wind Motion → Local Flutter Strength | 默认 1，范围 0–2；0 关闭局部铰接运动，包含光学转向；需启用惯性响应 |
| 树叶整体偏移 | 同处 → Overall Wind Offset | 默认 1，范围 0–3；0 关闭整体平移，1 保留原幅度，3 为三倍；不缩放局部抖动 |
| 通用植物惯性 | Wind → Vegetation Wind Response | 速度、阻尼、通用风偏转与姿态更新率；影响不止树叶 |
| 场景风来源 | Wind → Background Wind / Wind Item | 唯一的自然背景风，以及玩家释放的局部风；共同进入同一运输场 |

声音总音量仍在 Audio。动画两项与声音参数分离；新动画滑杆使用声明式配置，
Save 保存在 config/gui.toml。原有 Background Wind / Wind Item 临时控件仍标明不保存，
本次未扩大其持久化范围。

## 改动

- ef053e28：删除无效 Base Wind 的声明、CPU 控制存储及合成项；8 个合成控件折叠显示。
  程序生成 clip 时风输入为 1，原 Base Wind 项乘以 (1-wind)，删除不改变游戏声音。
- b3b7c990：删除背景风旧 A 实现、旧噪声/方向摆动参数、A/B checkbox 和 smoke 开关；
  NaturalInflow 成为唯一背景入口。手动风、运输、GPU 风场和音频共用路径不变。
  更新只服务旧模式的测试，保留新自然风的确定性、关背景和手动风测试。
- 树叶专用整体偏移倍率贯穿 GUI → frame input → GPU GuiInput → flora vertex；
  在叠加局部偏移前单独缩放整体偏移，不影响草、果实或声音。
  Flutter 移入 Wind Motion；旧无惯性 paddling 控件明确标为 Legacy/inertia off，
  它不是被删除的旧背景风生成器。
- 更新既有过期 leaf_flutter_contract 测试：改为当前五参数接口，
  固定风应收敛而非依赖已移除的周期驱动；生产扭转求解器未改。

## 验证与边界

- cargo fmt --check、cargo check、cargo test：最终 978 主测试 + 4 库测试，2 ignored。
  包含保存 round-trip、新动画控件仅绘制一次、frame input 数据映射。
- python3 scripts/run_slang_tests.py：11/11 通过。新增测试覆盖整体偏移为 0 时
  局部运动不变、默认比例保持整体位移、两项关闭零位移，以及整体倍率不缩放局部项。
- 每步使用 flock --nonblock --close /tmp/re-flora-summer-gpu.lock
  cargo run --release -- --hidden --mute --auto-exit 0.5，并检查日志：
  - 声音菜单：target/re-flora-logs/re-flora-20260914-001034.788-166049.log
  - 背景风定稿：target/re-flora-logs/re-flora-20260914-001252.715-169070.log
  - 最终动画：target/re-flora-logs/re-flora-20260914-001715.259-173673.log
- 正常退出、shutdown failures=0，无 ERROR；现有多蝴蝶 atlas 警告仍在。
  尚未人工验收新的动画手感、菜单排布或听感；不把 smoke 当性能验收。
- 生成文件由 cargo check 更新：src/app/generated/gui_adjustables_gen.rs、
  src/auto-generated/gpu_structs.rs；没有手改。GuiInput 新增 float 占用原尾部 padding，
  相关 Slang 已重新编译。
- 用户既有音量（Leaves 10、Cicadas 128、Footsteps 1.5）和声音响应
  （Release 0.35、Min 0.25）的未提交修改原样保留，未混入功能提交。
  未启动可见游戏、未 push 游戏分支。
