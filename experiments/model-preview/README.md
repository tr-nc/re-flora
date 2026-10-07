# Re:Flora 模型像素化预览台

持续维护的美术工具，**唯一模型注册入口是 `models/index.js`**。不复制 HTML、不为新模型复制后处理。

```sh
node scripts/serve-model-preview.mjs
# http://127.0.0.1:8765/model-preview/
```

需要 Node 24、WebGL 2；Three.js 0.183.0 与许可证在本地 vendor，无远程资源、无需 npm 构建。不要直接用 `file://`。服务仅监听 127.0.0.1。

## 模型与观察

- 原有：`?model=leaf`、`butterfly`、`apple`。
- 参数化花型：`wild-geranium`、`forget-me-not`、`oxeye-daisy`、`cosmos`、`coneflower`、`tulip`、`white-geranium`、`gillenia`。
- 新候选：`four-petal`（四瓣蓝白）、`five-star`（五瓣尖星）、`custom-flower`（自定义花朵）。顶部下拉框也可选择。
- 仅网页动物：`bee`（蜜蜂）、`kingfisher`（三趾翠鸟）。原麻雀 / 燕子模型和入口已删除。
- 营养土袋网页候选：`soil-bag`（包装预设、换色、贴纸导入、撕开与倾倒动作）。
- `?model=star-strawberry` 旧书签仍规范化为 `gillenia`。
- `&variant=compare` / `model` / `pixel` 切换布局。

两个画布同步拖动／方向键旋转；左侧可滚轮检查缩放，右侧固定模型取景，不跟随左侧缩放。支持正交／透视、前／后／侧视角、线框、独立相机转台、透明 PNG。动画使用共用时间轴（播放、暂停、相位、逐帧、2–60 FPS）；静态花头不伪造动画。

**通用保守覆盖（关闭 = 旧八邻接补点）** 是后处理实时 A/B，默认开启。所有模型复用同一几何覆盖、分组连接与新增颜色采样。已有中心 RGBA 不被修复覆盖；保守覆盖会加粗细部，也可能合并亚像素缝隙。未增加时间滤波，动画／旋转仍可能跳像素。

## 统一花朵工作台

所有网页花型和六种游戏花现在都是 `assets/models/parametric-flower.mjs` 的数据预设，不再分别写 radial / tulip / Gillenia 造型分支。完整花头包含花瓣、花心、花萼，没有茎叶。

- 3–24 瓣；花头大小、瓣长、瓣宽。
- 瓣尖从圆到尖，以及瓣尖缺口。
- 同一个姿态量：负值下垂、0 平展、正值向前聚拢成杯状。
- 花心可选平花盘／圆球凸起／锥状凸起，调整宽度和凸起高度。
- 花头仰角。
- 四个 **palette A/B/C/D** 色槽，没有给花瓣某个细分部位添加专用颜色滑杆。

### 权重贴纸与渐变

`调色板权重贴纸` 控制颜色如何分布，palette 控制这些区域实际是什么颜色。

- RGB 存 A/B/C 的权重，D 为剩余权重，最后归一化。纯红／绿／蓝／黑代表 A/B/C/D；红绿过渡就是 A 与 B 混色，不是数字索引跳到另一种颜色。
- 混色在 **Linear-sRGB**，结果由共享标准材质参与源图、像素图和补点采色。比如 palette A 蓝、B 白，同一贴纸可表现瓣中蓝→白边；换 palette 不改贴纸或几何。
- 模板在共用源中程序化生成（根→尖、瓣中→白边、细脉、纯色）；页面只展示预设并允许换 palette，不提供贴纸编辑。
- 贴纸图集左 ¾：每一片花瓣共用的 UV，**上根下尖、左右瓣缘**。右上：花心；右下：花萼。四槽不固定绑定任何器官。
- 画笔、撤销、权重 PNG 导入／导出与贴纸编辑器已按用户要求删除；颜色预设只换 palette，不覆盖造型和预设贴纸。
- 参数仅在当前页面内存；切换模型／恢复默认会清除调整，不提供保存工程。像素结果的透明 PNG 截图导出仍保留。

右侧仍是**每完整花头 N×N → 固定 512×512 透明合成**，不是每瓣一张图。每头使用同相机裁切投影，GPU 深度／几何补点深度参与合成。形状改变更新包围球，转台和左侧缩放不逐帧重新撑满瓦片。

### 与游戏的关系

