# 渐进铺土与堆高

## 职责与范围

本阶段只实现地表沉积，不实现空中撒落粒子、购买流程或土壤改良。

- Soil 工具决定有限投放量和分布。
- 沉积模块决定当前支撑面上的覆盖与局部堆积。
- 只增加新的 Dirt 体素，不替换旧地形，不改写旧土含水等属性。
- 继续操作可以不断加厚；横向范围不是最终高度上限。世界和可编辑工作区边界仍然有效。
- 原 Edit／Dig、Smooth、Till 等工具保持原行为。

研究背景见 [调研笔记](research/soil_sprinkling_and_granular_deposition.md)。用户在讨论后明确要求把铺土与改良分开，并把空中撒落表现推迟。

## 试用

从本工作树运行可见 Release（不要带 `--hidden`；普通试用不要带 `--perf`）：

```bash
cargo run --release -- --rooftop-poc
```

- 按 **B** 或点击工具栏 **Soil**。
- 左键点击／按住：逐渐覆盖并堆高；移动落点留下有厚薄变化的土层。
- 滚轮：调整横向铺土范围。同样时间投放量不随范围增大。
- **3** 切回原 Edit／Dig，可继续用原几何编辑方法比较或移除土。
- Debug 的 **Soil Deposition** 集中放置三个可保存参数：出料量、土团半径、堆坡斜率。

普通花园使用背包中收集的 Dirt，按实际写入量扣除；没有 Dirt 时不制造库存。屋顶 proof 沿用既有的不限材料测试规则，Soil 始终投放 Dirt，不改变 Edit 的材料选择。屋顶 proof 本身不保存地形。

本次没有自动启动可见游戏。

## 实现边界

- `src/terrain_deposition.rs`：独立的有限体积规划与时间预算；不依赖玩家输入、购买或 FX。
- `src/builder/plain/mod.rs::deposit_soil`：读取权威 atlas，在 CPU 规划沉积，裁剪实际改变范围，一次上传，再标记 solid-workgroup 数据过期。
- `src/app/core/soil_deposition.rs`：工具、库存、可编辑范围及统一 Visible Terrain Publication 的连接。
- `src/app/core/player_tools.rs`：Soil 独立语义动作、按键与释放／切换时的路径和预算清理。
- `config/gui.toml`：声明式保存参数；`src/app/generated/gui_adjustables_gen.rs` 由 `cargo check` 生成，没有手工修改。

每个剂量使用软边分布的随机土团，而不是填满球／胶囊。移动笔划只分配这份有限材料，不因笔划长度额外制造土量。每个接受的材料单元只写一个空体素，读写范围保留完整的合法 Y 范围，因此可以在新土表面继续累加。

局部列记录选中支撑上方的空气区间，避免把洞穴外表面当成唯一高度图。新增单元在不穿过固体上盖的前提下，最多沿邻域下移 16 次；斜向距离参与坡度判断，邻域遍历起点随机化以减少固定扫描方向偏好。判断使用加入新单元之后的高度差。已有土不被搬走，也不做整个世界的持续滑塌。

点击立即投放一个 80 ms 小剂量；按住随后按实际经过时间计算，保留不足一个体素的余量。释放、失去合法目标、切换工具会断开路径和预算。长时间卡顿后的补发限定为 250 ms，单批也有安全上限；没有停顿后一次性倾倒大量材料的行为。

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

屋顶支撑面为 y=216，笔刷半径约 7.68 体素；累计后的顶面达到 y=228，超过原笔刷纵向范围。原 Edit／Dig、模型／真实体素切换、Grow 与 capsule 验证也通过。该 fixture 验证的是不限材料屋顶；普通背包扣除路径没有独立 GPU 场景测试。

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

## 当前限制与后续

- 是局部沉积规则，不是完整颗粒物理或全局安息角求解；停止投放后不继续全场模拟。
- 当前可拾取体素地形均可作支撑；裸屋顶模型使用其明确的工作区支撑平面。未为所有其他非体素模型新增通用沉积碰撞桥接。
- 活动区 CPU atlas 读回和同步可见发布尚未做正式持续交互性能验收。最大半径、长笔划和多 chunk 场景需要后续 Release 实测。
- 屋顶 fixture 的沉积批处理日志约 0.9–1.6 ms，不包含全部可见发布／渲染成本，不作为整体帧时或性能通过证据。
- 撒落 FX、物品购买、土壤改良、可重新扰动的松散土，仍是独立的后续范围。
