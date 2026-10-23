# 资产包总表（生成文件）

维护方式与统计口径见 [README](README.md)。逐文件明细见 [inventory.csv](inventory.csv)。
引用/许可人工审阅基线：`1067833b1b0f439726433dd5e2d2aabdd131ea21`；审阅日期：2026-10-23。
文件数量、字节数与哈希来自当前 Git 跟踪文件的工作树内容，不代表自动重新审核过代码或许可。

共 **600** 个登记文件/容器，**94.667 MiB**；
其中 `assets/` **505** 个，外部 **95** 个。
随包说明也计文件；HTML 按容器计一个，不展开内嵌媒体；未按哈希去重。

## 使用状态（逐文件口径）

| 状态 | 文件数 |
| --- | ---: |
| README 引用 / 发行包复制 | 1 |
| 当前制作源，非游戏直接输入 | 2 |
| 文档/验收展示 | 3 |
| 文档证据，非游戏输入 | 18 |
| 条件加载：主图标缺失时 | 7 |
| 构建校验 / 模型制作源 | 8 |
| 研究/分析参考，非游戏输入 | 1 |
| 研究/制作输出，非游戏模型输入 | 8 |
| 研究文献 | 3 |
| 研究网页/证据 | 49 |
| 离线制作/回归证据 | 8 |
| 离线网页原型 | 1 |
| 编译时嵌入 | 1 |
| 编译时嵌入 / 构建源 | 6 |
| 编译时嵌入 / 网页预览 | 1 |
| 运行时加载 | 11 |
| 运行时加载全部切片 | 384 |
| 运行时预加载 / 事件播放 | 72 |
| 运行时预加载 / 生态触发 | 2 |
| 随包来源/许可说明，不加载 | 14 |

## 分组与处理建议

表中数量包含该组随包说明；真正的引用状态以 CSV 的逐文件 usage 为准。
“先核许可”不是“已确认侵权”；“归档”是建议，不是删除授权。

