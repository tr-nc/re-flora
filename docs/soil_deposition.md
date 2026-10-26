# 渐进铺土与堆高

## 职责与范围

沉积模块只负责地表沉积，不负责购买流程、土壤改良或视觉粒子的落地结算。后续 Soil 工具已增加独立的世界土袋实例和简单落土视觉 FX，见 [土袋原生验证](evidence/soil-bag-native-tool.md)。

- Soil 工具决定有限投放量和分布。
- 沉积模块决定当前支撑面上的覆盖与局部堆积。
- 只增加新的 Dirt 体素，不替换旧地形，不改写旧土含水等属性。
- 继续操作可以不断加厚；横向范围不是最终高度上限。世界和可编辑工作区边界仍然有效。
- 原 Edit／Dig、Till 等工具保持原行为；Smooth 工具已移除。

研究背景见 [调研笔记](research/soil_sprinkling_and_granular_deposition.md)。初期用户明确要求把铺土与改良分开，推迟空中撒落表现；后续授权的简单 FX 不参与沉积和库存。

## 试用

从本工作树运行可见 Release（不要带 `--hidden`；普通试用不要带 `--perf`）：

```bash
cargo run --release -- --rooftop-poc
```

- 按 **B** 或点击工具栏 **Soil**。鼠标悬停不显示土袋；实际成功撒土且继续按住时才显示固定向下倾倒姿态，松开直接隐藏，不新增动画。土袋是工具持有的世界模型实例，使用共享像素受光并向普通地形投射太阳阴影，不是贴屏／编辑色覆盖或新增可掉落／保存的背包物品。空背包/拒绝释放不假装倾倒。旧固定模型屋顶地板本身不接世界阴影，观察投影可用普通花园或已有体素屋顶模式。
- 土袋世界尺寸已缩到原来一半（scale 0.05）。正在释放时发布新地形、等待新GPU悬停，袋子短暂保留确认的展示锚点，避免每批闪隐；确认miss、UI接管/换工具或250ms超时清理。展示锚点不用于编辑、库存或土量规划。
- 左键按下即按保存的频率连续释放，**没有点击／长按区分或 200 ms 等待**。默认20批／秒：按下时、50 ms、100 ms……各释放一批；松开即停止。每批数量由滑杆调节，每个体素仍独立沉积。
- 默认：**512 体素／批、20 批／秒（连续阶段约 10240 体素／秒）、散布半径 0、土团半径 0、堆坡斜率 1**。一批可以有多个原子单位，不是把它们合成一个大土团。无法合法落定时不新增、不扣库存；颗粒可滑到邻近支撑处，不保证强制停在鼠标指定格。
- 滚轮：调整 Soil 自己的可保存散布半径，初始为 0、每格 1 体素；不改变 Edit 的笔刷半径。同样时间投放量不随范围增大。
- **3** 切回原 Edit／Dig，可继续用原几何编辑方法比较或移除土。
- Debug 的 **Soil Deposition** 集中放置五个可保存参数：**Soil batch size (individual voxels)**（1–1024，默认512）、**Soil release frequency (batches / second)**（1–120，默认20）、散布半径（0–128 体素）、土团半径（0–4）、堆坡斜率；另有保存的性能 A/B 复选框 **Soil: faster sampling and settling (A/B)**（默认不勾选，原版；勾选为候选）。每批50、每秒20批即约1000体素／秒，不依赖渲染帧率。可选择宽散布或土团，但不再作为默认。

**Material Supply** GUI 分组和开关已移除。`src/app/core/material_supply.rs` 的代码常量 `INFINITE_MATERIALS = true` 决定当前构建始终使用无限供应，普通花园和屋顶都适用；旧 GUI 保存值不再生效。当前构建中空背包即可使用 Soil／Edit，铺土、放置、挖掘均不改动真实背包；右侧无限材料面板可以选择 Edit 材料，Soil 始终投放 Dirt。无限的是材料供应，不是出料速度或几何范围：正常支撑、工作区／世界边界、连通性处理仍然有效。

