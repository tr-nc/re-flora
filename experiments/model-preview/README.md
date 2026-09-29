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
- `?model=star-strawberry` 旧书签仍规范化为 `gillenia`。
- `&variant=compare` / `model` / `pixel` 切换布局。

两个画布同步拖动／方向键旋转；左侧可滚轮检查缩放，右侧固定模型取景，不跟随左侧缩放。支持正交／透视、前／后／侧视角、线框、独立相机转台、透明 PNG。动画使用共用时间轴（播放、暂停、相位、逐帧、2–60 FPS）；静态花头不伪造动画。

**通用保守覆盖（关闭 = 旧八邻接补点）** 是后处理实时 A/B，默认开启。所有模型复用同一几何覆盖、分组连接与新增颜色采样。已有中心 RGBA 不被修复覆盖；保守覆盖会加粗细部，也可能合并亚像素缝隙。未增加时间滤波，动画／旋转仍可能跳像素。

## 统一花朵工作台

所有网页花型现在都是 `parametric-flower.mjs` 的数据预设，不再分别写 radial / tulip / Gillenia 造型分支。完整花头包含花瓣、花心、花萼，没有茎叶。

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
- 提供“瓣根→瓣尖”“瓣中→白边”“细脉”“纯色”四个模板，可自由画 A/B/C/D、调画笔、撤销，切换查看最终配色或原始权重。
- 贴纸图集左 ¾：每一片花瓣共用的 UV，**上根下尖、左右瓣缘**。右上：花心；右下：花萼。四槽不固定绑定任何器官。
- 可导出／导入 **不透明权重 PNG**（32–512px 宽高、≤4 MiB）。alpha 不参与权重；透明／越界图片拒绝且不破坏当前贴纸。导入不上传网络。建议使用本工具导出的无额外色彩配置权重图，避免图像编辑器对数据通道作颜色管理。
- 画布可拖动绘制，也可方向键移动光标、空格绘制。颜色预设只换 palette，不覆盖造型和贴纸。
- 参数仅在当前页面内存；切换模型／恢复默认会清除编辑。PNG 导出可保存权重，但不是完整形态工程文件。

右侧仍是**每完整花头 N×N → 固定 512×512 透明合成**，不是每瓣一张图。每头使用同相机裁切投影，GPU 深度／几何补点深度参与合成。形状改变更新包围球，转台和左侧缩放不逐帧重新撑满瓦片。

### 与游戏的关系

这轮统一花型和贴纸是**网页候选**。六种已发布游戏花的 `assets/models/flower-source.mjs` / `flowers.json` 未改，也未把新贴图编码偷偷塞进只支持四种固定材质角色的 native cache。原网页预设保留名字／颜色风格，但不是旧几何的逐顶点复刻；勿忘草网页恢复五瓣，而游戏仍保留之前批准的共用八瓣缓存形状。

郁金香是单轮聚拢花瓣的杯形近似，不等价于真实双轮交错花被／闭合花苞；重瓣、融合钟形花冠、兰科唇瓣等不硬塞进本轮径向单轮模型。以后可以在统一头模型接口下增加别的拓扑族，不必为颜色再开分支。迁移表、权重取舍与限制见 [花朵工作台报告](../../docs/evidence/parametric-flower-studio.md) 和 [权重贴图调研](../../docs/research/palette-weight-flower-textures.md)。

落叶／蝴蝶仍读取正式 GLB，苹果仍读取共用 `apple-source.mjs`；修改正式配方需遵守 [资产发布规则](../../assets/models/README.md)。网页灯光和尺寸不是游戏最终效果，也不是 Release 性能证据。

## 模块与接口

| 文件 | 职责 |
| --- | --- |
| `viewer.js` / HTML / CSS | 唯一模型切换、声明式控件、相机、时钟、导出入口 |
| `weight-map-editor.js` | 通用权重贴纸控件，绘制／撤销／有界 PNG I/O，不参与渲染 |
| `models/flower-catalog.mjs` | 统一花型预设数据；不发布游戏资产 |
| `models/parametric-flower.mjs` | 无 DOM / Three.js 的确定性几何、UV、参数边界 |
| `models/palette-mask.mjs` | 纯函数权重模板、绘制、验证、线性 palette 混色 |
| `models/flowers.js` | Three.js 花头适配；颜色更新不重建几何 |
| `pipeline.js` | 唯一 source/pixel/ID/高分辨率采色与资源生命周期 |
| `part-composite.js` / `part-depth.mjs` | 花头裁切、GPU 深度与透明合成，不另建 renderer |
| `geometry.js` / `connectivity.mjs` / `postprocess.mjs` | 当前姿态几何投影、分组连通与通用修复 |
| `timeline.mjs` | 共用静态／动画时间采样 |

新模型定义注册到 `models/index.js`，提供：

- `id`、`label`、`defaults`、`controls`（range / checkbox / color / select / weight-map / note）、`preview`。
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
```

支持 `CHROME_EXECUTABLE` / `PREVIEW_ARTIFACT_DIR`。脚本自行创建／关闭服务与 headless Chrome；不启动游戏，不保存 GUI 设置。

新花型验证：32 个纯函数／资产回归测试；11 种花、242 个头瓦片姿态（两种投影、三个视向、8/32/64/128px）、原始 RGBA 保留、GPU 深度、8 个遮挡夹具；贴纸真实绘制／撤销／PNG 往返／透明拒绝、palette/贴图不重建网格、重置／资源释放、390px 移动布局。结果与图片在 `target/flower-studio-review/`，不提交二进制截图。游戏的性能和美术接受需另行验证。