| 资产组 / ID | 文件数 | MiB | 使用状态 | 分仓建议 | 来源/许可证据 | 尚缺信息 |
| --- | ---: | ---: | --- | --- | --- | --- |
| Pixelify Sans (`font`) | 3 | 0.081 | 运行时加载 | 保留公开 | 已核本地字体 OFL 1.1；许可随包 | 原始下载批次未记录；当前上游 TTF 不同字节，不影响已核字体内许可 |
| 项目 HRTF (`hrtf-project`) | 2 | 3.045 | 运行时加载 | 先核许可，保留合法基础版 | NH172 Neumann KU100；原始数据与转换来源不完整 | 补原始数据 URL/许可、转换命令；显式 override 缺失不会退回内嵌数据 |
| PetalSonic 内嵌 HRTF (`hrtf-embedded`) | 1 | 3.045 | 编译时嵌入 | 先核许可，保留合法基础版 | 与项目 HRTF 字节相同；crate 代码许可不代替数据许可 | 和项目副本一起处理；移走会破坏编译 |
| 正式蝴蝶模型 (`model-butterfly`) | 1 | 0.014 | 编译时嵌入 / 网页预览 | 保留公开基础资源 | 项目 Blender v5 制作；项目所有者确认由助手发起的蝴蝶实验为项目自有内容 | 当前制作源已保留；旧实验仅因不再需要而删除，不按版权风险处理 |
| 叶片、64 变体与生成源 (`model-leaf`) | 3 | 0.119 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成；源码、发布指纹可追溯 | 无已知外部原始模型依赖；保持源码与生成物同步 |
| 苹果模型与生成源 (`model-apple`) | 2 | 0.033 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成 | 保持源码与生成物同步 |
| 花朵模型、调色板与生成源 (`model-flower`) | 5 | 1.447 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成；JSON 含派生贴图数据 | JSON 内嵌 atlas 不单独计图片；保持源码与生成物同步 |
| 土袋模型、贴纸与生成源 (`model-soil`) | 4 | 0.202 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目模型/矢量贴纸生成 | 保持源码、JSON、PNG 与指纹同步 |
| 跨包说明 (`bundle-notes`) | 2 | 0.025 | 说明文件 | 保留公开 | 项目文档 | 无 |
| 脚步音效 (`footsteps`) | 73 | 4.124 | 运行时预加载 / 事件播放 | 拆出私有或替换 | High Impact Media / Tom Winandy；购买已确认，当前许可已保存 | 禁止可提取原始音频的开源分发；历史处理和购买时版本条款未恢复 |
| 地形编辑循环与 UI 按键 (`sound-armoury`) | 2 | 14.081 | 运行时预加载 / 事件播放 | 暂按私有候选，待授权确认 | WAV 元数据：Sound Armoury | 缺商品 URL、购买记录、许可原文和历史处理记录 |
| 夏蝉音效 (`cicadas`) | 4 | 1.654 | 运行时预加载 / 生态触发 | 暂按私有候选，待授权确认 | SA_SF_CricketsCicadas_96k_v1.0；已有文件对应与处理记录 | 缺购买凭证、商品 URL 和许可原文 |
| 当前工具图标 (`tool-icons`) | 8 | 0.009 | 运行时加载 | 先补来源，再定是否拆出 | Git 导入记录只有 add icons | 请确认这 8 个图标的自制/生成/购买来源及许可；未使用的 tree.png 已删除 |
| 农具图标：7 个备用项及许可 (`farm-fallback`) | 8 | 0.003 | 条件加载：主图标缺失时 | 拆出/替换，先做公共兜底 | 随包许可允许项目使用，禁止原始素材再分发 | 缺作者/商品 URL/购买记录；当前备用依赖仍在 |
| 太阳贴图及许可 (`sun`) | 2 | 0.003 | 运行时加载 | 拆出/替换，先做公共太阳 | Planets_16x16 随包许可限制原始资产再分发 | 补作者/购买来源；当前缺文件会 panic |
| FAST 噪声（两组） (`fast-noise`) | 129 | 8.026 | 运行时加载全部切片 | 保留合法基础数据，先补许可 | README 给出 EA FastNoise 和生成命令 | 补生成器版本、上游 BSD 通知及输出适用许可核对；不只给链接 |
| STBN 噪声（四组） (`stbn-noise`) | 257 | 12.184 | 运行时加载全部切片 | 先核许可；必要时替换 | README 仅 NVIDIA 技术博客链接 | 缺来源版本、数据许可与取得方式；不能从代码/文章许可推断纹理可商用 |
| 风声参考录音 (`wind-reference`) | 1 | 3.058 | 研究/分析参考，非游戏输入 | 先核来源，必要时隔离 | 文档称 reference capture，未写原音源授权 | 补录制对象、原始来源与许可；capture 不等于自己拥有声音 |
| README 宣传图 (`splash-promo`) | 1 | 1.377 | README 引用 / 发行包复制 | 先补来源 | 未找到同目录制作说明 | 确认截图还是绘制/生成图及素材来源；不是游戏启动屏加载图 |
| 原生游戏证据截图 (`native-evidence`) | 18 | 5.009 | 文档证据，非游戏输入 | 保留文档，按所含素材核权 | 相关验收文档/屋顶 PoC 截图 | 截图中的第三方视觉素材不自动变成项目自有；无需当作运行资产分仓 |
| 抖动实验截图 (`dither-evidence`) | 49 | 12.842 | 研究网页/证据 | 保留文档 | evidence.json 记录真实 Release 截图来源和设置 | 注意截图所含第三方素材；不是运行时噪声贴图 |
| 石材 OBJ 导出样本 (`stone-obj`) | 8 | 0.046 | 研究/制作输出，非游戏模型输入 | 保留或独立研究归档 | 项目 StoneSpec 程序生成 | 运行时使用 Rust 生成器而非读取这些 OBJ |
| DDGI 论文 PDF (`papers`) | 3 | 21.496 | 研究文献 | 核实再分发；可改官方链接 | 有作者、论文与官方来源链接 | 逐篇核 PDF 转载条款；能公开下载不等于允许仓库镜像 |
| 批准蝴蝶 Blender 源工程与导出 GLB (`butterfly-v5-source`) | 2 | 0.521 | 当前制作源，非游戏直接输入 | 保留制作源或独立公开制作仓库 | 项目自有 Blender v5 制作源；项目所有者已确认实验归属 | v2 调色板依赖已迁入 v5/palette.py；完整 Blender 重导出本次未执行 |
| 批准蝴蝶离线图集/截图 (`butterfly-v5-evidence`) | 8 | 0.140 | 离线制作/回归证据 | 可归档，保留源工程关联 | 项目自有 v5 制作流程派生输出 | 保留当前制作验证证据；不被游戏读取 |
| 内嵌字体的启动屏原型 HTML (`splash-html`) | 1 | 0.131 | 离线网页原型 | 保留工具，随附字体许可 | 项目生成网页；内嵌 Pixelify Sans 原字节 | 单独分发 HTML 时也须遵守字体 OFL；不只检查独立 TTF |
| 内嵌图片的性能/原理报告 (`evidence-html`) | 3 | 1.952 | 文档/验收展示 | 保留文档，逐容器核对 | 报告内有运行观测/图表；按 HTML 容器列出 | 未逐张展开 data URI 做来源鉴定；不代表已全部授权 |

## 引用与来源证据索引

