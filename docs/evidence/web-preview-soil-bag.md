# 营养土袋：网页验收候选

## 范围

本页保留网页候选各轮的历史验收记录；截至 `018bff82` 当时尚未替换游戏 Soil，也没有相关 Rust/shader/正式资产变更。随后按用户要求接入共享静态姿态与简单视觉落土颗粒，当前原生交付边界和实际游戏验证见 [Soil 土袋试用](soil-bag-native-tool.md)。网页编辑仍不会自动发布游戏资产。

在现有 [模型预览台](../../experiments/model-preview/README.md) 注册 `soil-bag`，复用苹果／小花使用的 `pipeline.js`、几何投影、保守覆盖、八邻接修复和新增像素采色。没有袋子专用后处理、相机、renderer 或动画循环。

候选包含柔软鼓起的袋体、封口、正面贴纸、单一角部开口、连接着的撕条、内衬与袋内土。三种包装预设可以继续修改袋色、充盈度、贴纸底色／内置图案色／大小。贴纸可换内置图案或上传 PNG／JPG／WebP；自定义图保留比例与原色，透明部分铺在贴纸底色上，不突破共享不透明材质契约。参数与图片只留在本次页面，不自动写入游戏资产或恢复花朵的旧贴纸编辑器。

## 动作

- **撕开一个袋口（1.65 s）**：完整封口 → 仅右上角袋口分开 → 撕条抬起，左侧封口保留。
- **倾倒营养土（3.8 s）**：独立片段从已开口袋子开始，袋口法线转向下；粗略土流从同一世界位置沿重力落下，随后收回袋子，末帧保留开口。
- 两个一次性动作在共用时间轴声明 `loop:false`。末帧不绕回封口；播放重播、相位、逐帧检查均走同一时间采样。蝴蝶／动物默认循环不变。
- 土流仅是位置／方向示意，不是土块粒子、落地碰撞或沉积系统；当前贴纸色块没有库存或出土量含义。

## 原始候选验证（2cecc148）

```sh
node --test experiments/model-preview/tests/*.test.mjs
node experiments/model-preview/tests/soil-bag-browser.mjs
```

- **42/42** Node 测试通过（新增 8 项）：浏览器适配／控件语法守卫、确定性／有限且非退化的几何、固定拓扑、单一开口、绝对姿态与精确末帧、世界重力、固定取景、完整预设、共用一次性／循环时钟。
- 独立 `agent-browser` 浏览器验收 **32 个状态**通过：实际 HEX 控件改袋色并改变像素结果；内置贴纸切换；透明 PNG 导入改变贴纸但不改变袋体覆盖；完整预设同步颜色与参数控件；错误文件导入后可恢复。
- 动作验收：撕开末帧保持一个开口、没有土流；7 FPS 前后逐帧不漏末前帧／不绕回；末帧重播从头开始；倾倒中口法线 Y < −0.7 且显示土流，结束停止流动并保留开口。源图与像素图时间相同。
- 共享 A/B 两种模式、16/32/64/128²、正交／透视、后／侧视角；已有中心 RGBA 均不被新增修复覆盖，透明背景保留，相机方向同步。
- 三轮苹果／蝴蝶／小花／袋子切换通过；花头仍使用独立 512² 合成，蝴蝶仍循环；袋子重新加载后的几何／纹理计数不递增，自定义图片清除。
- **1440×1000** 与 **390×844** 无页面横向溢出。通过的完整运行中浏览器错误为空。
- 额外重复启动独立 headless SwiftShader 检查出现间歇性 WebGL 上下文丢失，测试如实终止；最近一次重复验证被此问题阻断。单独打开既有苹果模型（未创建袋体）也复现同样的上下文丢失，因此不能把它当作袋子资源故障或声称已修复整个浏览器环境。已有多次完整 32 状态通过记录；没有跳过断言、自动重试到绿或加入渲染特例。正常用户浏览器仍需实际验收。

浏览器测试拥有独立临时 HTTP 服务、PID 隔离会话，结束后关闭两者；它不启动游戏、不保存 GUI 配置。上传自动化使用绝对文件路径。