如需有限材料版本，将该代码常量改为 `false` 后重新构建（不是游戏内切换）：Soil 使用收集的 Dirt，并按实际写入量扣除；Edit 沿用第一个有库存的材料，挖掘正常收集。没有 Dirt 时不会凭空铺土，也不会为背包制造假库存。屋顶 proof 本身仍不保存地形。

本次没有自动启动可见游戏。

## 实现边界

- `src/terrain_deposition.rs`：独立的有限体积规划与时间预算；不依赖玩家输入、购买或 FX。
- `src/builder/plain/mod.rs::deposit_soil`：读取权威 atlas，在 CPU 规划沉积；若规划触及未读的合法空间，扩展读区并用同一个种子从原 atlas 重算，丢弃试算写入。最终裁剪实际改变范围，一次上传，再标记 solid-workgroup 数据过期。
- `src/app/core/soil_deposition.rs`：工具、库存、可编辑范围及统一 Visible Terrain Publication 的连接。
- `src/app/core/material_supply.rs`：统一供应开关与 Edit 材料选择，独立于场景几何和真实库存。
- `src/app/core/player_tools.rs`：Soil 独立语义动作、按键与释放／切换时的路径和预算清理。
- `config/gui.toml`：声明式保存参数；`src/app/generated/gui_adjustables_gen.rs` 由 `cargo check` 生成，没有手工修改。

默认逐个投放独立体素；非零散布使用软边的平面落点分布，非零土团半径才启用空间相关的小团。不是在三维包围盒里随机塞点，也不填满球／胶囊。移动笔划只分配这份有限材料，不因笔划长度额外制造土量。每个接受的材料单元只写一个空体素。原版读取完整合法 Y；性能候选从合法底部采到源高度上方两格，触及未读顶部则扩高并从原 atlas 重放，不把采样上界当成物理高度上限，因此两种模式都可以在新土表面继续累加。

局部列记录选中支撑上方的空气区间，避免把洞穴外表面当成唯一高度图。坡度判断保留颗粒的原始高度与累计平面欧氏距离；候选下坡高度差按单位／对角步长（1／√2）归一化，不能每走一个整数格就重启坡度比较而退化成方形度量。每个选择的下坡步骤严格降低整数列高，保证终止；32 体素只作初始读区 halo，不是滑落步数、距离或堆脚宽度上限。已有土不被搬走，也不做整个世界的持续滑塌。

新增体素必须有下方支撑。硬质基底／明确模型支撑面可承接第一粒；继续叠在 Dirt 上时，下一层的 X、Z 两轴都须至少有一个实体肩部，防止随机单粒长成孤立的 2–4 格针柱。不满足时继续向低处找合法位置，仍不满足则拒绝这粒，不用删土、隐藏土或额外补土修正外观。原有洞穴与上盖仍保留，不宣称把整个世界的空腔填平。

按下立即投放配置的一批，并从按下时开始按实际经过时间计算后续批次，不设置点击／长按等待。默认20批／秒时，下批在50 ms到期；不足一个配置周期不会重复，松开即停止。使用整数纳秒累计保留不足一个批次的余量，避免逐帧浮点误差少发一批。改变批量从下一次出料生效；每批内所有体素分别经过支撑、坡度、上盖与空格检查。释放、失去合法目标、切换工具会断开路径和预算。长时间卡顿后的补发限定为 250 ms，单批也有安全上限。低帧率时可能在同一帧发出多个独立体素，不是把它们变成一个团。

库存以成功 atlas 写入量扣除，在下游可见发布之前记账，不因发布错误留下已写土却未扣库存的第二份材料。未能落在合法支撑处的尝试不扣除库存。

新土仍是现有 atlas 的 Dirt 字节，不引入第二份土层存档或新的存档格式；没有额外的营养／质量数值。本次没有新增普通花园保存加载的端到端测试。

## 验证记录（2026-10-07）

通过：

