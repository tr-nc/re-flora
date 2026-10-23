# 针对 Re: Flora 的像素风游戏阅读清单

调研基线：本轮读取的 `main`，`adde77ac`。本清单是资料筛选与阅读建议，不是新的产品决定、渲染实现方案或视觉／性能验收。

## 项目背景与筛选依据

- [Game Direction](../game_direction.md) 将项目定义为可居住、可自由种植的 3D Restorative Garden，并确认探索普通光栅建筑模型与可编辑体素土壤、植物共存的屋顶餐厅方向。[Rooftop PoC](../rooftop_poc.md) 记录的是实验候选，不能据此宣称已完成美术管线或获用户认可。
- [`scene_resolution.rs`](../../src/tracer/scene_resolution.rs) 已区分显示像素网格与源图采样密度，采用整数屏幕像素块、nearest 展示及原生 UI。因此只讲“降低 RenderTexture 分辨率”的教程，对本项目增量有限。不要把历史实验中的固定 0.125 比例当作当前唯一实现。
- [`camera/controller.rs`](../../src/gameplay/camera/controller.rs) 使用透视投影；项目还有 orbit、缩放与自由观察。因此正交相机的像素对齐方案只能条件性借鉴，不能保证直接解决透视运动中的全部跳变。
- [已有高分辨率像素化调研](high-resolution-pixelization.md) 和 蝴蝶 3D-to-pixel 调研（历史文件已删除，见[清理记录](../assets/cleanup-unused.md)；原路径 `../../experiments/butterfly-method-comparison/blender-v2/research-3d-to-pixel.md`） 已涉及 ProPixelizer、Dead Cells 和 Blender Studio。本清单把这些资料放回整体美术学习路线，不把它们当作本轮首次发现。

优先选作者亲述、工具作者文档与已完成游戏的制作资料。事实与迁移建议分开：以下“对 Re: Flora 的价值”均是阅读者的判断，不是来源对本项目效果的保证。资料以英文为主。

## 最推荐的三项

### 1. A Short Hike：从整款游戏看美术取舍

