# 单体素铺土：网格偏向、尖柱与空心风险

日期：2026-10-07；实现分支 `terrain-magic`，`fd374729` / `6afd5c1d`。

用户要求：同一点应逐渐形成圆整的堆，不是方形金字塔；允许一个小体素一个小体素添加，同时必须避免随机单粒形成孤立 2–4 格柱和空心／网状内部。研究与验证后交付 HTML，期间不启动可见游戏。

## 一手资料与结论边界

1. [SideFX HeightField Erode Thermal](https://www.sidefx.com/docs/houdini/nodes/sop/heightfield_erode_thermal.html#settle_repose)：Repose Angle 是不再滑移的最大坡度；官方建议现实的土／石约 30–45°。这支持“材料先投放、局部坡度决定去向”的职责分离，不是照画一个圆锥。我们的 slope 是 rise/run，默认 1 对应 45°，体素接触与分辨率使它不是精确连续角度求解器。
2. [SideFX Grain Source](https://www.sidefx.com/docs/houdini/nodes/sop/grainsource.html)：Jitter Scale 文档要求启用 jitter 后做 relaxation；其 Relax Iterations 段明确说明非均匀尺寸／jitter 会导致初始重叠。Dither Normal 段提醒碰撞约束面的 dithering 可能生成碰撞范围外的点或会塌陷的孔洞。这是颗粒求解器中的采样／接触警告，不能直接说同样数值适用于我们，但足以说明“加随机”不等于“堆得正确”。
3. [SideFX Points from Volume](https://www.sidefx.com/docs/houdini/nodes/sop/pointsfromvolume.html)：Grid 与 Tetrahedral 是不同的点配置；Jitter Scale 只是位置随机变化，设 0 就不 jitter。采样配置与材料落定约束是不同问题，随机粒子云不保证致密地形。

以上三页已获取官方原始 HTML、核对具体参数段。可读正文提取最初只返回巨大导航；不以导航或搜索摘要冒充实际参数证据。影视／Houdini 的颗粒求解不是本项目算法；没有声称实现 PBD、DEM 或 MPM。更广的游戏／工业／论文背景见 [初次调研](soil_sprinkling_and_granular_deposition.md)。

## 本地诊断而非从文献外推

- 老同点堆的 16 方向截面宽度最大／最小约 1.333。形状偏向在纯体素数据里出现，不能仅归因于 pixel renderer。
- 在整数列高上每一步重启坡度阈值，会抹掉单位步与对角步之间的差别；只随机扫描顺序不足以修复度量。
- 细颗粒随机输入在刻意陡峭 slope=2 下，于第 11 粒复现缺少 X 轴肩部的高点 `(22,3,22)`。失败记录 `/tmp/terrain-magic-soil-steep-spike-red.log`；强支撑约束后的通过记录 `/tmp/terrain-magic-soil-broad-base.log`。
- 本轮没有把用户曾遇到的“空心／网状旧实现”虚称为已在当前代码复现；将其作为强制结构检查，直接检查体素内部，不能只看表面截图。

## 实现选择

### 一粒就是一个体素

默认 spread=0、clump=0；每次点击立刻释放 1 粒，短按 200 ms 内不重复，之后默认 20 粒／秒。数量用整数纳秒信用累计，帧率不改变长期数量，卡顿补发最多 250 ms。滚轮修改独立的保存散布字段，不借用 Edit 半径。

### 欧氏源相对坡度，而不是离散步步重启

保留颗粒初始的高度和累计水平欧氏位移。选下坡邻居时，高差分别按 1 与 √2 归一化；八方向遍历顺序的随机只做平局消偏。所选步骤严格降低列高，保证终止；不靠一次性的几何圆锥 stamp 或改用大土团让问题不显眼。

### 广基底接触与填充前沿

新增只发生在支撑上方的第一个空气格；不在三维盒子里随机打孔。Dirt 上叠加必须在下一层的 X、Z 两轴各有至少一个实体肩部；硬基底和显式模型平面可承接第一粒。支撑不足时继续滚落，否则拒绝；拒绝不扣库存。没有删除已写土、压缩体积、额外补土或只隐藏尖柱的视觉修补。

只记录当前三维支撑上方的局部空气区间，不把整个世界替换成二维高度场。保留旧材质／含水状态、原有洞穴和固定上盖；不借“无空心”之名封闭已有世界空腔。

### 读区不能成为隐藏堆脚上限

初始 32 格 halo 只是读区估计。规划触及未读但合法的空间且可能需要继续下坡／支撑时，标记试算需扩区；builder 丢弃尚未上传的试算数据，从原 atlas 重读并以同一请求／种子重算。最终一次上传、一次数量记账。实际世界／工作区边界不可扩展。

增加回归：扩区重放与一次完整规划逐字节相同；65536 粒的基底宽度超过旧 64 格限制。否则中等堆圆整也可能在继续堆高后变成固定半径柱台。

## 当前证据

- 16384 粒同点、spread=0/clump=0/slope=1，两个截面 16 个方向的宽度比 1.051 / 1.094；保留 `<1.15` 原门槛。
- 旧复现相同输入（spread=2/clump=1）当前为 1.071 / 1.077，旧结果约 1.333。
- 4096 次 spread=8、clump=0、slope=2 单粒：逐次检查新增粒的双轴接触；最终竖向缺格、各层封闭孔洞、无肩部尖柱均 0。slope=2 是压力测试，不是推荐的真实土角度；宽随机输入的上层外轮廓不强制按单点圆锥标准验收。
- 三组导出数据各层封闭孔洞均 0。图来自真实 planner 的占用数据，不是示意的目标圆锥。
- 生产 GPU：4224 个实际新增体素逐个检查下方支撑；LMB 路径精确写 1，100 ms 后仍为 1；原材质、地板、库存、可见发布和旧 Edit／Grow／A/B 验证通过。
- 空白独立 GPU plot：65536 请求／写入一致，初始 66 宽读区扩为 132，实际堆脚宽 78、最高 y=254（底面216），竖向孔洞0。确认初始为空，避免混入旧几何笔刷结果再称为新沉积。

## 可复现入口

普通快速测试：`cargo test terrain_deposition`（20 个回归通过）。显式数据导出：

```bash
cargo test export_soil_report_samples -- --ignored --nocapture
```

输出 `target/soil-deposition-report.json`。正常测试默认跳过写文件导出器，逻辑回归本身不跳过。

Release GPU／截图（诊断会改本次临时屋顶地形；屋顶不持久化）：

```bash
env -u WAYLAND_DISPLAY RE_FLORA_ROOFTOP_VALIDATE=1 RE_FLORA_SOIL_REPORT_CLOSEUP=1 \
  RUST_LOG='warn,re_flora::app::core::rooftop_scene=info,re_flora::builder::plain=info,re_flora::run_log_binding=info' \
  flock --close --timeout 120 /tmp/re-flora-summer-gpu.lock \
  cargo run --release -- --hidden --mute --rooftop-poc \
  --screenshot rooftop-scene target/soil-fine-rooftop.png \
  --screenshot-delay 0.1 --auto-exit 1
```

实际截图内嵌于 [HTML 报告](../reports/soil-deposition.html)，数量被诊断加速，不是假装默认出料速度这么快。

## 未完成或不能据此声称的内容

- 数值与 Release 隐藏图形路径通过不等于玩家已批准视觉效果。
- 表面保留一格的颗粒起伏与体素阶梯；没有去改变 Dirt 材质／光照／像素后处理。
- 不是完整摩擦、颗粒再扰动或全局持续滑塌模拟，低坡值也受整数网格／局部接触限制。
- 65536 的四个 16384 粒加速批次日志约 40/58/140/151 ms（含较大堆的重试）；一粒调用约1.33ms。它们不是总帧时，不能声称持续性能预算通过；高出料量可能明显卡顿，优化应在视觉确认后按 Release 实测另做。
- 正常花园保存／加载端到端尚未新增验收。
- 全量测试的5项已在未改动基线复现的默认值断言仍失败；没有改用户无关默认设置去刷全绿。