- `cargo fmt --check`
- `cargo check`
- `cargo test terrain_deposition`：11 passed，约 0.12 s；覆盖精确体积、持续堆高、旧材质／含水状态、洞穴与固定上盖、单层平台浮点落点、世界边界、小剂量、可重复性和时间预算。
- `cargo test player_tools`：13 passed；包括 Soil 独立左键动作、释放重置和原 Shovel 右键动作不变。
- `cargo build --release`
- `cargo run --release -- --hidden --mute --auto-exit 0.5`
- 既有屋顶 GPU／输入验证，扩展 Soil 的沉积验证并抓图：

```bash
env -u WAYLAND_DISPLAY RE_FLORA_ROOFTOP_VALIDATE=1 \
  RUST_LOG='warn,re_flora::app::core::rooftop_scene=info,re_flora::builder::plain=info,re_flora::run_summary=info,re_flora::run_log_binding=info' \
  flock --close /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --rooftop-poc \
  --screenshot rooftop-scene target/soil-deposition-rooftop.png \
  --screenshot-delay 0.75 --auto-exit 2
```

真实 atlas 验证日志：

```text
[SOIL_DEPOSITION][CHECK] first=128 total=4224 thin_y=218 thick_y=228
exact_volume=true old_material_preserved=true floor_preserved=true
height_uncapped=true publication=true production_lmb=true backpack_preserved=true
```

屋顶支撑面为 y=216，笔刷半径约 7.68 体素；累计后的顶面达到 y=228，超过原笔刷纵向范围。原 Edit／Dig、模型／真实体素切换、Grow 与 capsule 验证也通过。该 fixture 验证的是不限材料屋顶；初次交付时尚未独立验证普通背包的 GPU 扣除路径，后续验证见下节。

最终日志已检查，无 ERROR、panic 或 VUID，正常退出：

- `target/re-flora-logs/re-flora-20261007-011320.938-1904285.log`
- `target/re-flora-logs/re-flora-20261007-011323.893-1904353.log`

普通启动有既有 fruit-physics hitch 警告；本次没有处理这个无关问题。截图 `target/soil-deposition-rooftop.png` 与裁剪图 `target/soil-deposition-closeup.png` 已查看；它们是本地验证产物，不是玩家视觉批准。

### 全量测试的基线问题

`cargo test` 首轮结果为 1399 passed、6 failed、5 ignored。一个新洞穴测试的失败已修正：封顶列仍可以向同一空气区间中的邻列分配土，但不能穿过上盖；随后新增沉积测试全部通过。

另外五个失败已在独立的、未改代码的 `c52ed36b` 基线 checkout 逐项复现，断言和值与当前一致，没有更改相关用户默认设置：

- `saved_control_adapter_uses_type_specific_dimensions_and_one_source_spec`：0.0318 / 0.035。
- `stone_search_checkbox_edit_save_and_reload_are_the_real_saved_fields`：stone preview 默认开启。
- `glass_unrefracted_raster_fallback_defaults_off`：当前默认开启。
- `main_saves_gain_lifecycle_controls_without_changing_authored_display`：420 / 120。
- `older_saves_get_disabled_supersampling_and_saved_choices_survive_reload`：1 / 0。

基线 checkout 已删除。最终修正后复跑了沉积和工具测试，不宣称全量测试全绿。

### Infinite Materials 后续验证

格式、`cargo check`、Release 构建通过；库存、沉积、工具、通用设置保存和 Debug 分组共 **37 个相关测试通过**。未重新宣称全量测试全绿，以上既有默认值基线问题未改动。

普通隐藏启动、正常花园供应切换，以及屋顶既有 GPU／生产输入验证均正常退出，日志无 ERROR、panic 或 VUID。正常花园验证使用真实地形 atlas，显式诊断入口只在设置环境变量时运行：

```bash
env -u WAYLAND_DISPLAY RE_FLORA_MATERIAL_SUPPLY_VALIDATE=1 \
  RUST_LOG='warn,re_flora::app::core::material_supply=info,re_flora::builder::plain=info,re_flora::run_summary=info,re_flora::run_log_binding=info' \
  flock --close /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --auto-exit 0.5
```

