# 三趾翠鸟：用户照片对照修正

## 为什么重做

用户指出首版（`6f7e8bc7`）神韵、比例、颜色都偏离参考。实际打开工作树的网页 demo，以侧面原始画布与用户的 `pi-clipboard-1aefd626-8ecd-4359-bbc8-ec6640b36afd.png` 并排检查，问题成立：

- 圆头过大，紫色像整顶头盔；身体比较直立。
- 喙像细针，翼面狭窄，尾 / 足暴露过长。
- 紫粉与黄色在侧光下变为暗紫 / 赭色，蓝覆羽是硬色块。

前次测试验证了不透明几何、像素覆盖、控制与资源契约，**没有证明与照片相似**。此轮保留截图做人工对照，不以测试通过替代美术验收。

## 当前修改

- `models/kingfisher-geometry.mjs`：独立头 / 身体侧轮廓，扁圆冠顶、后倾而饱满的身体、宽根扁平喙、短尾、收拢的连续曲线足趾。34 个网格、8,086 个三角形。
- `models/kingfisher-plumage.mjs`：原创程序羽色权重与细纹；五张不透明颜色贴图、三张细纹 bump 图。铜红冠顶上的粉紫羽区、暖脸与肩背、柠檬黄腹、不规则蓝耳斑 / 颈白斑、炭灰底色上的短蓝覆羽斑。没有下载贴图，也没有将照片投影到模型。
- `models/animals.js`：消费 UV / appearance，Linear-sRGB 混合原有七个颜色控制，改色复用数据与 GPU 资源。靠近观察面的中性主光让亮黄 / 橙红 / 粉紫不再整体发暗。
- `animal-geometry.mjs` 删除被替换的首版鸟几何，转用新配方；蜜蜂几何 / 姿态 SHA-256 与修改前一致。
- 共用 renderer、时间轴、三个修复组和“通用保守覆盖”A/B 开关未改。没有鸟专用补点或新 HTML 入口，没有 native 动物整合。

### 用照片约束比例，不用错误指标强行过关

按参考侧照的眼、胸腹轮廓与身体质量中心定位，建模坐标 +Z 指向喙。头 / 身体高度比从约 **0.702 → 0.482**；侧面眼至身体投影面积中心的后倾斜率从 **0.615 → 0.744**，照片目测地标约 **0.74**。

最初尝试用胸到腹的前缘斜率约束后倾是不合适的：参考胸部本来就比较直且丰满，该指标会鼓励过瘦的身体。改为 YZ 投影面积加权中心，避免顶点密度影响测量；照片前缘另用于加宽腹部。纯函数比例检查是防回退护栏，不是鸟类测量或自动相似度评分。

### 取景与分辨率

新翼形在最大模型大小 / 翅长 / 拍翼幅度下，原 `span:4.8` 取景失败：半幅 2.4，而最大半径 **2.915514**。没有缩小动作或删除断言；最终固定 `span:6`、target `[0,.02,.18]`、offset `[10,.65,2.1]`，半幅 3。默认鸟画布改为 **96²** 保留新的羽斑细节；32² / 64² 仍可选并包含在回归中。蜜蜂保持 48²。取景不随动画伸缩。

## 实际验证

产物均在当前工作树 `target/kingfisher-correction/`，截图不提交：

```sh
node --test experiments/model-preview/tests/*.test.mjs
NODE_PATH=/home/terence/code/linky/node_modules \
  PREVIEW_ARTIFACT_DIR="$PWD/target/kingfisher-correction/browser" \
  node experiments/model-preview/tests/animals-browser.cjs
cargo fmt --check
cargo check
cargo build --release
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

- Node：**40/40**。新增照片比例护栏，以及权重确定性、归一化、七色槽覆盖、全不透明、像素复用与 palette 往返检查。
- 浏览器：两种动物、**89 个姿态，32,429 个原始 RGBA 样本全保留**，centerMisses `[]`，页面 / 控制台 / 请求错误 `[]`。覆盖 32 / 64 / 默认分辨率，正交 / 透视与三视向、后处理 A/B、尺寸 / 翅长 / 幅度边界、改色、GPU 资源复用 / 切换释放、PNG 与移动布局。
- 亲自检查侧面照片对照及当前默认 / 背面 / 拍翼截图。`before-comparison.png` 是首版问题；最终 `final-comparison.png` 是同尺度参考 / 修正版，`reference-before-after.png` 加上首版方便检查改动，`final-demo.png` 是网页原始 / 像素画布。
- `cargo check` 与最终 Release 构建成功（冷缓存首次组合调用碰到工具总时限，重新执行后完成；不是未完成的构建）。工作树本地隐藏、静音运行到 first game frame ready 并正常退出，最新日志无 ERROR / panic / Vulkan validation error。没有 Rust / Slang、生成文件或 `config/gui.toml` 修改；既有 `tree_ray_candidates` / `query_margin` 两项 unused-method 警告保留。

## 还没有通过的部分

这是视觉候选，不声明已经还原到位。参考中的密集细羽、绒边、羽片相互遮挡和闪光下的 iridescence，比当前较平滑的低模精细得多；眼、耳斑及覆羽仍有程式化感。照片姿态 / 摄影透视与模型取景也不是精确配准。

拍翼仍是折叠翼根旋转展示，不是真实飞羽展开或飞行控制；裸露弯趾没有树枝承托。低分辨率后处理会合并眼高光、足趾与羽斑。尚无用户美术认可，也没有据此宣称通过性能验收；原生 Release smoke 仅确认现有应用未受影响。

当前体验入口（本工作树服务）：

`http://127.0.0.1:8767/model-preview/?model=kingfisher&variant=compare`
