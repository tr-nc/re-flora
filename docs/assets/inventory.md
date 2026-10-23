# 资产包总表（生成文件）

维护方式与统计口径见 [README](README.md)。逐文件明细见 [inventory.csv](inventory.csv)。
引用/许可人工审阅基线：`29307bce128b897b0b96636a17a914e0069eb64b`；审阅日期：2026-10-23。
文件数量、字节数与哈希来自当前 Git 跟踪文件的工作树内容，不代表自动重新审核过代码或许可。

共 **608** 个登记文件/容器，**71.985 MiB**；
其中 `assets/` **517** 个，外部 **91** 个。
随包说明也计文件；HTML 按容器计一个，不展开内嵌媒体；未按哈希去重。

## 使用状态（逐文件口径）

| 状态 | 文件数 |
| --- | ---: |
| README 引用 / 发行包复制 | 1 |
| 仅 selected loop 运行时预加载；original 为不加载的编辑参考 | 2 |
| 当前制作源，非游戏直接输入 | 2 |
| 文档/验收展示 | 3 |
| 文档证据，非游戏输入 | 18 |
| 条件加载：主图标缺失时 | 7 |
| 构建校验 / 模型制作源 | 8 |
| 研究/制作输出，非游戏模型输入 | 8 |
| 研究网页/证据 | 49 |
| 离线制作/回归证据 | 8 |
| 离线网页原型 | 1 |
| 编译时嵌入 | 1 |
| 编译时嵌入 / 构建源 | 6 |
| 编译时嵌入 / 网页预览 | 1 |
| 运行时加载 | 11 |
| 运行时加载全部切片 | 384 |
| 运行时预加载 / 事件播放 | 70 |
| 运行时预加载 / 操作反馈与生态触发 | 4 |
| 随包来源/许可说明，不加载 | 24 |

## 分组与处理建议

表中数量包含该组随包说明；真正的引用状态以 CSV 的逐文件 usage 为准。
“先核许可”不是“已确认侵权”；“归档”是建议，不是删除授权。