从空背包开始：有限模式添加 0；无限模式添加 16 且库存不变；切回有限后仍为 0，不残留虚假库存；诊断专用 7 个 Dirt 的有限剂量请求 16、实际添加和扣除 7。测试结束恢复原开关值与空背包。该入口会修改本次诊断场景中的地形，不是正常启动动作或性能证据。

日志：

- `target/re-flora-logs/re-flora-20261007-013038.890-1913049.log`：普通隐藏启动。
- `target/re-flora-logs/re-flora-20261007-013041.835-1913118.log`：`[MATERIAL_SUPPLY][CHECK] empty_finite=0 infinite_added=16 finite_added=7 toggle_back=true real_inventory_preserved=true shared_edit_supply=true normal_geometry_bounds=true`。
- `target/re-flora-logs/re-flora-20261007-013219.764-1913386.log`：屋顶原 Edit／Dig、材料放置、A/B、capsule 与 Soil 生产 LMB 验证仍通过。

没有自动启动可见游戏。普通花园保存／加载端到端验证与持续交互性能验收仍未完成。

### 细颗粒、圆整度与致密性后续验证

本次实现提交：`fd374729`（细颗粒、圆整、支撑约束），`6afd5c1d`（自适应读区／无人工堆脚上限）。[HTML 报告](reports/soil-deposition.html) 内嵌当前 Release 截图、真实体素数据的截面图和核验结果；[补充调研](research/soil_deposition_roundness_and_compaction.md) 说明依据与证据边界。

- 当前沉积回归：**20 passed**，另 1 个显式报告导出器默认 ignored（已单独运行通过）。包括逐次单粒支撑、内部竖向断层、每层封闭孔洞、孤立土柱、单击／短按／持有、时间采样独立性、超出初始 halo 的大堆、扩区重放与完整规划逐字节一致，以及既有的材质、洞穴、上盖、边界、数量守恒。
- 单点 16384 个体素、散布／土团半径 0：两个高度截面在 16 个方向的最大／最小宽度为 **1.051 / 1.094**，保持 `< 1.15` 门槛。相同旧复现参数（散布 2、土团 1）为 **1.071 / 1.077**；旧复现约 **1.333**，不是靠渲染掩盖或换大土团。
- 随机散布 8、土团 0、刻意陡峭斜率 2：4096 次单粒输入，检查各粒叠加的双轴肩部；最终每列断层、各高度层封闭孔洞、无肩部尖柱均为 **0**。
- 真实屋顶 atlas：128 + 8×512 恰为 **4224**；新体素下方支撑逐粒检查 4224 个，生产 LMB 恰加 **1**、100 ms 内不重复，库存与旧材质不变。
- 独立的空白屋顶图形诊断：四批 16384 恰为 **65536**，读区从 66 扩到 132；堆脚宽 **78**（超过旧 64 宽限），最高 y=254（底面 y=216），新增堆体竖向空缺 **0**。截图数量加速，仅用于展示沉积结果，不代表默认 20／秒的实时速度。
- `cargo fmt --check`、`cargo check`、`cargo build --release`、正常隐藏启动、普通花园空／无限／有限供应切换及屋顶 GPU 验证通过。生成 GUI 代码来自正常构建。

最终运行日志：

- `target/re-flora-logs/re-flora-20261007-041702.902-1979422.log`：正常隐藏启动。
- `target/re-flora-logs/re-flora-20261007-041705.847-1979490.log`：普通花园 0／16／7 实际库存／供应检查。
- `target/re-flora-logs/re-flora-20261007-041314.622-1978467.log`：单粒生产输入、4224 支撑检查、65536 纯沉积、扩区及截图。

日志无 ERROR、panic 或 VUID；正常启动仍有既有 fruit-physics hitch 警告。最终全量测试主程序为 1411 passed、5 failed、6 ignored，另有 collision_bench 4 passed；失败仍为上节列出的 5 项已复现的设置默认值基线问题，不宣称全绿。没有自动启动可见游戏，没有获得玩家视觉批准。

### 每批体素数后续调整

