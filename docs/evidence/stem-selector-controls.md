# Flower Stems：单一模式选择器与相关控件

此页取代前几轮报告的 UI 使用说明；原视觉候选和已知 aliasing／性能限制不变。

## 当前入口

**R → Debug → Pixel Sampling — Flower Stems → Flower stems**：

| 模式 | 动态显示的模式参数 |
| --- | --- |
| Original cube stems | 无；原来的 voxel stem pipeline |
| World-direction A | fixed object pixels B 复选框、A direction resolution |
| World-direction B | 同一复选框、B complete-stem object resolution |
| Surface-attached A | block geometry B 复选框；材质格固定为 authored stem edge |
| Surface-attached B | 同一复选框、B geometry cell size |

World-direction／Surface-attached 还显示共用的 radius、test branches、disable wind/rest bend；Original 不显示这些无效实验参数。切换模式／A-B 不重置隐藏字段，下一次切回来恢复各自保存值。说明文字也按当前模式／A-B 更新，而不是一整段混合全部候选的介绍。

## 删除与唯一权威

- 删掉独立的 **Continuous silhouette (reference)** 方法和 `stem-continuous` review 路径。
- 删掉 `flower_stem_experiment` GUI checkbox／生成字段／frame mapping。模式索引固定为 **0=Original、1=World-direction、2=Surface-attached**，不再有两份可能矛盾的 enable／method 保存状态。
- `StemExperiment::enabled()` 从 mode 推导。mode=0 使用原 cube pipeline，不再能启用 mode=0 的解析 continuous reference。
- 删掉 `flower_stem_surface_cell_scale` 的声明、生成字段、分类、frame mapping、Rust policy 和可变 GPU 参数。Surface-attached A 仍需要 `stemSurfaceCell`，故保留逻辑并将 cell size 固定为 authored edge（原默认倍率 1）；B 使用独立的几何格大小。
- GPU float4 ABI 保留，退休的 material multiplier 位置明确 reserved=0，不再读取它。
- 仍保留 Surface-attached 的 A/B；A 的连续几何是该模式的比较基线，不是另一个 dropdown 方法。

GUI relevance 只在 `debug_groups.rs` 的一个纯 visibility mapping 中分类；静态 group 仍拥有全部参数，实际绘制通过既有 SavedControls 绑定。mode selector 先绘制，后续控件随其当前值更新。没有 App-only 值、专用保存分支、第二套控件 ownership 或隐藏的 legacy checkbox。

## 旧保存兼容

加载时、验证与生成字段映射前完成内存迁移，不自动写文件：

- 旧 experimental=false → Original，保留原来实际使用的 renderer；旧 true 且 mode=1/2 保持 World-direction／Surface-attached。
- 旧 continuous mode=0 → Original；新 mode=0 已有原版意义，不暗中保留退休 renderer。
- 退休 checkbox／material slider 从 live schema 和下次保存中移除。
- 更新 retained controls 的标签、条件和 selector options；保留 radius、direction resolution、两套 B 的隐藏偏好等数值。旧文件缺少新字段时从声明补齐。

本工作区旧 true+World-direction 仍为 World-direction，direction=550 和 radius=1.43 原样保留、未提交；新版本默认 Original。用户请求删除的旧 material=1.33 不再以无效字段留在文件里。

## 验证

- `cargo fmt --check`、`cargo check`；`cargo test` **1266 passed / 4 ignored**；Slang **30** 项通过。
- 实际 egui output-shapes 测试覆盖 Original、World A/B、Surface A/B：逐一验证每个控件标签应出现／不出现；在 mode=0 时两套 B 保存为 true，仍不画无关控件。绘制前后完整保存 schema/value 相同。
- 迁移测试覆盖旧 enable=false/true × mode=0/1/2 六种组合，验证 active renderer、退休字段删除、条件／options 更新、保存值保留、加载不写文件、save→reload 幂等。
- 隐藏 muted Release：`validate-stem-sampling.mjs --seconds 70` 三 captures／16 phase；`validate-stem-contract.mjs --seconds 70` 四近远 captures／12 phase；`validate-stem-blocks.mjs --seconds 90` 四 captures／8 phase。实际六花 draw、resize、返回 Original／A、no head rebuild、GUI hash 和无 ERROR/panic/VUID 检查通过。
- Release smoke 通过，并用 `--tail-latest-log 120` 检查同 worktree 日志，shutdown failures=0。没有自动启动可见游戏。

日志：`target/stem-selector-{fmt,check,unit,tests,slang,native,contract,blocks,smoke,run-tail}.log`；图像和阶段数据沿用三个 `target/stem-*-review/` 目录，不提交图片。生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，来自 cargo check。
