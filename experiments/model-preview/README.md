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
- 仅网页动物：`bee`（蜜蜂）、`sparrow`（圆身麻雀风格）、`swallow`（长尾燕子风格）。
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

## 仅网页动物

新增一只蜜蜂和两种小鸟；**小狗已取消**。均为本仓库原创程序化低模，无下载模型／贴图，无游戏注册或行为逻辑。

- 蜜蜂：七段交替条纹腹部、六足、双触角、两对翅。翅膀故意使用不透明浅色几何，不假装支持 alpha 透明覆盖。
- 麻雀风格：短喙、圆胸、棕色翼斑、短扇尾；燕子风格：较细深蓝身体、浅胸赤褐喉、尖翼、分叉长尾，不只是同一个鸟换色。
- 两种时间轴片段：静止姿态／拍翼展示。共用暂停、播放、相位、逐帧；同一时刻绝对采样，不累计旋转、不在模型里再开时钟。拍翼不是物理飞行，足仍保持示意姿态。
- 调整模型大小、翅长、拍翼幅度与 palette；固定取景包含最大参数下的拍翼包围，不随姿态缩放。身体／左翅／右翅三个修复组。
- 默认 48²，可切 8–256²。极低分辨率的燕子可能完全错过中心采样，需保守覆盖保留轮廓；细足／眼高光／尾叉会变粗、丢失或合并，没有模型专用例外。

报告：[动物网页验证](../../docs/evidence/web-preview-animals.md)。

## 模块与接口

| 文件 | 职责 |
| --- | --- |
| `viewer.js` / HTML / CSS | 唯一模型切换、声明式控件、相机、时钟、导出入口 |
| `models/flower-catalog.mjs` | 直接引用游戏预设，补充仅网页的研究预设 |
| `assets/models/flower-source.mjs` | 六种正式花的共用形态／palette／贴纸预设 |
| `assets/models/parametric-flower.mjs` | 无 DOM / Three.js 的确定性几何、UV、参数边界 |
| `assets/models/palette-mask.mjs` | 两端共用的权重模板、验证、线性 palette 混色 |
| `models/flowers.js` | Three.js 花头适配；颜色更新不重建几何 |
| `models/animal-geometry.mjs` / `animals.js` | 纯函数动物几何／姿态与 Three.js 适配；仅网页 |
| `pipeline.js` | 唯一 source/pixel/ID/高分辨率采色与资源生命周期 |
| `part-composite.js` / `part-depth.mjs` | 花头裁切、GPU 深度与透明合成，不另建 renderer |
| `geometry.js` / `connectivity.mjs` / `postprocess.mjs` | 当前姿态几何投影、分组连通与通用修复 |
| `timeline.mjs` | 共用静态／动画时间采样 |

新模型定义注册到 `models/index.js`，提供：

- `id`、`label`、`defaults`、`controls`（range / checkbox / color / select / note）、`preview`。
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
```

支持 `CHROME_EXECUTABLE` / `PREVIEW_ARTIFACT_DIR`。脚本自行创建／关闭服务与 headless Chrome；不启动游戏，不保存 GUI 设置。

完整 Node 套件：34 个纯函数／资产回归测试。新花型浏览器验证；11 种花、242 个头瓦片姿态（两种投影、三个视向、8/32/64/128px）、原始 RGBA 保留、GPU 深度、8 个遮挡夹具；确认没有贴纸编辑／导入／保存控件，palette/预设贴图不重建网格、重置／资源释放、390px 移动布局。结果与图片在 `target/flower-studio-review/`。动物额外验证 75 个姿态、19,614 个原始颜色样本不被覆盖、动作与固定边界／资源释放／透明 PNG／移动布局，产物在 `target/animal-preview-review/`。不提交二进制截图。游戏的性能和美术接受需另行验证。
