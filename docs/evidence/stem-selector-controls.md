# Flower Stems：单一模式选择器与相关控件

此页取代此前报告的 UI 使用说明。Surface-attached block geometry B 已按用户请求整条删除；既有 aliasing／性能限制不变。

## 当前入口

**R → Debug → Pixel Sampling — Flower Stems → Flower stems**：

| 模式 | 动态显示的模式参数 |
| --- | --- |
| Original cube stems | Original cube stems: voxel edge scale |
| World-direction A | fixed object pixels B 复选框、A direction resolution |
| World-direction B | 同一复选框、B complete-stem object resolution |
| Surface-attached | 连续几何、固定 authored-edge 材质格；没有 block checkbox／cell-size 滑杆 |

World-direction／Surface-attached 还显示共用的 radius、test branches、disable wind/rest bend；Original 不显示这些实验参数。模式／World A-B 切换不重置隐藏字段。

Original 滑杆复用 **Flora.model_flower_voxel_scale** 的声明、生成绑定和统一保存，范围 **0.2–4**、既有默认值 **2** 不变；从 Model Flowers 的 Geometry 移到 Original 模式下面，不复制控件或保存值。authored edge=.05、WORLD_SCALE=10/256；**成熟且整体 size=1 时，scale=2 的边长等于草的 1/256 世界单位**。growth 与整体 size 仍按既有逻辑缩放植株；该参数也改变共同骨架的高度，保持层数分布不变，并非独立改变宽度。没有强制覆盖用户保存的 voxel／size／height 参数。

## 删除与唯一权威

- 模式索引固定为 **0=Original、1=World-direction、2=Surface-attached**；没有独立 experimental checkbox 或 Continuous reference 方法。
- 删除 `flower_stem_surface_geometry`、`flower_stem_geometry_cell_scale` 的声明、生成字段、Rust policy、frame mapping、负值 GPU 编码和几何 culling／proxy padding。
- 删除 block shader、对应 Slang 测试、`stem-blocks*` review 模式与 `validate-stem-blocks.mjs`；不是只隐藏控件。
- Surface-attached 保留 `traceStem` 连续几何、固定 `stemSurfaceCell`、live socket cap／真实深度。没有替换成全局尺寸 block 几何，也没有新抗锯齿 workaround。
- World-direction angular A／fixed-object B 完整保留；float4 ABI 不变，y=reserved 0，w=World B object resolution 或 0。

GUI 通过既有 SavedControls 绘制；Original 参数仍属于 Flora 保存区，但由 Debug concern 唯一呈现。selector 先绘制，相关控件与说明动态更新。

## 旧保存兼容

加载前完成内存迁移，不自动写文件：

- 旧 experimental=false → Original；true 且 mode=1/2 保留模式；旧 continuous mode=0 → Original。
- 退休 experimental、material cell、block geometry、geometry cell-size 四个 ID 从 live schema 和下次保存中移除。旧 Surface B 保存仍选择 Surface-attached，现在只使用连续几何。
- retained labels、条件、selector options 更新，其他参数值不重置；Original voxel ID／值不变。

## 验证

- `cargo fmt --check`、`cargo check`；Rust **1265 passed / 4 ignored**；Slang **29** 项通过。
- egui output-shapes 覆盖 Original、World A/B、Surface：Original voxel label 恰好出现一次且只在 Original 出现；隐藏值与整个保存 schema 不变。
- 六种旧 enable/mode 组合的迁移测试加入退休 block fields，验证加载不写文件、保留 renderer／有效数值、删除退休字段及 save→reload 幂等。
- 纯 Column 测试覆盖六花、voxel=.2/1/2/4 的真实三角形生成，层数不变，默认 nominal 世界 edge 与草一致。
- `validate-original-stem-voxels.mjs --seconds 60`：Original .2/2/4 三 live phase、六花实际 draw、每阶段源 cache 重建、resize、无 ERROR/panic/VUID、GUI 文件不变。此处 voxel 参数属于 Shape，改变它会按既有设计重建源 cache；不是 World A/B 的纯 live-uniform 分辨率设置。
- `validate-stem-sampling.mjs --seconds 70`：三 captures／16 phase；`validate-stem-contract.mjs --seconds 70`：四近远 captures／12 phase，六花、World A/B、head bank 不重建、socket／resize／GUI hash 回归通过。
- hidden muted Release smoke 及 `--tail-latest-log 120` 通过，shutdown failures=0。没有启动可见游戏；不宣称视觉或性能验收。

### 额外严格 cache oracle：已确认基线失败，未放宽检查

首次 Original 扫描额外开启 `RE_FLORA_MODEL_CACHE_REVIEW=1`，在 **voxel=4、height mean=1、variance=0、head=1、32px、256 views** 时失败：whole-source **kind=3/source=105/view=255/pixel=14,15**，depth error **0.009290457**（GPU .8825817、CPU .89187217）。这是 opt-in geometry oracle 的 panic，不是 Vulkan validation 错误。

在临时 detached **eafbb32d** 工作树，只有上述旧参数数值调整，用同样 strict flag 的普通 hidden Release 运行，复现了**完全相同**的 source/view/pixel/depth；该工作树已清理。烘焙 shader／cache／oracle 本次无改动。常规渲染扫描通过，但这个额外严格诊断未通过，不能称其已修复。新脚本的显式 `--cache-review` 保留该失败信号，不改变生产检查或 tolerance；它的 help 标注已知基线失败。

日志：`target/original-stem-{fmt,check,tests,slang,native,selector-native,contract-native,smoke,run-tail}.log`；额外失败：`target/original-stem-oracle-failure.log`、`target/original-stem-baseline-oracle-exact.log`。新截图／数据：`target/original-stem-voxels-review/`。生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，由 cargo check 生成。