- **font**：[assets/font/README.md](../../assets/font/README.md)；[assets/font/OFL.txt](../../assets/font/OFL.txt)；[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)
- **hrtf-project**：[assets/hrtf/README.md](../../assets/hrtf/README.md)；[src/audio/spatial_sound_manager.rs](../../src/audio/spatial_sound_manager.rs)；[crates/petalsonic/src/spatial/processor.rs](../../crates/petalsonic/src/spatial/processor.rs)
- **hrtf-embedded**：[crates/petalsonic/src/spatial/processor.rs](../../crates/petalsonic/src/spatial/processor.rs)；[assets/hrtf/README.md](../../assets/hrtf/README.md)
- **model-butterfly**：[src/model_assets.rs](../../src/model_assets.rs)；[assets/models/README.md](../../assets/models/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)；[docs/assets/cleanup-unused.md](../../docs/assets/cleanup-unused.md)
- **model-leaf**：[src/model_assets.rs](../../src/model_assets.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-apple**：[src/tracer/apple_preview.rs](../../src/tracer/apple_preview.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-flower**：[src/flora/models.rs](../../src/flora/models.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-soil**：[src/soil_bag.rs](../../src/soil_bag.rs)；[src/tracer/model_geometry.rs](../../src/tracer/model_geometry.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **bundle-notes**：[assets/models/README.md](../../assets/models/README.md)；[assets/sfx/README.md](../../assets/sfx/README.md)
- **footsteps**：[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/audio/local_player_footsteps.rs](../../src/audio/local_player_footsteps.rs)；[assets/sfx/Footsteps SFX - Undergrowth & Leaves/README.md](../../assets/sfx/Footsteps%20SFX%20-%20Undergrowth%20%26%20Leaves/README.md)；[assets/sfx/Footsteps SFX - Undergrowth & Leaves/LICENSE.txt](../../assets/sfx/Footsteps%20SFX%20-%20Undergrowth%20%26%20Leaves/LICENSE.txt)
- **sound-armoury**：[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/app/core/input.rs](../../src/app/core/input.rs)
- **cicadas**：[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/audio/summer_cicadas.rs](../../src/audio/summer_cicadas.rs)；[assets/sfx/summer_cicadas/README.md](../../assets/sfx/summer_cicadas/README.md)；[assets/sfx/summer_cicadas/provenance.json](../../assets/sfx/summer_cicadas/provenance.json)
- **tool-icons**：[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)；[src/app/core/mod.rs](../../src/app/core/mod.rs)
- **farm-fallback**：[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)；[src/app/core/mod.rs](../../src/app/core/mod.rs)；[assets/texture/Pixel_Farming_Tools_IconSet_16px/README.txt](../../assets/texture/Pixel_Farming_Tools_IconSet_16px/README.txt)
- **sun**：[src/tracer/resources.rs](../../src/tracer/resources.rs)；[assets/texture/Planets_16x16/License.txt](../../assets/texture/Planets_16x16/License.txt)
- **fast-noise**：[assets/texture/noise/fast/README.md](../../assets/texture/noise/fast/README.md)；[src/tracer/resources.rs](../../src/tracer/resources.rs)
- **stbn-noise**：[assets/texture/noise/stbn/README.md](../../assets/texture/noise/stbn/README.md)；[src/tracer/resources.rs](../../src/tracer/resources.rs)
- **wind-reference**：[docs/procedural_tree_rustle_progress.md](../../docs/procedural_tree_rustle_progress.md)
- **splash-promo**：[README.md](../../README.md)；[scripts/package_release.py](../../scripts/package_release.py)
- **native-evidence**：[docs/evidence/model-cache-restored/summary.json](../../docs/evidence/model-cache-restored/summary.json)；[docs/research/model-view-quantization.md](../../docs/research/model-view-quantization.md)；[docs/rooftop_poc.md](../../docs/rooftop_poc.md)；[docs/research/procedural-stone-models.md](../../docs/research/procedural-stone-models.md)
- **dither-evidence**：[docs/research/ordered-dither-assets/evidence.json](../../docs/research/ordered-dither-assets/evidence.json)；[docs/research/ordered-dither.md](../../docs/research/ordered-dither.md)；[docs/research/ordered-dither.html](../../docs/research/ordered-dither.html)
- **stone-obj**：[docs/research/procedural-stone-models.md](../../docs/research/procedural-stone-models.md)；[src/stone_models.rs](../../src/stone_models.rs)
- **papers**：[docs/references/ddgi/README.md](../../docs/references/ddgi/README.md)
- **butterfly-v5-source**：[experiments/butterfly-method-comparison/blender-v5/README.md](../../experiments/butterfly-method-comparison/blender-v5/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)；[assets/models/README.md](../../assets/models/README.md)；[docs/assets/cleanup-unused.md](../../docs/assets/cleanup-unused.md)
- **butterfly-v5-evidence**：[experiments/butterfly-method-comparison/blender-v5/README.md](../../experiments/butterfly-method-comparison/blender-v5/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)
- **splash-html**：[docs/research/splash-prototype.md](../../docs/research/splash-prototype.md)；[scripts/build-splash-prototype.mjs](../../scripts/build-splash-prototype.mjs)；[assets/font/OFL.txt](../../assets/font/OFL.txt)
- **evidence-html**：[docs/reports/soil-deposition.html](../../docs/reports/soil-deposition.html)；[docs/evidence/grass-pixelization-performance.html](../../docs/evidence/grass-pixelization-performance.html)；[docs/research/ddgi_response_explained.html](../../docs/research/ddgi_response_explained.html)