| 资产组 / ID | 文件数 | MiB | 具体用途 | 使用状态 | 分仓建议 | 来源/许可证据 | 尚缺信息 |
| --- | ---: | ---: | --- | --- | --- | --- | --- |
| 喷淋器六连录音与来源 (`sprinkler-loop`) | 3 | 1.759 | 喷淋器每 1.56 秒六次喷水的空间循环音效；统一固定运动节奏 | 仅 selected loop 运行时预加载；original 为不加载的编辑参考 | 用户要求恢复用于本地开发；商业及公开分发未获授权 | ElevenLabs SFX v3 Free-plan；历史素材原样恢复 | 保留原有商业使用限制；本轮没有取得新授权或推送 |
| Pixelify Sans (`font`) | 3 | 0.081 | GUI、菜单和文字的像素字体；网页启动屏原型也内嵌同一字体 | 运行时加载 | 保留公开 | 已核本地字体 OFL 1.1；许可随包 | 原始下载批次未记录；当前上游 TTF 不同字节，不影响已核字体内许可 |
| 项目 HRTF (`hrtf-project`) | 4 | 3.071 | 将空间声源滤波为左右耳信号，提供方向听感；应用指定的 HRTF 文件 | 运行时加载 | 保留公开；数据遵循 CC BY-SA 3.0 | ARI NH172 官方 SOFA 与历史原件哈希一致；native 表完整重建字节一致，源元数据明确 CC BY-SA 3.0 | 署名、许可、转换记录已补；修改数据须保留 ShareAlike。外部文件缺失的降级仍待改 |
| PetalSonic 内嵌 HRTF (`hrtf-embedded`) | 1 | 3.045 | PetalSonic 未指定外部 HRTF 时的内嵌默认滤波表；不是第二组音效 | 编译时嵌入 | 保留公开；数据遵循 CC BY-SA 3.0 | ARI NH172 官方 SOFA 与历史原件哈希一致；native 表完整重建字节一致，源元数据明确 CC BY-SA 3.0 | 署名、许可、转换记录已补；修改数据须保留 ShareAlike。外部文件缺失的降级仍待改 |
| 正式蝴蝶模型 (`model-butterfly`) | 1 | 0.014 | 游戏与网页共用的蝴蝶双翼几何、材质分组和拍翼动画 | 编译时嵌入 / 网页预览 | 保留公开基础资源 | 项目 Blender v5 制作；项目所有者确认由助手发起的蝴蝶实验为项目自有内容 | 当前制作源已保留；旧实验仅因不再需要而删除，不按版权风险处理 |
| 叶片、64 变体与生成源 (`model-leaf`) | 3 | 0.119 | 模型叶片渲染及落叶实例的立体叶片、64 个稳定形状变体 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成；源码、发布指纹可追溯 | 无已知外部原始模型依赖；保持源码与生成物同步 |
| 苹果模型与生成源 (`model-apple`) | 2 | 0.033 | 树上和掉落苹果共用的几何与制作配方 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成 | 保持源码与生成物同步 |
| 花朵模型、调色板与生成源 (`model-flower`) | 5 | 1.447 | 6 种可种植模型花的花头几何、调色板权重/atlas 和制作配方 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目参数化生成；JSON 含派生贴图数据 | JSON 内嵌 atlas 不单独计图片；保持源码与生成物同步 |
| 土袋模型、贴纸与生成源 (`model-soil`) | 4 | 0.202 | 土袋几何、包装贴纸与倾倒制作源，游戏与预览共用派生数据 | 编译时嵌入 / 构建源 | 保留公开基础资源 | 项目模型/矢量贴纸生成 | 保持源码、JSON、PNG 与指纹同步 |
| 跨包说明 (`bundle-notes`) | 2 | 0.025 | 资源发布流程、音效选择和来源说明；不是加载数据 | 说明文件 | 保留公开 | 项目文档 | 无 |
| 脚步音效 (`footsteps`) | 73 | 4.124 | 玩家走/跑各 25 条、起跳/落地各 10 条；当前各地面类型共用 | 运行时预加载 / 事件播放 | 拆出私有或替换 | High Impact Media / Tom Winandy；购买已确认，当前许可已保存 | 禁止可提取原始音频的开源分发；历史处理和购买时版本条款未恢复 |
| Sound Armoury 共用来源音效（地形/UI/蝉鸣） (`sound-armoury`) | 9 | 15.745 | 同一采购来源的 4 条音效：岩石循环用于地形编辑，按键用于物品栏反馈，两种蝉鸣用于生态点位 | 运行时预加载 / 操作反馈与生态触发 | 游戏内可商用；公开原件需许可、拆出或替换 | 用户确认四条共用 Sound Armoury SFX Mega Bundle 采购来源；2025-07-09 购买，当前 itch.io EULA 已保存 | 公开 WAV（含修改版）仍需明确许可；历史原始包版本/购买时条款及地形/UI 历史处理未恢复 |
| 当前工具图标 (`tool-icons`) | 8 | 0.009 | 8 个工具主图标：挖土、种植、平整、修剪、浇水、喷水器、检查器、翻土 | 运行时加载 | 先补来源，再定是否拆出 | Git 导入记录只有 add icons | 请确认这 8 个图标的自制/生成/购买来源及许可；未使用的 tree.png 已删除 |
| 农具图标：7 个备用项及许可 (`farm-fallback`) | 8 | 0.003 | 主图标缺失时的 7 个备用图形；没有喷水器专用备用图 | 条件加载：主图标缺失时 | 拆出/替换，先做公共兜底 | 随包许可允许项目使用，禁止原始素材再分发 | 缺作者/商品 URL/购买记录；当前备用依赖仍在 |
| 太阳贴图及许可 (`sun`) | 2 | 0.003 | 天空的太阳像素 sprite，不是光照计算数据 | 运行时加载 | 拆出/替换，先做公共太阳 | Planets_16x16 随包许可限制原始资产再分发 | 补作者/购买来源；当前缺文件会 panic |
| FAST 噪声（两组） (`fast-noise`) | 131 | 8.041 | 余弦方向用于漫反射反弹与太阳盘采样；unit_vec3 已加载但未找到实际采样 | 运行时加载全部切片 | 保留公开；附 BSD-3-Clause 上游通知 | EA/SEED FastNoise；生成命令、固定审计版本、完整 BSD 通知和纹理哈希已补 | 历史生成器版本/seed 未记录，不宣称可逐字节重生成；未找到输出另有限制 |
| STBN 噪声（四组） (`stbn-noise`) | 259 | 12.245 | 标量用于阴影旋转/体积光抖动、2D 向量用于阴影 UV 抖动；另两组加载但未找到实际采样 | 运行时加载全部切片 | 暂保留待 NVIDIA 澄清，不认定必须拆 | 256 PNG 与固定版本 STBN.zip 字节一致；完整非商业/商业条款已保存 | 商业游戏及公开原始 PNG 分发权限仍需澄清；非 CC0，私有化也不自动解决 |
| README 宣传图 (`splash-promo`) | 1 | 1.377 | 根 README 宣传图，随发行包复制；不是原生启动屏贴图 | README 引用 / 发行包复制 | 保留公开：项目自有游戏截图 | 项目所有者确认由团队截取游戏运行画面；同目录来源说明和 SHA-256 已补 | 无待确认的制作来源；原始截图日期/构建版本未记录，不影响自有截图确认 |
| 原生游戏证据截图 (`native-evidence`) | 18 | 5.009 | 屋顶、模型缓存/量化、花朵和石材的原生游戏验收截图 | 文档证据，非游戏输入 | 保留文档，按所含素材核权 | 相关验收文档/屋顶 PoC 截图 | 截图中的第三方视觉素材不自动变成项目自有；无需当作运行资产分仓 |
| 抖动实验截图 (`dither-evidence`) | 49 | 12.842 | 抖动研究网页的原生 A/B 截图和控件证据，不是噪声纹理 | 研究网页/证据 | 保留文档 | evidence.json 记录真实 Release 截图来源和设置 | 注意截图所含第三方素材；不是运行时噪声贴图 |
| 石材 OBJ 导出样本 (`stone-obj`) | 8 | 0.046 | 固定 seed 的石块/石板 OBJ 样本；游戏调用 Rust 生成器而非这些文件 | 研究/制作输出，非游戏模型输入 | 保留公开，不因非运行输入而拆仓 | 项目 StoneSpec 程序生成 | 运行时使用 Rust 生成器而非读取这些 OBJ |
| 批准蝴蝶 Blender 源工程与导出 GLB (`butterfly-v5-source`) | 2 | 0.521 | 当前蝴蝶可编辑 Blender 工程及制作导出 GLB；正式产物另发布到 assets/models | 当前制作源，非游戏直接输入 | 保留公开，不因非运行输入而拆仓 | 项目自有 Blender v5 制作源；项目所有者已确认实验归属 | v2 调色板依赖已迁入 v5/palette.py；完整 Blender 重导出本次未执行 |
| 批准蝴蝶离线图集/截图 (`butterfly-v5-evidence`) | 8 | 0.140 | 当前 v5 的透明边界、循环、调色板和离线投影验证图集 | 离线制作/回归证据 | 保留公开，不因非运行输入而拆仓 | 项目自有 v5 制作流程派生输出 | 保留当前制作验证证据；不被游戏读取 |
| 内嵌字体的启动屏原型 HTML (`splash-html`) | 1 | 0.131 | 离线启动屏美术/动画原型，内嵌 Pixelify 字体 | 离线网页原型 | 保留工具，随附字体许可 | 项目生成网页；内嵌 Pixelify Sans 原字节 | 单独分发 HTML 时也须遵守字体 OFL；不只检查独立 TTF |
| 内嵌图片的性能/原理报告 (`evidence-html`) | 3 | 1.952 | 土壤、草像素化、DDGI 响应的离线图表与验收报告，含内嵌截图 | 文档/验收展示 | 保留文档，逐容器核对 | 报告内有运行观测/图表；按 HTML 容器列出 | 未逐张展开 data URI 做来源鉴定；不代表已全部授权 |