- **作者网页：** [Crafting a tiny open world](https://blog.playstation.com/2021/08/05/crafting-a-tiny-open-world-a-look-behind-the-scenes-at-the-creation-of-a-short-hike/)
- **补充演讲入口：** [Crafting A Tiny Open World: A Short Hike Postmortem — GDC](https://www.gdcvault.com/play/1026613/)
- **类型：** 已完成的实时 3D 像素风游戏；作者第一人称文章，免费可读。

作者 Adam Robinson-Yu 在网页中说明，他希望用尽可能少的像素做出美丽且可读的 3D 世界；这一决定反过来约束平坦而统一的着色、无抗锯齿和帮助识别对象的柔和轮廓。他还解释了宁静的探索体验与玩家可调的像素尺寸。

**对 Re: Flora 的价值：**先回答“低分辨率为何让这个花园更好”，再决定色彩、模型细节与效果。它比孤立 shader 示例更适合研究小团队如何形成完整、舒适的游戏视觉方向。学习其取舍，不代表要引入对白、探索任务或改变本项目产品方向。

### 2. David Holland：最贴近花园题材的实时 3D 技术拆解

- **阅读：** [3D Pixel Art Rendering](https://www.davidhol.land/articles/3d-pixel-art-rendering/)
- **补充：** [How my 3D PIXEL ART Camera Works](https://youtu.be/LQfAGAj9oNQ)
- **类型：** 作者技术长文，免费；Godot 4.3 自定义构建背景。

文章把轮廓、边缘高光、相机、toon lighting、草、树、水、体积光和粒子放在同一个视觉目标下解释；也明确承认模型／角度对描边质量的影响，并说明渲染顺序、透明物体和屏幕读取的实际问题。草的均匀受光与地面融合、浅水和植被例子尤其值得读。

**对 Re: Flora 的价值：**是“让一片 3D 自然环境看起来像有意设计的像素画”的直接参照。建议先读 Outlines → Grass → Lighting → Water，再读 Camera 和体积光。

**边界：**这是技术／风格实验，不是完整游戏生产手册。其相机演示采用正交投影；Godot 的补丁、接口限制和作者 GPU 耗时不应直接迁移为本项目需求或性能证据。

### 3. Saint11：真正决定像素画质量的色块组织

- **先读：** [Cluster Sketching and Painting](https://saint11.art/pixel_art_articles/article2/)
- **再查图例：** [Pixel Art Tutorials](https://saint11.art/blog/pixel-art-tutorials/)，重点看 Vegetation、Wind、Water、Rock、Silhouette、Outlines。
- **类型：** 作者 2D 像素画文章和图解，免费。

Cluster 文章从大块同色区域开始，再细化形状、明暗和边界，解释孤立单像素为何可能产生噪声，也说明单像素可以用于眼睛、纹理等有意强调的细节。作者明确推荐这种方法绘制植物、水、山等自然背景。

**对 Re: Flora 的价值：**2D 资料在这里不是低质量替代品，而是补齐实时渲染经常缺少的美术语言。可以拿最终游戏截图检查草丛、树冠、花头、果实的轮廓和色块，而不是只检查 3D 模型是否精细。优先保留可辨的大形与主次，不把“禁止一切孤立像素”变成机械规则。

## 按具体问题继续读

### 4. ProPixelizer：渲染策略与边界条件

- [Pixelisation Controls](https://propixelizer.github.io/docs/usage/pixelization/)
- [Eliminating Pixel Creep](https://propixelizer.github.io/docs/usage/eliminate-pixel-creep/)
- [Edges and Outlines](https://propixelizer.github.io/docs/usage/outlines/)
- **类型：** Unity 工具作者的免费文档；不是要求购买或切换引擎。

文档区分低分辨率目标与 Pixel Expansion：后者先将对象画成点状模式，再扩展为大像素，以保留较细的运动步进，并可支持逐对象像素尺寸。轮廓章节区分对象 ID 轮廓、法线／深度边缘及受光倒角效果。像素对齐章节明确指出正交方案与透视投影的不同限制。

**对 Re: Flora 的价值：**在建筑、体素地形、植物混合时，帮助明确“最终像素尺寸、底层渲染密度、轮廓分组、运动稳定性”是不同问题。最值得借鉴的是这种分工，而非照搬整套 Pixel Expansion 或为每类对象新增独立管线。

### 5. Lucas Pope / Obra Dinn：动态画面的舒适性

- **阅读：** [Fullscreen, Round 3](https://dukope.com/devlogs/obra-dinn/tig-32/)，定位到作者 2017-11-23 的 #767 帖。
- **类型：** 已完成 3D 游戏的作者开发日志，免费；1-bit 风格，不是彩色花园美术教程。

作者具体展示了低分辨率抖色在移动画面中的游动与闪烁，以及屏幕、几何、相机相关映射和超采样的取舍。日志中的方案会牺牲部分严格的 1-bit 输出，以改善全屏观看舒适性。

**对 Re: Flora 的价值：**Restorative Garden 的风格应在观察、绕转、缩放时仍然舒服，而不只是截图漂亮。学习评估方式，不是建议复制黑白风格；其旋转稳定方案也不保证动态植物、平移和透视缩放全部稳定。

### 6. Dead Cells：用 3D 提高像素资产迭代效率

- **阅读：** [Art Design Deep Dive: Using a 3D pipeline for 2D animation in Dead Cells](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-)
- **类型：** Motion Twin 美术 Thomas Vasseur 亲述，免费；**3D 制作、预渲染 2D 精灵**，不是实时 3D 世界。

作者从 2D 像素 model sheet 出发制作简单模型和骨架，以小尺寸无抗锯齿输出 PNG 与 normal map；先确认少量关键帧和节奏，再补帧，利用资产、骨架复用和快速修改提高产出。他也明确承认闪烁与细节限制。

**对 Re: Flora 的价值：**适合蝴蝶、未来小动物或重复资产的制作规划：由最终像素预算约束建模工作量，并优先保证姿态与可辨识特征。不建议据此将当前世界渲染整体改为 sprite，也不照搬其战斗动画节奏到风中植物。

### 7. Pixel Logic：系统性的 2D 图解教材

- **官网：** [Pixel Logic](https://pixellogicbook.com/)
- **作者商店及目录：** [Pixel Logic — A Guide to Pixel Art](https://michafrar.gumroad.com/l/pixel-logic)
- **类型：** Michael Azzi 的付费数字书，图解为主。

官方商店目录覆盖 Line Art、Anti-aliasing、Colours、Readability、Dithering、Game Perspectives、Clean-up、Subpixeling 和 Animation。对本项目建议优先读 Colours、Readability，再按需求补轮廓、Subpixeling 和 Animation。

**对 Re: Flora 的价值：**为配色、形状可读性和像素运动建立统一评价语言，比零散搜索滤镜参数更系统。手工像素画中的 anti-aliasing 也不等同于引擎的 TAA／MSAA，不能把不同语境的处理混为一谈。

**核对边界：**本轮核对了官网与作者公开目录，没有购买或审阅付费正文。价格以购买时官方页面为准。

## 可选的固定镜头风格探索

[Blender Studio — Exploring 3D Pixel Art in Blender 4.2](https://studio.blender.org/blog/3d-pixel-art-in-blender/) 展示了 Eevee、Grease Pencil 与手工像素笔触结合，并提供 `.blend` 下载入口。作者明确记录其两套像素网格在平移／缩放时无法一致，因而在该实验中采用固定镜头。

适合制作一张花园或屋顶的风格参考图，体会“渲染输出之上还需要美术判断”。不把文章当作适用于自由透视相机的完整游戏管线，也不据此断言所有 3D 像素风都必须固定镜头。

## 建议阅读路线

1. **先建立视觉目标：** A Short Hike → Saint11 的 Cluster 文章。
2. **再看自然场景怎么实现：** David Holland。
3. **遇到运动、描边、混合表示问题：** ProPixelizer → Obra Dinn。
4. **要扩充资产生产：** Dead Cells；需要系统补美术基础时读 Pixel Logic。

阅读后可以用现有花园／屋顶的一张截图制作很小的风格样稿，只关注一株植物、一个果实和一块建筑边缘：哪些形状必须保留、哪些细节应合并、主次靠什么明暗与色彩区分。再对照运动中的画面。这个练习是美术探索建议，不构成实施授权，也不要求立即增加新渲染系统。

## 本轮来源核对说明

已读取 A Short Hike 作者网页、David Holland 长文、ProPixelizer 三章、Saint11 文章与教程索引、Dead Cells 作者文章、Blender Studio 文章，并核对 Obra Dinn 开发日志中的相关段落。Pixel Logic 的公开目录从作者商店原始页面核对。

GDC 入口本轮返回会员／会话提示，没有完整观看演讲；David Holland 的相机视频只作为原文给出的补充入口。t3ssel8r 的 Imgur 页面动态加载、YouTube 抽取失败，Patreon 笔记返回 403，因此没有把这些未完整取得的内容作为本清单技术结论的依据。未运行游戏、改动 shader 或做性能测量。