另外通过 `cargo fmt --check`、Release 构建与普通隐藏静音启动；原生日志显示首帧就绪、成功退出，无 ERROR／panic／VUID，仅保留既有 fruit-physics hitch 警告。没有生成文件变化，用户本地 `config/gui.toml`（spread=13）未纳入提交。

产物（不提交二进制截图）：

- `target/soil-bag-preview-review/soil-bag-preview-qa.json`
- `target/soil-bag-preview-review/soil-bag-preview-{closed,opened,pouring,custom-sticker,mobile}.png`

## 侧面扁平度更新

新增声明式滑杆 `sideFlatness`（0–1，步长 0.01，默认 0），放在充盈度旁边；0 保持原来的圆弧侧面，1 增加上下肩部厚度，让中段轮廓更平直。它不缩放整个袋子，也不改变中间峰值厚度、袋宽／袋高、两处封口和单一袋口；贴纸使用同一表面配方同步变形。包装预设／恢复默认仍回到 0，没有游戏或保存规则改变。

- 与变更前 `2cecc148` 的原配方比较 **72 个输入组合**，`sideFlatness=0` 的全部位置／索引／UV／袋口信息精确相等。
- 默认袋体在 y=±0.615 的侧面厚度约 **0.229 → 0.348**，中间 y=0 的 **0.411** 不变；填肩而不是压薄。两端封口保持相同位置。
- **44/44** Node 测试通过：新增形态／连续插值与边界守卫；原有有限非退化三角形、固定拓扑与动作取景测试覆盖扁平度 0／0.5／1 和充盈度极值。
- **36 个网页状态**在窗口渲染／自动私有虚拟显示中完整通过；实际滑杆改变侧面原模型与共享像素结果，还原 0 后像素 hash 精确回到原值，几何／纹理计数不增加；倾倒时袋口位置／法线、土流开关保持一致。旧模型切换、RGBA 保留、移动布局等断言继续通过。
- 默认 headless SwiftShader 仍复现上轮已记录的上下文丢失；额外 headless GL 诊断不能创建上下文。二者没有被算作通过，没有更改应用渲染或弱化断言。窗口渲染验证命令：

  ```sh
  env -u DISPLAY -u WAYLAND_DISPLAY AGENT_BROWSER_HEADED=true \
    PREVIEW_ARTIFACT_DIR=target/soil-bag-side-profile-window-review \
    node experiments/model-preview/tests/soil-bag-browser.mjs
  ```

产物位于 `target/soil-bag-side-profile-window-review/`，含 `soil-bag-preview-side-{round,flat}.png` 与扩展的 QA JSON；不提交截图。

## Nearest 采样与低分辨率贴纸更新

内置贴纸改成原创的双叶芽／三层土／五瓣花大轮廓，每张仅保留主体与一条实心标题色块；移除此前所有包装字样（含“2 L”）、细边框／根系和淡色细节。32×40 二值配方在现有 512×640 画布中用整数矩形绘制，保留底色与图案色控件，不增加字体／图片依赖。参考了 Miracle-Gro／COMPO SANA／Westland 官方包装的视觉层级，不使用其商标、照片或包装资产；来源与取舍见 [调研](../research/soil_bag_low_resolution_packaging.md)。

实际贴纸材质设置 `minFilter = magFilter = NearestFilter`、`generateMipmaps = false`；Canvas 图片缩放也关闭 `imageSmoothingEnabled`，避免 GPU 最近邻之前就已经混糊。透明图仍正常铺到底色上。袋体纸纹、共享采色／修复、形态／动作和包装预设不变，没有游戏或配置变更。

