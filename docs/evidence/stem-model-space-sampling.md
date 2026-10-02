# Flower stem pixelization：模型尺寸采样

## 当前状态：用户批准，作为唯一像素化方式

**R → Debug → Pixel Sampling — Flower Stems → Pixelization** 只保留：

- **Flower stems: model-space pixelization**：是否像素化。
- **Flower stems: samples per stem height**：勾选像素化时显示。

已删除 World-direction 角度采样、A/B 开关、方向分辨率、对应 shader 函数、角度剔除边距及专用验证路径。不是把旧分支藏在 UI 后面。模型采样和连续／分格着色仍独立组合；没有恢复固定角度图像、立方体茎或冻结风动。

旧存档中的 `flower_stem_model_sampling`（无论 true/false）和 `flower_stem_direction_resolution` 都会被移除；`flower_stem_pixelized`、分格着色、已有 `flower_stem_model_resolution` 保持原值。采样分辨率的条件改绑到 `flower_stem_pixelized`。加载不写盘，正常保存时清理旧字段。缺少模型分辨率的老存档补声明默认值。

验证脚本已更新为 5 张截图与 16 阶段 sweep，覆盖四种像素化／着色组合、模型采样远近、环绕、近裁剪、风动和 resize。距离不变采样、连续视角、socket 裁剪、深度与剔除测试均保留，并改为只调用已采用的生产路径。

本次转正验证：`cargo fmt --check`、`cargo check` 通过；`cargo test` 为 4 + 1271 通过、4 ignored；29 项 Slang CPU 测试通过。隐藏 Release smoke 与 `node scripts/validate-stem-sampling.mjs --seconds 35` 均通过，运行日志 `target/re-flora-logs/re-flora-20261002-234725.107-432302.log` 正常退出 `failures=0`，无 ERROR / VUID。保留用户最新的采样数 103、分格着色开启和茎底颜色设置，不把这些个人调参混入模式清理提交。

用户视觉选择不等于大规模 Release 性能验收；远处仍受屏幕亚像素覆盖限制。

---

以下记录此前 A/B 候选阶段，旧 UI／模式和对应复现命令不再是当前接口。

## 历史：入口与效果

**R → Debug → Pixel Sampling — Flower Stems → Pixelization**：

1. 勾选 **Flower stems: pixelized sampling**。
2. **Flower stems: model-space sampling (B; off = world-direction A)** 默认不勾选。
   - A 保留原来的世界方向角度网格及 direction resolution。
   - B 使用植物高度定义格宽，默认 **128 samples per stem height**（32–512）。拉远不会降低模型本身的采样密度。
3. 两者都可与 Shading 下的 surface-attached cells 独立组合；正常风动始终保留。

A/B、两个分辨率均走 declarative config 和统一保存流程。旧存档添加 B=false，保留 A 的数值；隐藏控件不重置值。没有恢复已删除的 Original Cube、冻结风动或离散视图 Fixed Object 模式。

## 为什么不用先前的固定视角图像

用户拒绝的是移动时跨过固定视角边界造成的整株突然换形，而不是模型尺寸采样本身。

B 不调用 `nearestModelView`，没有角度表／缓存视图切换，也不使用整株正交替代图像：

- 网格原点固定在植物的模型中点，格宽是当前模型高度 / resolution，包含生长和整体缩放。
- 网格朝向由当前相机位置和 right 向量连续计算，不按固定角度选择。这里“模型空间”指尺寸与锚点，不声称网格朝向永远固定在茎表面。
- 将真实透视显示射线投到该网格，在格心生成同一相机原点出发的透视采样射线，求交现有连续锥段。
- 连续／分格着色使用同一个命中点；显示深度由命中点所在的平行网格平面与显示射线求交得到。继续执行 socket 裁剪、真实场景深度测试，不覆盖到花头或地形前面。
- 相机进入植物包围球时，格宽连续缩小，趋近连续采样，避免穿过网格原点时的透视奇点。靠近不会额外降低细节；包围球外是严格固定模型单位格宽。近退化边缘朝向也连续缩小格宽，不切换备用离散坐标轴。
- GPU 代理范围增加最多两个基础格宽；CPU chunk 剔除使用匹配的模型尺寸保守边距，而不是随距离增长的角度边距。

这消除了离散 source-view 换图路径，不等于消除一切像素跳动：格心跨过轮廓、风动、材质变化、远处亚像素覆盖仍会引起局部变化。花头仍使用原有花头 renderer，本次不改花头视角策略。

## 验证

- 先运行实际 `stemDirectionCell` 的距离回归：同一模型单位区间，2 与 8 距离的 source sample 数不同，测试退出 1，复现 A 的距离退化。
- `shader/tests/flower_stem_model_sampling_test.slang` 使用生产采样函数：B 在 2 / 8 / 32 距离上同一区间均为 65 个格心；远近对应的模型采样点一致。1440 步完整环绕覆盖世界轴边界，无离散视图／射线跳变。还覆盖内部／近／远相机的有限深度、保守 fringe、两种着色及中心退化。
- `cargo fmt --check`、`cargo check` 通过；`cargo test`：4 + 1271 通过，4 ignored。Rust 测试覆盖 opt-in、范围规范化、CPU 剔除、8 种 UI 组合及隐藏值保持、旧保存兼容、统一保存重载和 frame input。
- `python scripts/run_slang_tests.py`：29 项全部通过。
- `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过，通过本工作区 log helper 检查日志。组合验证最后一次原生日志为 `target/re-flora-logs/re-flora-20261002-231827.191-414730.log`，正常退出 `failures=0`，无 ERROR / VUID。
- `node scripts/validate-stem-sampling.mjs --seconds 35`：8 张截图、两个 16 阶段实时 sweep。包括 A/B 近／4 倍远距离、连续／分格着色、环绕／远近／近裁剪移动、正常风动和 resize。普通阶段所有花种均提交绘制，切换不重建花头缓存，GUI 文件哈希不变，正常退出 `failures=0`，无 ERROR / VUID / validation warning。

产物：`target/stem-sampling-review/summary.json`，重点对比 `stem-direction.png` / `stem-direction-far.png` 与 `stem-model.png` / `stem-model-far.png`。截图用于候选效果审阅，不是逐像素确定性画面对比（正常风动未冻结）。

尚待用户实际移动观察后的视觉批准；没有进行性能验收，也不把 CPU 测试或截图中的 FPS 当性能结论。极远处仍受屏幕像素数限制，本次没有加入新的抗锯齿机制。