**六种游戏花现已迁移到同一来源**：`assets/models/flower-source.mjs` 保存正式预设，网页直接引用，不再另存一份相同形态。生成器和 palette 权重工具也已移入 `assets/models/`。`flowers.json` 发布同样的几何、UV 和生成颜色贴图；游戏保留自己的茎、种植、风、光照与像素缓存。勿忘草两端都用五瓣，不再借波斯菊八瓣形状。网页控件仍是会话内编辑，不自动覆盖游戏资产。新增四瓣／五瓣示例和白花／Gillenia 等仍仅网页，不扩游戏物种数。

郁金香是单轮聚拢花瓣的杯形近似，不等价于真实双轮交错花被／闭合花苞；重瓣、融合钟形花冠、兰科唇瓣等不硬塞进本轮径向单轮模型。以后可以在统一头模型接口下增加别的拓扑族，不必为颜色再开分支。当前游戏迁移与验证见 [共用花型报告](../../docs/evidence/unified-native-flowers.md)；此前网页阶段的迁移表见 [花朵工作台报告](../../docs/evidence/parametric-flower-studio.md)，权重取舍见 [贴图调研](../../docs/research/palette-weight-flower-textures.md)。

落叶／蝴蝶仍读取正式 GLB，苹果仍读取共用 `apple-source.mjs`；修改正式配方需遵守 [资产发布规则](../../assets/models/README.md)。网页灯光和尺寸不是游戏最终效果，也不是 Release 性能证据。

## 营养土袋 · 网页验收候选

打开 `http://127.0.0.1:8765/model-preview/?model=soil-bag`（端口可由 `--port` 改）。先选包装预设，再调袋色、充盈度、贴纸底色／内置图案色和大小。三种内置贴纸可直接切换，也可导入 PNG／JPG／WebP；图片保留比例，透明部分铺在贴纸底色上，仍是共享管线支持的不透明贴图，不增加 alpha/cutout 特例。文件限 8 MB、边长 8192。自定义图片不受内置图案色控制。

- **最近邻贴纸**：贴纸放大／缩小均用 Nearest，不生成 mipmap；导入缩放也关闭平滑。三种内置图案是大双叶芽／三层土／五瓣花，用连续贝塞尔曲线／椭圆在原始 512×640 贴图上绘制，只有底色、大图案与一条实心标题色块，没有任何包装文字。左侧不预先做马赛克或低分辨率放大；像素风仅由共享后处理生成。保留低分辨率轮廓，不靠微小细节；32/64/128² 有实际截图，极小尺寸不承诺全部可读。设计参考与取舍见 [低分辨率包装调研](../../docs/research/soil_bag_low_resolution_packaging.md)。
- **固定扁平侧面**：已按用户选择固定为原滑杆的 1，填满上下肩部、中段侧壁更平直；滑杆及其说明已移除。所有包装预设、重新加载／恢复默认和两个动作使用同一袋型，贴纸跟随袋面；充盈度仍可单独调厚薄。
- “撕开一个袋口”：从完整封口开始，只分开右上角的一处真实几何开口；左侧封口保留，撕条仍连接在袋上。
- “袋口朝下 · 倾倒营养土”：独立片段，从已开口姿态开始；开口法线朝下才显示粗略土流，土流从同一个实际袋口沿世界重力下落，再收回袋子，保留开口。
- 两个动作共用时间轴，播放一次后保持精确末帧，再点播放重播；相位／逐帧可检查。既有蝴蝶和动物片段仍默认循环。
- 土流只示意出土位置与方向，**没有土块粒子／碰撞／地面沉积动画**。贴纸色块没有库存或实际出土量含义。
- 袋子、贴纸和土流复用同一个 `pipeline.js` 与通用后处理 A/B。袋体一组、独立土流一组，不为袋子复制渲染器、相机或时钟。默认 64²、固定取景包含两个动作，不逐帧自动撑满画布。
- 调整与导入仅在页面内存，切换模型／恢复默认后清除；不恢复花朵贴纸编辑器，也不自动保存工程或发布资产。

**默认袋子已接入游戏 Soil 工具，网页改色/图片仍不自动改游戏资产。** 两端使用 `assets/models/soil-bag-source.mjs` 和连续矢量配方；原生显式发布 hover/pour 两个完整静态快照与 opaque RGB 贴纸，游戏仅实际撒土时显示固定倾倒姿态，悬停与松开都隐藏袋子，没有撕开/倾倒动画或 GLB morph 支持。简单落土方粒只作视觉效果，不改堆土、库存、肥力或出土节奏。网页历史验证见 [营养土袋网页验证](../../docs/evidence/web-preview-soil-bag.md)，实际游戏截图/检查见 [Soil 土袋试用](../../docs/evidence/soil-bag-native-tool.md)。