- 新的绘图守卫首先在旧代码上复现 **6 次文字绘制 != 0** 与 **imageSmoothingEnabled=true != false**，修改后通过。
- **48/48** Node 测试通过；新增 4 项涵盖确定性二值大轮廓／实心色块、8×10 中心缩小下的形态差异、无字体／细线／分数边缘，以及等比最近邻图片导入。
- **45 个网页状态**通过（窗口渲染／私有虚拟显示）：所有袋子状态检查真实纹理的 min／mag／mipmap 与 Canvas 平滑设置；真实 Canvas 输出验证内置图案只有两种颜色、全部 alpha=255；8×8 透明测试图缩放后只出现原图颜色与底色，没有插值色。
- 在同一袋色／底色／图案色／几何下，三种图案的 **32／64／128²** 输出均不同，袋体 alpha 覆盖和几何／纹理数量相同；检查实际 32／64² 截图，主体明显大于旧版微小字。hash 差异是渲染回归证据，不是最终人类辨认或用户美术批准。
- 侧面扁平度还原、贴纸导入／错误恢复、两个动作、一次性时钟、旧模型循环与资源释放、共享两种覆盖模式、RGBA 保留和移动布局均继续通过，浏览器错误为空。
- `cargo fmt --check`、`cargo check`、Release 构建和普通隐藏静音启动通过；新日志首帧就绪、成功退出，无 ERROR／panic／VUID，仍有既有 fruit-physics hitch 警告。没有生成文件变化，用户本地 GUI 配置逐字保持不变。
- 本轮沿用上节已成功的窗口渲染验证路径，没有宣称此前 headless SwiftShader 上下文丢失已修复，也没有自动重试到绿或关闭断言。Nearest 仍可能在旋转／极小分辨率下跳像素，不能恢复不存在的像素。

```sh
env -u DISPLAY -u WAYLAND_DISPLAY AGENT_BROWSER_HEADED=true \
  PREVIEW_ARTIFACT_DIR=target/soil-bag-sticker-window-review \
  node experiments/model-preview/tests/soil-bag-browser.mjs
```

本轮截图／QA JSON 在 `target/soil-bag-sticker-window-review/`，包括 `soil-bag-preview-sticker-{sprout,layers,flower}-{32,64,128}.png`、原始透明像素 PNG 和全部原有动作／布局截图，不提交图片。

## 固定扁平侧面并移除控件

按用户选择，页面袋型固定为此前 `sideFlatness=1`：几何配方默认 1，页面默认值／包装预设不再包含该字段，运行时不再从页面参数读取它；滑杆与调节说明一起移除。充盈度、Nearest 简洁贴纸与两个动作保持原有控制方式。纯几何接口保留显式形态输入，仅供已有形态／边界回归，不是隐藏页面设置。

- 新的默认／预设守卫在旧代码上先复现字段仍存在（`true != false`），修改后 **48/48** Node 测试通过。
- 与变更前 `0d09ad1a` 的显式 `sideFlatness=1` 对照 **108 组**开口／充盈度／贴纸大小／余土输入，全部位置、索引、UV、袋口与撕条信息精确相等，不重新设计用户选中的袋型。
- 本轮窗口渲染全流程通过前三种图案的 32／64／128² 断言，包括无滑杆／无页面字段与真实 Nearest 材质检查；随后切换侧面时刷新等待超时，未产生完整通过的 QA JSON。诊断时页面 visible、WebGL 上下文未丢失、无浏览器错误，但 `dirty=true`、相机暂未同步，独立 `requestAnimationFrame` 也超时；截图请求后能观察到正确扁平侧面与已移除的控件。
- 在同一私有窗口中切到未修改的苹果，侧面切换复现相同刷新等待超时；激活标签也未解决。因此没有修改共享时钟／渲染代码或将截图强制刷新加入测试来规避断言。此自动化窗口刷新问题仍未修复，不以该诊断冒充完整网页验收。
- 另外独立检查默认 headless 路径，再次出现此前记录的 `THREE.WebGLRenderer: Context Lost.`（随后 Restored）；应用明确报告上下文丢失，全流程同样未通过。两种路径的失败均保留，没有重试到绿。
- `cargo fmt --check`、`cargo check`、Release 构建和普通隐藏静音启动通过；首帧就绪、成功退出，无 ERROR／panic／VUID，仍有既有 fruit-physics hitch 警告。没有 Rust、shader 或生成文件变化，用户 GUI 配置逐字保持不变。

