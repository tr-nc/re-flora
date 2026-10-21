# 草的土壤湿度外观响应 — scope v2 交付记录

工作树 `re-flora-agent-garden-wetness`，分支 `agent/garden-wetness`，基线
`16cd68d30b85bf88504be44c5a1fcb43fc900ffe`。本记录只覆盖 wetness，不能代替新花园、
音效或整体性能验收。全部运行均为隐藏、静音 Release；没有推送、打包或打开可见窗口。

## 实际实现与边界

- 高草、矮草的线性材质颜色按支撑土壤湿度 0/1/2/3 乘以
  `1 / 0.916667 / 0.833333 / 0.75`。先应用原有 growth-stress tint，再做湿度暗化，
  最后进入现有照明。没有更换草的配色、光照、几何或风模型。
- `floraSupportSample` 复用原来的支撑体素 atlas fetch，分别提供 `growth_factor`
  和 `moisture_level`。原来的 `floraEnvironmentGrowthFactor` 签名仍保留为兼容包装。
  竞争、生长曲线和 Debug growth override 不再被误当作材质湿度。
- 材质只认 dirt/sand，非土壤、无效 species、atlas 越界、坐标加法溢出、chunk-local
  越界均为干；保留非土壤支撑原有的生长解码语义。花、果实、树叶返回精确原颜色。
- cached compute 和 uncached native band vertex 使用同一个纯材质函数；pixelized
  表现继续消费相同的颜色。玻璃光传输 cache 变体 import 同一 compute 源。
  **当前基线已删除 legacy voxel grass**，原 fixture 也明确拒绝 `-a`；没有伪造一个
  不存在的旧渲染路径或为这项工作重新引入它。
- atlas 仍是唯一水分权威：手动 Water、喷头、土壤转移、干燥和存档恢复都自然生效。
  无独立湿度计时器、第二套 wet mask、纹理、buffer、dispatch、反射 pass、CPU
  per-blade 查询或 per-pixel 喷头扫描。`Instance` 仍是对齐的 8 字节，atlas/save 格式不变。
- 这是**根部/土壤湿度的外观代理，不是每片叶面真的存有水的证明**。停止喷洒后土壤
  仍湿，草就仍暗；四档湿度仍可能产生阶梯变化。25% 是美术起点，不是实测草叶 BRDF。
  来源和物理局限见 [研究笔记](../research/vegetation-wet-materials.md)。

### 保存的运行时对照控件

Debug → **Moisture Appearance** → **Grass soil-moisture darkening (unchecked: original)**。
默认勾选；取消勾选精确恢复原材质颜色，不改生长或土壤。只增加一个 bool，故没有
非有限 float 或不同入口 clamp 的歧义。唯一值来自 `config/gui.toml` 的声明，经过统一
GUI/Save → `FloraAppearanceFrameInput` → `U_GuiInput`；上传为规范的 0/1。
旧配置缺失该项时只在内存追加默认项，加载不写文件；已有 false/true 均保留。
没有重置任何已保存偏好或 camera snapshots。

构建自动更新的 tracked 文件仅为：

- `src/app/generated/gui_adjustables_gen.rs`：声明式 bool 的统一访问/保存映射；
- `src/auto-generated/gpu_structs.rs`：`GuiInput` 增加 uint，使用原有 4 字节 padding，
  没有手工修改生成文件。

## 正确性证据

所有 evidence 位于 `target/garden-delivery/wetness/`，集成时应复制同一相对目录。
不是只检查 PNG 是否存在：已实际打开 native 原图、cached/native 合图、tall/short 合图、
pixelized 图、喷头图及 lifecycle 的 restored/drying/valve-paused 图。