## 仅网页动物

现有一只蜜蜂和一只三趾翠鸟；原麻雀、燕子已移除，**小狗仍已取消**。均为本仓库原创程序化低模，无下载模型／贴图，无游戏注册或行为逻辑。

- 蜜蜂保持原样：七段交替条纹腹部、六足、双触角、两对不透明浅色翅。
- 三趾翠鸟（`Ceyx erithaca`）：按用户给出的侧面照片修正扁圆小头、向后展开的饱满身体、宽根橙红长喙、铜红顶冠与粉紫侧冠、柠檬黄胸腹、白喉 / 颈侧斑、蓝耳斑、炭灰翼底与不规则蓝覆羽斑、短尾和收拢弯趾。每足两趾向前、一趾向后。仍是原创低模，不是写实扫描；羽毛细密感和蓬松边缘仍需要人工审美验收。
- 头部 / 身体以独立侧面轮廓定义连续曲面；收拢翅膀与细分长羽映射到身体侧面的曲率。五张原创、不透明的程序羽色 / 喙色权重图以 Linear-sRGB 混合七个 palette 槽，三张细纹 bump 图只影响材质光照（原始与像素画布共享同一材质）；改色复用像素存储与 GPU 贴图，不重建几何。不是参考照片投影或 alpha 羽片。
- 两种时间轴片段：静止姿态／拍翼展示。共用暂停、播放、相位、逐帧；同一时刻绝对采样，不累计旋转、不另开时钟。拍翼仅旋转翼根，不展开真正的飞羽扇面，足仍保持停栖示意姿态；不是物理飞行。
- 调整模型大小、翅长、拍翼幅度与各部位配色；固定取景包含最大参数下的拍翼包围，不随姿态缩放。身体／左翅／右翅三个修复组。
- 蜜蜂默认 48²，三趾翠鸟默认 96²，可切 8–128²；32² / 64² 仍覆盖在回归检查中。新鸟完整拍翼包围比旧模型大，固定 span 为 6，不缩小最大参数来规避取景检查。共用现有后处理开关，不增加鸟专用补点。低分辨率下羽斑、眼高光和足趾仍会变粗、丢失或合并。

报告：[照片对照修正与当前验证](../../docs/evidence/kingfisher-reference-correction.md)；[首版三趾翠鸟记录（用户指出还原不足）](../../docs/evidence/web-preview-kingfisher.md)；[旧动物交付记录](../../docs/evidence/web-preview-animals.md)。

## 模块与接口

| 文件 | 职责 |
| --- | --- |
| `viewer.js` / HTML / CSS | 唯一模型切换、声明式控件、相机、时钟、导出入口 |
| `models/flower-catalog.mjs` | 直接引用游戏预设，补充仅网页的研究预设 |
| `assets/models/flower-source.mjs` | 六种正式花的共用形态／palette／贴纸预设 |
| `assets/models/parametric-flower.mjs` | 无 DOM / Three.js 的确定性几何、UV、参数边界 |
| `assets/models/palette-mask.mjs` | 两端共用的权重模板、验证、线性 palette 混色 |
| `models/flowers.js` | Three.js 花头适配；颜色更新不重建几何 |
| `models/animal-geometry.mjs` / `animals.js` | 动物目录、纯函数姿态与 Three.js 适配；仅网页 |
| `models/kingfisher-geometry.mjs` | 翠鸟的轮廓曲面、UV、闭合折翼 / 短尾和弯趾；无 DOM / Three.js |
| `models/kingfisher-plumage.mjs` | 原创羽色权重 / 细纹、线性 palette 解析与存储复用；不改几何 |
| `assets/models/soil-bag-source.mjs` / `models/soil-bag-geometry.mjs` / `soil-bag.js` | 共享袋型/绝对动作来源、兼容重导出与网页 Three.js 适配；原生发布静态快照 |
| `assets/models/soil-bag-sticker-art.mjs` / `models/soil-bag-sticker-art.mjs` / `soil-bag-stickers.js` | 共享连续矢量大图案、兼容重导出与网页原始尺寸 opaque 贴纸/最近邻导入；默认图发布原生 RGB |
| `pipeline.js` | 唯一 source/pixel/ID/高分辨率采色与资源生命周期 |
| `part-composite.js` / `part-depth.mjs` | 花头裁切、GPU 深度与透明合成，不另建 renderer |
| `geometry.js` / `connectivity.mjs` / `postprocess.mjs` | 当前姿态几何投影、分组连通与通用修复 |
| `timeline.mjs` | 共用静态／循环／一次性动作的时间采样与逐帧 |