部分窗口产物在 `target/soil-bag-fixed-profile-window-review/`，最小侧面诊断图为 `target/soil-bag-fixed-minimal-side.png`；独立 headless 产物在 `target/soil-bag-fixed-profile-headless-review/`。这些不是全流程通过记录，不提交图片；此前贴纸更新的 45 状态通过记录仍属于上一版本。

## 平滑原始贴纸，只由共享后处理像素化

按用户要求移除原始贴纸的预制马赛克：`soil-bag-sticker-art.mjs` 改为连续的贝塞尔曲线／椭圆配方，80×100 仅是逻辑坐标，直接缩放到现有 512×640 原始贴图绘制，不生成 32×40 二值 mask、不把低分辨率图再最近邻放大。双叶芽、波浪土层和五瓣花保持大轮廓、实心色块与无文字风格。左侧原模型显示平滑轮廓；右侧像素风仍完全由同一个 `pipeline.js` 生成，没有第二套后处理。

贴图 min／mag 仍为 Nearest，无 mipmap，自定义图片缩放仍关闭平滑；正常 Canvas 曲线边缘抗锯齿与图片 Linear 过滤不是同一件事。轮廓覆盖先正常合成到贴纸底色，整体 alpha=255，不增加透明材质例外。固定扁平袋型、其他控件与动作不变。

- 绘图守卫先在旧格子实现上复现失败：`source silhouettes must use continuous curves before shared pixel processing`；修改后 **48/48** Node 测试通过。测试采样轮廓验证有限／确定性和低分辨率区分，但生产绘图没有采样网格。
- **42 个完整网页状态**本轮首次窗口渲染运行通过：实际 Canvas 原始轮廓存在窄边缘抗锯齿（非主色像素低于原始贴图的 2%），全部不透明，而且不由恒定 16×16 块组成。自定义 8×8 透明图等比缩放仍只有原图颜色与底色，真实 min／mag／mipmap 设置保持正确。
- 同色三图案 32／64／128²、固定侧面与已移除滑杆、实际 HEX 改色、预设同步、图片导入／错误恢复、撕开与倾倒、7 FPS 精确末帧／逐帧、源／像素时间同步、共享两种覆盖模式／RGBA 保留、三个旧模型切换周期／稳定资源计数和移动布局均通过，浏览器错误为空。
- 查看 512×640 原始图案以及双视图截图，叶片／波浪土层／花瓣在左侧是平滑大图案、右侧仍是像素结果；没有通过修改模型取景或共享修复来掩盖原始马赛克。
- 这次通过记录不表示上一轮窗口刷新停滞或既有 headless 上下文丢失已修复，没有重试到绿、截图强制刷新或应用时钟改动。
- `cargo fmt --check`、`cargo check`、Release 构建和隐藏静音启动通过；首帧就绪、成功退出，无 ERROR／panic／VUID，保留既有 fruit-physics hitch 警告。没有 Rust、shader 或生成文件变化，用户 GUI 配置逐字保持不变。

产物在 `target/soil-bag-vector-source-window-review/`：`soil-bag-sticker-source-{sprout,layers,flower}.png` 为原始贴图，`soil-bag-preview-sticker-*-{32,64,128}.png` 为实际双视图，另有动作／移动截图和本轮通过的 QA JSON；不提交图片。

用户表示这一处调整之后接近游戏接入阶段；本提交只完成原始图案修正与候选验证，没有偷偷替换游戏 Soil 或发布未经适配的动态袋口资产。

## 网页验收边界与后续原生试用

上述记录属于截至 `018bff82` 的网页阶段，没有宣称已完成原生接入或满足游戏性能预算。网页灯光不等于游戏光照，浏览器测试不是 Release 性能证据。

后续 `203078a8` 将几何/连续矢量配方移到正式共享来源，发布两个完整静态快照和 opaque RGB 贴纸；游戏按用户要求直接切换悬停/倾倒姿态，不播放动画。动态嘴部 morph 仍不属于 GLB/TRS 加载器支持范围。简单视觉方粒已接入共享地形粒子路径，不改变每体素堆土与材料账务；详细土块模拟仍未实现。实际原生验证、截图和手动试用说明见 [Soil 土袋试用](soil-bag-native-tool.md)。
