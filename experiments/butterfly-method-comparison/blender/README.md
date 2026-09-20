# B 路线：Blender 共用模型与拍翅动画实验

状态：2026-09-06。这是一次性可审阅原型，不是生产素材替换；没有改游戏代码、覆盖游戏图片、commit 或 push。

## 本次问题与结论

问题：极简三维蝴蝶能否用同一动作，从五个固定角度得到一致且可复现的像素序列帧？

**结构上可以，16px 美术效果尚未过关。** 共用模型/翅铰链直接保证跨视角的身体、翅纹和相位一致，保存的 `.blend` 可以完整重放导出。不过物理上的正面/背面视角常常只能看到翼膜侧缘；这个模型用 15° 相机仰角时，16px 正/背高抬翼帧只剩 8 个不透明像素。它证明了稳定生产管线，不证明它比原手绘图更好看。

这个区别不能用“输出恰好 80×80”掩盖。当前手绘 atlas 正面/背面各帧约 53–93 个不透明像素，轮廓可读性明显更强，但不必然对应同一真实三维几何。后续若选 B，应调翼面/镜头俯角，或在稳定导出后做轻量人工像素整理。未经这种美术验收，不建议替换现有素材。

## 交付文件

- `butterfly-prototype.blend`：可编辑原模型、左右翅铰链、25 fps 源动画、五台正交相机。
- `butterfly-prototype.glb`：重新打开上述保存工程后直接导出的同源三维模型；一条 `Scene` 动画包含左右翅两个旋转通道，时间严格为 0..1 秒。
- `preview.html`：可旋转真实 GLB、连续源动画与 5 帧 Sprite 同一时间线、方向切换、暂停、逐帧、原始/16px/32px/灰度切换、完整 atlas。
- `blender-source-turntable.webm`：同一 `.blend` 真实渲染的 4 秒转台/四次拍翅，60 帧，15 fps；有损视频、黑底，仅作预览。
- `raw/`：五方向 × 五帧 128×128 原始透明 RGBA 渲染，加一张 t=1 的循环验收帧。
- `replay/`：重新打开同一保存工程后的第二次导出，用于逐像素复现验证。
- `turntable-frames/`：真实 Blender 转台渲染的 60 张 256×256 透明原始帧。
- `atlas-128-rgba.png`：640×640 原始渲染拼图，不是索引色/二值 alpha。
- `atlas-16-blue-indexed.png`：80×80，16px 单帧，蓝色实验预览。
- `atlas-16-gray-indexed.png`：80×80，16px 单帧，灰度、5 项索引、binary alpha 的格式候选。
- `atlas-32-*-indexed.png`：160×160，32px 单帧对照，不符合当前游戏 80×80 尺寸要求。
- `source-manifest.json` / `validation.json`：参数和实际验收数据。
- `create_export_blender.py` / `run.py`：来源与导出流程。

## 一键复现与编辑入口

已安装官方 Blender 4.5.13 LTS Linux 便携版：

`/home/terence/.local/opt/blender-4.5.13-linux-x64/blender`

系统安装需要管理员密码，因此没有尝试绕过权限；使用用户目录的官方便携版。`run.py` 依次读取环境变量 `BLENDER`、PATH、该用户目录路径。依赖的 `/usr/bin/python3`、Pillow 11.3.0、FFmpeg 7.1.4 已存在。

在本目录执行：

```bash
/usr/bin/python3 run.py
```

**这个默认命令会按脚本配方重新创建并覆盖实验 `.blend`。** 若先手动编辑了工程，使用下列命令保留编辑并从保存工程导出：

```bash
/usr/bin/python3 run.py --export-existing
```

两种入口都从保存工程重新导出 GLB，完成原始序列、第二遍复现序列和转台渲染，再生成索引图并验收。`source-manifest.json` 记录本次脚本配方；手动改相机/模型后应同步更新配方与记录，不能把旧参数记录当作新编辑的证明。

若只需打开工程：

```bash
/home/terence/.local/opt/blender-4.5.13-linux-x64/blender butterfly-prototype.blend
```

没有自动启动可见 Blender，以免打断用户当前桌面工作。

## 固定导出约定