用户澄清：一个体素是材料添加的原子单位，不是每次更新必须仅加一个。现增加声明式保存滑杆 `soil_deposition_batch_size`（1–512，默认10），原 `soil_deposition_rate` 保持 ID、改为批次／秒语义（1–120，默认20）。连续出料量为两者乘积；散布／结团／堆坡算法不变，有限库存仍按实际写入量扣除。单帧可以补发多个批次，时间上限250ms和单帧体积安全上限仍有效，超过安全上限的积压丢弃而不是之后追发。

验证：

- 批量测试先复现失败（请求10，旧逻辑实际1），再通过1／10／50／512的点击与连续出料、不同时间采样、运行时批量切换、非法值、极端预算限流与不积压测试。
- 沉积21、工具13、库存6、通用保存1、新滑杆无保存钩子1、Debug分组6，共 **48 个相关测试通过**；圆整、广基底、内部孔洞、上盖、数量守恒与自适应读区回归仍通过。
- `cargo fmt --check`、`cargo check`、Release构建通过；生成GUI文件只由正常构建更新。
- 真实屋顶生产LMB分别设批量 **1／10／50**：点击增量与250ms持有增量均准确，100ms内不重复，真实背包不变，原Edit／Grow／A/B仍通过。
- 正常隐藏启动、普通花园0／16／7有限与无限供应验证正常退出，运行日志无ERROR／panic／VUID；既有fruit-physics hitch警告未处理。

日志：

- `target/re-flora-logs/re-flora-20261007-123125.941-2027219.log`：`batch_sizes=1,10,50 click_counts_exact=true hold_counts_exact=true tap_no_repeat=true`。
- `target/re-flora-logs/re-flora-20261007-123321.175-2027490.log`：正常隐藏启动。
- `target/re-flora-logs/re-flora-20261007-123324.208-2027557.log`：普通花园真实库存供应。

没有自动启动可见游戏；本次是出料数量／频率调整，不是持续渲染性能优化或验收。未重跑已知有5项无关基线失败的全量测试。

### 批量滑杆上限与默认值

按用户手动体验后的选择，`soil_deposition_batch_size` 默认值从10改为 **512**，滑杆上限从512翻倍为 **1024**，下限仍为1。频率、逐体素沉积规则与单帧体积安全上限不变。

- 保存回归先验证旧范围1–512失败，再验证默认512、范围1–1024以及1024实际保存／重载通过。
- 出料与采样率独立性回归增加512／1024；沉积21、工具13、滑杆保存1、通用保存1，共 **36项通过**。
- Release屋顶生产LMB验证1／10／50／512／1024：点击与持有增量准确、100ms短按不重复、背包不变。日志：`target/re-flora-logs/re-flora-20261007-130013.343-2038392.log`，正常退出，无ERROR／panic／VUID。
- `cargo fmt --check`、`cargo check`、Release构建通过，另跑普通花园隐藏静音启动并检查日志。未自动启动可见游戏，未重跑此前有5项已知基线失败的全量测试；不声称最大出料率已通过持续性能验收。
- 只提交已确认的批量默认值／上限；用户本地散布13及浮点格式保存变化保留，不作为新的项目默认值提交。

### 出料从按下时连续开始

按用户体验反馈移除旧的200 ms点击／长按区分。批次预算从按下时间开始：默认20批／秒在0／50／100／150……ms释放，松开、失去合法目标或切换工具仍清理连续性；频率、每批体素数、分数余量与250 ms卡顿补发上限不变。两种性能 A/B 模式使用同一出料时钟。