### 自动验证

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test` 最终完整运行：主应用 **1658 passed / 8 ignored**，library **4 passed**，
  display authorization **1 passed / 1 ignored**，clipboard **3 ignored**；无失败。
  日志 `final-validation.log`。随后仅修改 opt-in valve fixture，重新 check、跑该 fixture
  的 **4 项测试**、默认 Release 构建和实际 lifecycle native run 通过。
- `SLANGC=/home/terence/vulkan-sdk/x86_64/bin/slangc python3 scripts/run_slang_tests.py`：
  **50 项通过**（含新 `grass_moisture_contract.slang`），使用 pinned 2025.23.2。
- Slang CPU contract 直接调用生产材质/解码函数：穷举 256 个 atlas byte（含 reserved
  bits 和所有 type）、invalid support、干燥精确 identity、关闭精确 identity、非草精确
  identity、湿度单调/有界及 stress 输入与湿度系数独立。
- Rust 覆盖缺失项加载不写文件、已有 false/true 保留、save/reload、所有声明式参数 live
  edit 保存、完整 frame mapper 字段；source wiring guard 确认两个消费者只取一次 support、
  stress → wet → lighting 顺序以及 raw moisture 不读取 growth override。
- 默认 validation-layer Release 的 smoke 与 fixture 均正常退出；检查日志没有
  ERROR/VUID/panic。fixture shutdown `failures=0`。最终默认 `cargo build --release`
  已完成；没有把 no-validation 性能二进制留作最终 try-out 二进制。

早期验证确实失败过：一次 120 秒工具超时中断了测试编译；一次新增测试跨模块调用私有
`DebugSettings` 方法导致 E0624。已改用 loader 的公开保存接口，并由既有通用 live-save
测试覆盖 GUI 同步；最终完整验证通过。保留 `contracts.log`、`contracts-2.log`，不把这些
失败运行计为通过。没有降低断言或改 unrelated defaults。

### 匹配截图与读取值

截图均为 2560×1440；原有 scene stride=5，实际 scene=512×288。固定 camera/painting，
growth override=1；moisture fixture 在 warmup 后冻结 wind/spawn pose。A/B 切换只改材质
bool，**同一湿土、同一草数量**，因此不是把土壤颜色、生长或风差异误算为湿草效果。
跨进程的 DDGI/dither 仍有小噪声，不能宣称最终 PNG bit-identical。

| 证据 | 实际观察 |
| --- | --- |
| `cached-native-comparison.png` | 上行 cached、下行 native；左关闭、右开启。两条路径都呈温和暗化，形状不变 |
| `tall-short-comparison.png` | 上行 tall、下行 short；左关闭、右开启。阴影和日照草都响应，树干不被整体染暗 |
| `manual-r1-p1-on1/near-both-b.png` | pixelized 路径实际显示同类响应；没有单独做 pixelized off/on 完整矩阵 |
| `dry-r1-p0-on{0,1}/near-both-b.png` | dry control；土壤采样 sum=0，原函数严格 identity，native PNG 只有很小跨进程噪声 |
| `sprinkler-r1-p0-on1/near-both-b.png` | 实际喷头浇水，含可见喷雾；不把喷雾泛白解释成草的暗化 |

`image-comparison.json` 的归一化全图 RMSE：wet cached A/B **0.0272372**，
wet native A/B **0.0272371**；dry A/B noise **0.000536517**；tall A/B **0.00852161**，
short A/B **0.00745690**。这是输出图差异，不是线性材质系数或独立草像素的精确测量。
两种 grass path 的 A/B 差异一致，但其完整画面并非逐像素相同。

手动浇水的 matched A/B 在 frame120 都得到同一 sphere readback：
`count=17728, sum=14912`；邻居 `count=2259, sum=0`。
实际 sprinkler-only run 在 frame120 为 `sum=3724`，frame180 为 `4613`；邻居仍为 0。
这些是 sphere 中**土壤体素的湿度等级总和**，不是刀片湿度或表面百分比。

### 真实生命周期（`lifecycle-valve.log`）

全部使用现有 gameplay watering、placement/removal、flora painting 和 terrain save/load。
存档只写入 fixture 自建的临时目录，退出自动删除；不访问用户已保存世界。

| fixture frame | 操作 / 实际土壤读取 |
| --- | --- |
| 10–25 | 正常手动 Water brush；喷头也使用既有权威 source |
| 30 | 喷头权威 duty=0 暂停阀门；soil sum=14933 |
| 59 | 阀门仍暂停，drying speed=0；sum=14919，草没有因停水立即变干 |
| 60 | duty=100 恢复；在已湿土上真实移除/重种草；sum=14919 |
| 90 | 保存 8 chunks / 134217728-byte disposable snapshot；sum=14914 |
| 120 | 正常移除 1 个喷头；sum=14912，仍湿 |
| 150 | 从快照恢复；assert 读取 sum=14914 与保存时完全相等 |
| 180 | 恢复真实 evaporation，drying speed=3；sum=14909 |
| 300 / 600 / 720 | sum=13620 / 10855 / 10039；邻居始终 sum=0 |

speed=0 只暂停蒸发，转移仍会改变局部 sphere 总和，不能错误断言它恒定。
截图位于 `lifecycle-valve/mid-both-b-{30,59,60,90,120,150,180,300,600}.png`；
另保留第一次生命周期运行 `lifecycle.log`。跨阶段截图存在重种/存档重建/照明稳定过程，
只将固定土壤 A/B 用作严格的材质外观隔离。
这里验证的是**喷头阀门暂停/恢复**和 snapshot quiescence，不是一个全局游戏暂停按钮；
未执行全局暂停的额外 native 验收。

## Release 增量性能实测

最终材质/bounds 版本 `33f52234`，RTX 3060 Ti，driver 580.159.04。
`cargo build --release --features no_validation_layer`；自动选择 FIFO，没有强制 present mode。
固定 wide camera、9×9 planting grid，**16959 tall + 13583 short = 30542 plants**；
manual-wet、0 sprinklers、cached pose、非额外 grass pixelization；场景和其他保存设置不变。

6 次独立进程，顺序 off/on/on/off/off/on，每模式 3 次。每次 warmup 192 fixture frames，
再排除 sample marker 后 20 帧和末 5 帧，每次 **276 samples**。
性能运行不截图；两端 soil readback 帧排除在采样窗口之外。
完整逐帧日志和数值在 `performance/{0..5}-response-{0,1}.log` 与 `performance/summary.json`；
复现 harness 为 evidence 中的 `measure.mjs`，须由外层 heavy lock 调用。

下表为**各次 p50 / p95 的跨三次中位数**，单位 µs；不是合并样本百分位：

| scope | off p50 / p95 | on p50 / p95 | p50 Δ |
| --- | ---: | ---: | ---: |
| GPU grass preparation/cache | 472 / 478 | 471 / 477 | -1 |
| GPU grass draw | 687 / 702 | 686 / 696 | -1 |
| GPU frame.render | 6300 / 6778 | 6277 / 6767 | -23 |
| CPU render.record | 332 / 619 | 338 / 625 | +6 |
| CPU tracked work | 1229 / 1484 | 1230 / 1487 | +1 |
| CPU total frame | 16994 / 17911 | 16956 / 17870 | -38 |

结论：在该负载下，材质开关增量小于本次运行间噪声；**不能把负差宣称成优化收益，
也不能证明成本严格为零**。FIFO 导致 CPU total ~17ms，不能从 FPS 得出 GPU 成本。
首次 bounds guard 前的测量单独保留在 `pre-bounds-performance/`，不冒充最终版本证据。

结构上只是在已有 support fetch 后附加 type/validity 和少量乘法，无新增 pass/fetch/buffer；
这是结构解释，不是另一个实测结论。没有做 uncached 大规模成本、喷头 1/4/8/16
或集成新花园性能矩阵；这部分仍需 controller 的整体性能验收。原有 prepare 256 lanes、
logical workgroup 语义、stable wetting contacts、四种 16-pose 硬件和共享水剂量均未修改。

## 复现与剩余验收

已有 Release 后，例如默认 validation 的 near manual A/B（response 改 0/1，各用独立目录）：

```sh
flock -o /tmp/re-flora-garden-delivery-heavy.lock sh -c '
  if [ -n "$DISPLAY" ]; then unset WAYLAND_DISPLAY WAYLAND_SOCKET; fi
  RUST_LOG=info RE_FLORA_GRASS_STEM_REVIEW=near-both-b \
  RE_FLORA_GRASS_MOISTURE_REVIEW=manual RE_FLORA_GRASS_MOISTURE_RESPONSE=1 \
  RE_FLORA_GRASS_STEM_CAPTURE=target/garden-delivery/wetness/review-on \
  RE_FLORA_STEM_SAMPLE_FRAMES=60 \
  target/release/re-flora --hidden --mute --windowed --perf --authored-flora-bench
