# 三趾翠鸟：替换网页中的两种旧鸟

> **首版历史记录（`6f7e8bc7`），不是当前美术验收。** 用户随后指出神韵、比例和颜色离参考很远；本记录中的协议测试通过并不代表还原正确。当前模型与照片对照修正见 [kingfisher-reference-correction.md](kingfisher-reference-correction.md)。

## 范围与入口

用户要求移除网页 demo 中的麻雀 / 燕子，按所给照片复刻三趾翠鸟的配色和结构，并体验现有后处理。

- 当前动物目录仅有 `bee` 与 `kingfisher`。旧鸟的目录项、几何配方和测试夹具已移除，不只是隐藏下拉选项；纯函数拒绝旧 ID。
- 蜜蜂的确定性几何与停留 / 拍翼姿态 SHA-256 和修改前一致；颜色、光照、默认分辨率与适配行为不变。
- 仍使用唯一 `models/index.js`、原始 / 像素双画布、共用时间轴与 `pipeline.js`。没有新增 HTML、renderer、后处理分支或 native 动物资产。
- 入口：`/model-preview/?model=kingfisher&variant=compare`。本次工作树服务为 `http://127.0.0.1:8767/model-preview/?model=kingfisher&variant=compare`，不是其他工作树的旧服务。

## 外形与参考

参考用户提供的 [三趾翠鸟条目](https://zh.wikipedia.org/zh-tw/%E4%B8%89%E8%B6%BE%E7%BF%A0%E9%B8%9F) 与其两张照片：

- [Ceyx erithaca.JPG](https://commons.wikimedia.org/wiki/File:Ceyx_erithaca.JPG)：侧面轮廓、橙红长喙、金黄腹部、紫粉冠、蓝黑翼与蓝羽斑。
- [Oriental Dwarf Kingfisher (male).jpg](https://commons.wikimedia.org/wiki/File:Oriental_Dwarf_Kingfisher_(male).jpg)：白喉、颈侧白斑、耳斑、叠羽和短尾。

原创确定性低模，共 **7,050 三角形**，不包含下载的照片、纹理或第三方模型。美术目标是有物种特征的低模近似，不宣称扫描级或解剖学精确重建。

- 大头与紧凑身体；连续曲面上的色区，不再以多颗彩色球叠成胸 / 颊。
- 橙红喙有宽根、扁平截面、上脊、下颌与细喙缝，长度接近头长；小型深色眼与几何高光。
- 紫粉冠 / 腰尾、橙黄脸颊 / 额缘、金黄胸腹、米白喉部 / 颈侧斑、蓝耳斑、蓝黑翼底。冠顶羽尖使用 palette 权重在 Linear-sRGB 中混入暖色；七个颜色槽均可调，改色不创建几何或纹理。
- 每侧贴体收拢翼、五根长羽与三排十二片蓝覆羽。翼面沿同一身体曲率构造，先细分再映射，避免大三角形切进身体导致缺口。
- 短而窄的三片尾羽；橙红短足，每足**两趾向前、一趾向后**，不是旧配方的三趾全向前。

## 观察与后处理

- 三趾翠鸟默认 64²，蜜蜂保持 48²；可调 8–128²，导出透明 PNG。
- 原模型与像素图始终共享姿态与采样时刻；可拖动旋转、正交 / 透视、前 / 后 / 侧、相机转台、逐帧。
- 身体（含头尾足）、左翅、右翅保持三个 repair ID。所有部件不透明，未引入 alpha 贴片。
- 复用原有“通用保守覆盖（关闭 = 旧八邻接补点）”实时开关；未给喙、足、羽斑增加模型专用补点，已有中心 RGBA 仍不可覆盖。
- 停栖片段真正静止；拍翼仅作细节稳定性观察，按绝对时间旋转翼根。未实现真实飞羽扇面展开、收足或飞行身体姿态，不能当作真实飞行动画。
- 极低分辨率下细趾、高光、羽斑会丢失 / 合并，保守覆盖也会加粗和合并亚像素空隙；没有新增时间滤波。视觉接受仍待用户体验。

## 验证

- `node --test experiments/model-preview/tests/*.test.mjs`：**36 passed**。确定性、有限顶点 / 单位法线、非退化三角形、索引、颜色混合权重、修复分组、长扁喙 / 短尾 / 三趾结构、闭合细分羽片边缘（每条边恰好两面，没有 T-junction）、周期 / 镜像 / 静止姿态与既有资产回归。
- `NODE_PATH=/home/terence/code/linky/node_modules PREVIEW_ARTIFACT_DIR="$PWD/target/new-bird-review/browser" node experiments/model-preview/tests/animals-browser.cjs`：**2 个模型、89 个姿态、27,394 个原始 RGBA 样本原样保留**；此次没有整模型中心漏采样。覆盖 8/32/64/128px、两种投影、三个视向、额外 32 个三趾翠鸟拍翼相位、全部七个配色控件、后处理开关、重置 / 播放 / 逐帧、透明 PNG、模型切换与 390px 布局；page / console / request errors 为空，全部请求同源。
- 最大参数（大小 1.2、翅长 1.15、拍翼幅度 1.3），32 个相位的实际世界顶点距固定中心最大值：三趾翠鸟 **1.739132 < 1.8**（span 3.6），蜜蜂 **2.079213 < 2.3**。曾试用 span 3.4，被边界验证拒绝，已恢复安全固定取景；未缩小测试参数或放宽断言。
- 反复调整大小 / 翅长 / 配色以及切换带纹理花朵，GPU geometry / texture 计数无增长。新冠顶混色材质随场景一并释放。
- `agent-browser` 独立工作树会话实际查看默认 / 侧面模型与像素图；另检查浏览器测试的背面 / 拍翼截图。用户体验从当前工作树服务打开，不启动可见 native 游戏。
- `cargo build --release` 成功；`cargo run --release -- --hidden --mute --auto-exit 0.5` 成功，日志确认 RTX 3060 Ti、首帧完成、正常退出。通过同一工作树的 `--tail-latest-log 200` 检查，无运行错误。已有 `tree_ray_candidates` / `query_margin` 未使用警告，不是本次网页改动引入。
- 无 Rust / Slang 源码、生成文件、正式资产或 `config/gui.toml` 改动。浏览器 SwiftShader 是正确性 / 视觉验证，不是游戏 Release 性能证据。

证据：`target/new-bird-review/{node-tests.log,browser-tests.log,release-build.log,native-smoke.log,native-latest.log,browser/summary.json,browser/*.png}`。运行产物不提交。