- 回归先复现首个50 ms周期仍出料0的旧行为，再验证1／20／120批每秒、批量1／10／50／512／1024的前两次周期；边界前不抢发，同一时刻不重发。
- 沉积24、工具13，共37项通过；一秒预算／不同采样率／运行时批量切换同步更新，保持逐体素与库存规则。
- 真实屋顶生产LMB检查0／49／50／100／250 ms：49 ms不追加、50 ms追加1批、100 ms再1批、250 ms补3批；覆盖批量1／10／50／512／1024，不制造背包库存。出料增多使实际hover落点越出旧80格诊断裁剪区，计数改为完整合法工作区，不缩减物理范围或放宽数量断言。
- `cargo fmt --check`、`cargo check`、Release构建与普通隐藏静音启动通过；原版／候选两种GPU模式均通过上述生产验证，日志无ERROR／panic／VUID。记录：`target/re-flora-logs/re-flora-20261007-163205.292-2090880.log`、`re-flora-20261007-163400.060-2091835.log`、`re-flora-20261007-163209.529-2090934.log`。未重跑此前有5项既有默认值失败的全量测试；无生成文件改动，无可见游戏自动启动。
- 下文历史性能持有测量包含旧200 ms等待；固定工作量的形状／调用对比不受该时钟改动影响，旧持有数量不代表更新后的5秒出料数量。

### 大山包持续堆土性能 A/B

实验前已给视觉认可的 `fea03f56` 创建本地标签 **`soil-before-perf-20261007`**，本地配置另存于 `target/soil-performance-baseline/`。没有推送发布标签。

Debug → **Soil Deposition** → **Soil: faster sampling and settling (A/B)**：不勾选是原版，勾选开启采样范围提示、稠密列查询、延迟径向计算与自适应采样高度。每次仍读取权威 atlas；没有跨请求旧体素缓存、异步发布、降出料量或改形状。所有普通沉积 fixture 比较两套列查询的结果／字节，裁剪高度另测扩区重放与洞顶／状态字节。

RTX 3060 Ti、原生 5120×2880 隐藏窗口；当前本地 **512／批、20批／秒、散布13、土团0、斜率1**。交错 A1→B1→B2→A2 的真实 LMB 持有测试：

- 百万体素山包：平均 **16.53–16.56 → 58.19–58.31 FPS**（约3.52×），帧 p95 **82.15–84.20 → 31.00–31.21ms**；5秒原版新增48640、候选49152，未减少出料。帧节奏导致批次分组与射线／随机序列不同，不用这组最终哈希判定几何等价。
- 固定相同种子／位置／新增40960体素的百万山包：512体素调用平均 **57.85–58.08 → 25.99–26.02ms**（约2.23×）；四次最终字节哈希一致 **`4e5e1e5177ebbd05`**。
- 52万体素山包两种模式平均都接近58FPS；候选主要降低尾部帧时。候选仍有71–73ms冷帧尖峰，不是无卡顿保证。

没有测得整体10×／100×。此次复现比用户约30FPS的观察更重，不能保证用户原场景相同增益；最大散布、长移动笔划、普通花园及其他GPU仍需单独验收。完整方法、数据和进一步优化边界见 [性能调研与实验记录](research/soil_deposition_performance.md)。

`cargo fmt --check`、`cargo check`、Release构建通过；沉积23、工具13通过。全量主程序1419通过／5项既有默认值失败／6忽略，collision 4通过。候选模式真实屋顶仍通过4224逐粒支撑、材质／floor／发布／库存、1／10／50／512／1024点击持有验证；普通花园有限0／无限16／有限7实际供应与扣账通过。所有最终app日志无ERROR／panic／VUID；未自动启动可见游戏。

## 当前限制与后续

- 是局部沉积规则，不是完整颗粒物理或全局安息角求解；停止投放后不继续全场模拟。
- 当前可拾取体素地形均可作支撑；裸屋顶模型使用其明确的工作区支撑平面。未为所有其他非体素模型新增通用沉积碰撞桥接。
- 活动区 CPU atlas 读回和同步可见发布已测隔离屋顶的固定工作量／真实持有 A/B，但不是全部场景的性能验收。最大半径、长移动笔划、普通花园与不同GPU需要后续 Release 实测。
- 历史初版屋顶 fixture 日志约 0.9–1.6 ms；当时 Release 的一粒沉积调用约 1.33 ms，而加速诊断的 16384 粒批次约 40–151 ms（较大堆含扩区重算）。这些局部调用不包含全部可见发布／渲染成本，不构成持续帧时或性能验收；高出料速率可产生明显停顿。
- 撒落 FX、物品购买、土壤改良、可重新扰动的松散土，仍是独立的后续范围。