新模型定义注册到 `models/index.js`，提供：

- `id`、`label`、`defaults`、`controls`（range / checkbox / color / select / note / image）、`preview`。
- 可选 `colorPresets: [{name,colors}]`（仍仅覆盖颜色），或 `presets: [{name,values}]` / `presetLabel`（覆盖明确的模型参数）。
- `image` 控件调用模型的异步 `importImage(key,file)`，返回要合并的参数或 `null`；导入失败显示错误，模型切换后不应用旧结果。模型拥有并释放图片资源。
- `clips` 可声明 `loop:false` 来保持精确末帧；未声明时沿用循环。可选 `stateDescription` / `reviewState()` 提供当前姿态说明与浏览器验收数据。
- `async create()` → `scene`、`meshes`、固定 `view`、`clips`、`apply(settings)`、可跳到任意时刻的 `sample(time,clip)`、`preparePass`、`shadows`、`description`、`dispose()`。
- `repairGroups: [{id,label,meshes}]`；非零唯一 ID，一个 mesh 只归一组，不能把不应相连的对象硬缝起来。
- 花头可提供 `pixelParts: [{id,label,meshes,center,span,anchor}]`；完整花头与同 ID repair group 匹配，center/span 是旋转安全固定包围，不另建姿态或相机权威。

模型不能创建 renderer、OrbitControls、requestAnimationFrame 或专属后处理。材质允许不透明贴图；alpha cutout／透明几何不是当前 ID 与覆盖契约，不能当作自动支持。

## 验证

```sh
node --test experiments/model-preview/tests/*.test.mjs
# 既有 Playwright 安装；必要时设置 NODE_PATH
node experiments/model-preview/tests/browser.cjs
node experiments/model-preview/tests/flowers-browser.cjs
node experiments/model-preview/tests/head-platform-browser.cjs
node experiments/model-preview/tests/animals-browser.cjs
# 需要 agent-browser；独立本地服务／私有浏览器会话，结束时自动关闭
node experiments/model-preview/tests/soil-bag-browser.mjs
# 默认 headless 若出现既有上下文丢失，可单独验证窗口渲染路径（自动私有 Xvfb）
env -u DISPLAY -u WAYLAND_DISPLAY AGENT_BROWSER_HEADED=true \
  node experiments/model-preview/tests/soil-bag-browser.mjs
```

支持 `CHROME_EXECUTABLE` / `PREVIEW_ARTIFACT_DIR`。脚本自行创建／关闭服务与 headless Chrome；不启动游戏，不保存 GUI 设置。

完整 Node 套件：56 个语法／纯函数／资产回归测试（含翠鸟与共享土袋快照发布）。新花型浏览器验证；11 种花、242 个头瓦片姿态（两种投影、三个视向、8/32/64/128px）、原始 RGBA 保留、GPU 深度、8 个遮挡夹具；确认花型没有贴纸编辑／导入／保存控件，palette/预设贴图不重建网格、重置／资源释放、390px 移动布局。结果与图片在 `target/flower-studio-review/`。当前动物验证 89 个姿态、32,429 个原始颜色样本不被覆盖、动作与固定边界／资源释放／透明 PNG／移动布局，默认产物在 `target/animal-preview-review/`（本次修正在 `target/kingfisher-correction/browser/`）；此前75姿态/19,614样本是旧动物的历史记录。营养土袋本轮验证 42 个网页状态（窗口渲染／私有虚拟显示），覆盖原始尺寸连续矢量贴纸／窄边缘抗锯齿／无预制马赛克、真实 Nearest／无 mipmap 材质与不混色的图片缩放、32/64/128² 同色大图案、固定扁平袋型与已移除的滑杆、真实 HEX 改色、贴纸导入与失败恢复、末帧／重播／奇数 FPS 逐帧、重力流向、共享 A/B／分辨率／视向、三轮旧模型切换与资源计数、390px 移动布局。新原始贴纸、对照截图和 QA JSON 在 `target/soil-bag-vector-source-window-review/`；此前版本的 45 状态通过、108 组固定袋型几何对照以及窗口刷新／headless 上下文失败记录保留在验收报告，没有宣称已修复整个浏览器环境。不提交二进制截图。游戏的性能和美术接受需另行验证。