'
```

`RE_FLORA_GRASS_MOISTURE_REVIEW` 支持 dry/manual/sprinkler/lifecycle；不设置则保持旧
fixture 行为。lifecycle 用 sample frames=600，产生上述完整阶段。所有调参仅会话生效，
不会触发 GUI Save；production runtime checkbox 正常由统一 Save 保存。

剩余：玩家美术批准；集成新花园下的多喷头和 uncached 性能；pixelized 完整 A/B 矩阵；
全局游戏暂停（区别于已测阀门暂停）；native 无效/移除支撑与边界编辑组合的额外截图。
解码和越界保护有 CPU contract/source wiring/编译证据，但不谎称后几项 native 场景已经跑过。
当前高曝光黄绿色配色下效果温和，不是高光、反射或强制“更健康”的饱和度变化。
所有最终 code/config 改动均本地提交；只增加必要配置项，无用户世界或相机偏好改动。

## 集成后续（scope v4，已完成）

最终验收补齐真实世界时钟暂停、湿支撑移除、越界编辑和完整花园性能测量，并修复旧 moisture
提交未随暂停停止的缺口。主工作树暂停时土壤 sum=3177、tick/fraction 精确保持；恢复后继续变化。
见[最终验收](garden-final-acceptance.md)及[总报告](garden-delivery.md)。这不等于新增了完整的
pixelized/uncached 大人口性能矩阵，也不把当前测量外推为所有渲染路径零成本。