## 引用与来源证据索引

- **sprinkler-loop**：[assets/sfx/sprinkler/README.md](../../assets/sfx/sprinkler/README.md)；[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/audio/sprinkler_audio.rs](../../src/audio/sprinkler_audio.rs)；[src/sprinkler.rs](../../src/sprinkler.rs)
- **font**：[assets/font/README.md](../../assets/font/README.md)；[assets/font/OFL.txt](../../assets/font/OFL.txt)；[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)
- **hrtf-project**：[assets/hrtf/README.md](../../assets/hrtf/README.md)；[src/audio/spatial_sound_manager.rs](../../src/audio/spatial_sound_manager.rs)；[crates/petalsonic/src/spatial/processor.rs](../../crates/petalsonic/src/spatial/processor.rs)；[assets/hrtf/provenance.json](../../assets/hrtf/provenance.json)
- **hrtf-embedded**：[crates/petalsonic/src/spatial/processor.rs](../../crates/petalsonic/src/spatial/processor.rs)；[assets/hrtf/README.md](../../assets/hrtf/README.md)；[crates/petalsonic/asset/hrtf/README.md](../../crates/petalsonic/asset/hrtf/README.md)
- **model-butterfly**：[src/model_assets.rs](../../src/model_assets.rs)；[assets/models/README.md](../../assets/models/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)；[docs/assets/cleanup-unused.md](../../docs/assets/cleanup-unused.md)
- **model-leaf**：[src/model_assets.rs](../../src/model_assets.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-apple**：[src/tracer/apple_preview.rs](../../src/tracer/apple_preview.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-flower**：[src/flora/models.rs](../../src/flora/models.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **model-soil**：[src/soil_bag.rs](../../src/soil_bag.rs)；[src/tracer/model_geometry.rs](../../src/tracer/model_geometry.rs)；[build.rs](../../build.rs)；[assets/models/README.md](../../assets/models/README.md)
- **bundle-notes**：[assets/models/README.md](../../assets/models/README.md)；[assets/sfx/README.md](../../assets/sfx/README.md)
- **footsteps**：[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/audio/local_player_footsteps.rs](../../src/audio/local_player_footsteps.rs)；[assets/sfx/Footsteps SFX - Undergrowth & Leaves/README.md](../../assets/sfx/Footsteps%20SFX%20-%20Undergrowth%20%26%20Leaves/README.md)；[assets/sfx/Footsteps SFX - Undergrowth & Leaves/LICENSE.txt](../../assets/sfx/Footsteps%20SFX%20-%20Undergrowth%20%26%20Leaves/LICENSE.txt)
- **sound-armoury**：[src/audio/runtime_assets.rs](../../src/audio/runtime_assets.rs)；[src/app/core/input.rs](../../src/app/core/input.rs)；[assets/sfx/SOUND-ARMOURY.md](../../assets/sfx/SOUND-ARMOURY.md)；[assets/sfx/SOUND-ARMOURY-LICENSE.txt](../../assets/sfx/SOUND-ARMOURY-LICENSE.txt)；[src/audio/summer_cicadas.rs](../../src/audio/summer_cicadas.rs)；[assets/sfx/summer_cicadas/README.md](../../assets/sfx/summer_cicadas/README.md)；[assets/sfx/summer_cicadas/provenance.json](../../assets/sfx/summer_cicadas/provenance.json)；[assets/sfx/summer_cicadas/LICENSE.txt](../../assets/sfx/summer_cicadas/LICENSE.txt)
- **tool-icons**：[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)；[src/app/core/mod.rs](../../src/app/core/mod.rs)
- **farm-fallback**：[src/app/core/ui_style.rs](../../src/app/core/ui_style.rs)；[src/app/core/mod.rs](../../src/app/core/mod.rs)；[assets/texture/Pixel_Farming_Tools_IconSet_16px/README.txt](../../assets/texture/Pixel_Farming_Tools_IconSet_16px/README.txt)
- **sun**：[src/tracer/resources.rs](../../src/tracer/resources.rs)；[assets/texture/Planets_16x16/License.txt](../../assets/texture/Planets_16x16/License.txt)
- **fast-noise**：[assets/texture/noise/fast/README.md](../../assets/texture/noise/fast/README.md)；[src/tracer/resources.rs](../../src/tracer/resources.rs)；[assets/texture/noise/fast/LICENSE.txt](../../assets/texture/noise/fast/LICENSE.txt)；[shader/slang/tracer.slang](../../shader/slang/tracer.slang)
- **stbn-noise**：[assets/texture/noise/stbn/README.md](../../assets/texture/noise/stbn/README.md)；[src/tracer/resources.rs](../../src/tracer/resources.rs)；[assets/texture/noise/stbn/UPSTREAM-LICENSE.txt](../../assets/texture/noise/stbn/UPSTREAM-LICENSE.txt)；[shader/slang/tracer_shadowing.slang](../../shader/slang/tracer_shadowing.slang)；[shader/slang/god_ray.slang](../../shader/slang/god_ray.slang)
- **splash-promo**：[README.md](../../README.md)；[scripts/package_release.py](../../scripts/package_release.py)；[demo/img/README.md](../../demo/img/README.md)
- **native-evidence**：[docs/evidence/model-cache-restored/summary.json](../../docs/evidence/model-cache-restored/summary.json)；[docs/research/model-view-quantization.md](../../docs/research/model-view-quantization.md)；[docs/rooftop_poc.md](../../docs/rooftop_poc.md)；[docs/research/procedural-stone-models.md](../../docs/research/procedural-stone-models.md)
- **dither-evidence**：[docs/research/ordered-dither-assets/evidence.json](../../docs/research/ordered-dither-assets/evidence.json)；[docs/research/ordered-dither.md](../../docs/research/ordered-dither.md)；[docs/research/ordered-dither.html](../../docs/research/ordered-dither.html)
- **stone-obj**：[docs/research/procedural-stone-models.md](../../docs/research/procedural-stone-models.md)；[src/stone_models.rs](../../src/stone_models.rs)
- **butterfly-v5-source**：[experiments/butterfly-method-comparison/blender-v5/README.md](../../experiments/butterfly-method-comparison/blender-v5/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)；[assets/models/README.md](../../assets/models/README.md)；[docs/assets/cleanup-unused.md](../../docs/assets/cleanup-unused.md)
- **butterfly-v5-evidence**：[experiments/butterfly-method-comparison/blender-v5/README.md](../../experiments/butterfly-method-comparison/blender-v5/README.md)；[experiments/butterfly-method-comparison/blender-v5/source-manifest.json](../../experiments/butterfly-method-comparison/blender-v5/source-manifest.json)
- **splash-html**：[docs/research/splash-prototype.md](../../docs/research/splash-prototype.md)；[scripts/build-splash-prototype.mjs](../../scripts/build-splash-prototype.mjs)；[assets/font/OFL.txt](../../assets/font/OFL.txt)
- **evidence-html**：[docs/reports/soil-deposition.html](../../docs/reports/soil-deposition.html)；[docs/evidence/grass-pixelization-performance.html](../../docs/evidence/grass-pixelization-performance.html)；[docs/research/ddgi_response_explained.html](../../docs/research/ddgi_response_explained.html)
