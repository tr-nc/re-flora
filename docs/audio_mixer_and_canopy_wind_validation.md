# Audio 混音控件与树叶风响应

2026-09-13，`agent/butterfly-block-flight`。

## Audio 区域

Debug Panel → Audio → Live mixer：总开关／总倍率，以及 Tree leaves、Cicadas、Footsteps / jump / landing、Digging / terrain editing、Interface / item selection 五类声音的独立开关和倍率。0–8 倍，默认全部开启、1 倍；0 倍静音。倍率是线性振幅（2 倍约 +6 dB），不是主观响度倍数，较大倍率可能造成过响／削波。

已有 dB、音色、声学质量控件保留在 Audio → Advanced audio / source trims。新总控叠加原 master dB，仍尊重 M 键和 `--mute`。本轮不擅自重设用户已有音量参数。

`AudioMixSettings` 属于 `SavedCustomSettings`，自定义 GUI 只通过 SavedControls 编辑同一已序列化对象；Save 无新增逐字段复制分支，旧文件缺失这些字段时兼容默认值。通用配置与所有自定义叶子的 Save/reload 回归继续通过。

通过 PetalSonic 原生子总线控制已播放及后续声音，不重启循环、不修改播放速度、不破坏空间衰减或源增益。统一声音入口要求显式分类；树冠和本地脚步专用入口固定到对应类别。循环源更新音量／替换素材保留总线，缓存短音以 `(path, category)` 为键，避免同素材跨类串控。

实现提交：`bbabd7e7`。

## 树叶声持续与叶片动作

诊断并非仅降低总音量：

1. 树冠素材预先以满风生成循环，但运行时把风响应当成 bool。旧代码 1% 风和满风的发射器增益相同；音色参数 base_wind 还可绕过无风静音条件。
2. 叶片局部扭转使用共享风的强度，却另外加了独立、随墙钟时间运行的正弦激励。恒定风也会周期摆动；草没有这套局部周期激励。并非两者完全读取不同风源，也不能概括成现实中草和树叶必须同幅运动。

修复：循环的振幅乘以连续的实际风响应；生命周期淡入淡出仍按独立功率处理。音色 base_wind 不再给运行时增加一个无风背景音下限。保留既有攻击／释放响应，不突然启停音频 Voice。

叶片删除独立正弦激励，以共享风的压力和投影力矩驱动原机械状态。恒定风收敛为偏转姿态，共享风变化再激起响应；无风回到静止。保留个体几何差异、限幅、姿态发布和光学路径，没有冻结整棵树或改草的响应。

修复提交：`ef4983a6`。

## 验证证据

- `cargo fmt --check`、`cargo check` 通过；最终完整测试 **964 主测试通过、2 ignored，另 4 库测试通过**。
- 新混音测试覆盖中性默认、类别独立、0 倍静音、倍率到 dB 及非法数值处理。持久化全叶子测试自动包含新字段。
- `cargo test quiet_canopy_does_not_play_the_full_wind_loop_at_full_gain -- --nocapture` 在旧门控上明确失败（weak=strong）；修复后通过，1% 风比满风低 40 dB。
- GPU 旧模型恒定风稳态摆幅 `0.57414347` rad，新增真实 shader 回归明确失败。新模型为 `0.0000042766333` rad；零风后角度约 `1e-8`，峰值 `0.21398`，个体差异 `0.01772`。个体差异断言从旧振荡器的相位差指标改为 >0.01 rad 的几何／机械差异指标；稳态收敛约束另行保留，未用放宽稳定性断言来通过。
- 树冠实机遥测 776 样本，音量符合 `base + 10log10(lifecycle_power) + 20log10(wind_response)`，最大误差约 `0.00000171` dB。风响应 `0.09375`、生命周期功率 `0.576011` 时，基础 36 dB 对应实际源增益 `13.043732` dB，不再忽略风强。

所有 app 验证持有 `flock --close /tmp/re-flora-summer-gpu.lock`，使用 release、hidden、mute，没有启动可见窗口：

| 参数 | 本 worktree `target/re-flora-logs/` 日志 |
| --- | --- |
| `--hidden --mute --auto-exit 0.5`（mixer） | `re-flora-20260913-185204.568-51707.log` |
| `--hidden --mute --canopy-audio-diagnostic --auto-exit 10` | `re-flora-20260913-190125.303-56744.log` |
| `RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1`，`--hidden --mute --auto-exit 8` | `re-flora-20260913-190215.681-56963.log` |

上述有效验证正常退出、shutdown failures=0；预期失败的 red 运行不算验收。最终 GPU 回归同时通过 held pose、lifetime remap 和快速反向检查。生成文件无变化。

## 边界

没有进行新版本耳机听感或主观动作验收，未声称已调出最终混音比例；也未完成蝉鸣提前以 habitat_lost 结束的独立原因诊断。本轮实现了用户要求的可调混音，并修复树叶两处已复现的风响应问题。

用户刚保存的 `config/gui.toml` 中 DartingSprite 改动保留未提交；源代码与报告单独提交。不 merge/push、不管理其他 Worker。改动集中于 audio mixer／统一声音入口、GUI 已保存模型、树冠音量响应及 leaf torsion shader 和回归测试。