| 项目 | 本次值 |
| --- | --- |
| 核心模型 | 4 个平面翼膜轮廓；叠加几何色块；3 个低面身体体积与 2 根触角；全场景共 33 个 mesh、126 个面 |
| 翼动作 | 两个镜像铰链，共用 `60° × sin(2πt)`，不是昆虫生物力学模拟 |
| 源动画 | 25 fps，frame 1..25；frame 26 为严格重合的循环端点 |
| Sprite 时间 | t=0/.2/.4/.6/.8 秒，对应 frame 1/6/11/16/21 |
| 翼角 | 0/+57.063/+35.267/-35.267/-57.063° |
| 行顺序 | 正面 0°、前侧 45°、侧面 90°、后侧 135°、背面 180° |
| 相机 | 正交尺度 2.6，仰角 15°，距离 7，目标为世界原点 |
| 坐标 | 身体朝 +Y，Z 为上；90° 相机位于 -X，头朝画面左侧 |
| 锚点 | 世界原点投影到每帧中心 (.5,.5)，不按轮廓移动或裁切 |
| 渲染 | Cycles CPU、8 samples、seed 0、禁用 animated seed、Standard 色彩变换；纯几何+自发光色块，无外部图片贴图 |
| 布局 | 5 行 × 5 列，无间距，无字，无边框；128px 原图 → 16px 或 32px |
| 像素化 | 全 atlas BOX 缩小，不逐帧自适应；alpha <128 透明，否则255；固定全局4色最近色量化，不抖动 |
| 索引约束 | 1透明+4不透明；PNG PLTE 恰好5项；灰度版 RGB 相等 |

三维网页预览使用透视镜头供自由旋转，**不是**正交渲染的逐像素替代。转台视频也不参与 Sprite 生成。

## 实际验收记录

已执行默认重建命令，也已执行 `--export-existing`；两次均成功。

- 25/25 原始帧与重新打开保存工程后的重放逐 RGBA 像素一致。
- t=0 与 t=1 的真实渲染逐像素一致；源动作端点显式设为 0，避免浮点 `sin(2π)` 余差。
- 25 张原始帧四边全透明，无贴边裁剪；不按运动轮廓重定位。
- 每个角度都有 5 张不同的 16px 帧，不是重复静帧。
- 所有 16px/32px 索引图都使用且仅使用索引 0..4；alpha 恰为 `[0,255]`。
- 80×80 灰度版符合本次静态核验到的尺寸、索引、五种已使用颜色与二值 alpha 约束；**未在游戏里替换运行，不宣称完整导入已验收**。
- 首次 GLB 导出把左右翅拆成两条独立 clip，浏览器发现后关闭 `export_anim_scene_split_object`；另启用 `export_anim_slide_to_zero` 去掉 Blender frame 1 导出时的 .04 秒偏移。当前实际 GLB 是一条双通道、0..1 秒动画。
- 隐藏内置浏览器实际打开 `http://127.0.0.1:8787/butterfly-method-comparison/blender/preview.html`；标题正确，页面状态显示 `真实 3D 已加载 · 1 条源动画`、`80×80，每格 16px`。实际拖动看到模型旋转，t=.2 双翅与 Sprite 同为抬起；灰度/侧面切换、第五帧回首帧有效，转台视频实际有画面。
- 子任务调用 visible=true 得到产品限制 `IAB visibility is not supported in a subagent thread`。因此上面的浏览器验收是隐藏预览，不是已经向用户展示；需主任务打开最终 A/B 页面。没有操作 Chrome 或飞书。

## 仍不合格或不应夸大的部分

1. 16px 下身体/触角很容易消失；正/背窄翼姿态只有 8 像素。当前原手绘图更饱满。
2. 4 个翼膜轮廓、镜像花纹和纯正弦拍翅很机械；共用三维源带来一致性，不自动带来生动性。
3. 5 fps 本身显著离散。连续模型预览顺滑，不能拿它代替五帧成品验收。
4. alpha 阈值与低分辨率光栅化会造成轮廓边缘跳动；固定原点避免的是整体定位漂移，不是让每个边缘像素永远稳定。
5. 翼面投影面积/重心随动作变化是合理的真实运动；这里不使用逐帧居中去掩盖。
6. 最后一帧到首帧不是重复。统计见 `sampled_16px_phase_metrics`：各方向 wrap 改变 22–36 像素，未比该行内部最大的过渡更大，但这个简单统计不等同主观循环美术通过。

## 来源与完整性

模型与脚本为本实验新建；只参考现有资产的蓝/灰像素配色与布局，不使用外部艺术素材。

- [Blender 4.5 LTS 官方说明](https://www.blender.org/releases/4-5/)
- [Blender 官方下载目录](https://download.blender.org/release/Blender4.5/)
- [官方 SHA-256 文件](https://download.blender.org/release/Blender4.5/blender-4.5.13.sha256)
- 下载包 SHA-256：`da4e69b06b75b9e642d106496c50e7e240218b411d2f6e18271c1d1d819cef91`，已用官方校验文件核对成功。
- [model-viewer 动画接口官方文档](https://modelviewer.dev/examples/animation/)
- [model-viewer 相机官方文档](https://modelviewer.dev/examples/staging-and-camera-control.html)
- 本地 `model-viewer-4.3.1.min.js` 来自 `https://ajax.googleapis.com/ajax/libs/model-viewer/4.3.1/model-viewer.min.js`，SHA-256 `283b0672384614b4847636c306fc93fe4b1fcadc76d668b4e47f0ca76bcf033b`，附 `MODEL-VIEWER-LICENSE.txt`。

未购买服务、未要求用户 API key、未发布到外网。
