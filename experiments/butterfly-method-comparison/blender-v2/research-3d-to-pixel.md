# 蝴蝶 v2：从真实制作案例反推 3D-to-pixel 设计

调研日期：2026-09-06。范围：公开的一手作者说明、工作室文章和工具官方对作者的访谈。本文仅指导本次实验，不修改 Re: Flora 仓库，不涉及提交或发布。

## 核心结论

可借鉴的制作流程不是「正常建一个精细 3D 模型，最后加像素滤镜」，而是让最终像素图反过来约束模型、特征、姿态与帧选择：Dead Cells 从像素 model sheet 出发；Soyafire 明确为像素化而夸大特征；Javin 在低分辨建模时主动放弃精细纹理。这三者都实际输出图像精灵，而不是只展示实时 shader。[Dead Cells 作者文章](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-)、[Soyafire 作者原帖](https://www.reddit.com/r/PixelArt/comments/lo886t/i_made_a_tutorial_on_how_to_use_blender_to_create/)、[Javin 作者教程](https://www.javin-inc.com/blenderpixel/)

对本次 v2 的直接设计推论是：先让 16px 档的翼形和身体可辨，再做大色块、关键拍翅姿态与有节奏的身体运动。具体身体升沉、俯仰、胸腹跟随均是本实验的艺术设计，不是下面的人形角色／环境案例对蝴蝶生物力学的证明。

## 1. Motion Twin / Thomas Vasseur：Dead Cells

**类型：3D 制作、离线输出逐帧 PNG 的 2D 角色精灵；同时输出 normal map。不是运行时把角色当实时 3D 模型画出来。**

Vasseur 先画基础 2D 像素 model sheet，再在 3DS Max 做简单模型和骨架；他明确指出角色在游戏里约 50px 高，因此不把大量工作花在精细建模上。自制工具以很小尺寸、无抗锯齿渲染，逐帧导出 PNG 与 normal map。[作者说明：What 小节](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-)

动画先用尽可能少的关键帧确认姿态和节奏，再在关键帧之前／之后补插值；攻击主要是 pose-to-pose，并用特效表达冲击。他也承认低分辨渲染有像素闪烁问题，最终选择优先动画、接受部分细节限制。[作者说明：动画流程与 Result 小节](https://www.gamedeveloper.com/production/art-design-deep-dive-using-a-3d-pipeline-for-2d-animation-in-i-dead-cells-i-)

**迁移到 v2 的推论：**先画或检查展开、合拢、过渡这几个最终尺寸的剪影，再修改 3D 翼形和身体比例。不要以模型视图里的细节丰富度作为通过标准，也不要把「曲线平滑」误当作「动作已经读得出来」。50px 案例不能直接给出 16px 蝴蝶的特征尺寸。

## 2. Soyafire：Blender 角色 → 多方向像素走路精灵

**类型：离线渲染角色帧，进入 Aseprite 清理。个人作者实验，不冒充已发行工作室产品。**

作者在原帖的流程摘要里明确写到：建模和绑定时夸大特征，以便像素化后好看；随后制作 8 帧走路动画，每个方向循环转相机 45°；用 Freestyle 做外轮廓及合成效果像素化，渲染所有帧后在 Aseprite 清理，并补眼睛等特征。[作者原帖及过程链接](https://www.reddit.com/r/PixelArt/comments/lo886t/i_made_a_tutorial_on_how_to_use_blender_to_create/)

原文关键短语： “exagerated features so it would pixelize well”。作者还提供从早期模型、衣物尝试到最终角色的过程图；后续回复承认头发曾显得浑浊，并展示更精修的版本，因此这不是只交付滤镜结果的流程。[同一作者原帖](https://www.reddit.com/r/PixelArt/comments/lo886t/i_made_a_tutorial_on_how_to_use_blender_to_create/)、[作者完整教程](https://www.youtube.com/watch?v=eSqb6II3WMM)

**迁移到 v2 的推论：**身体可以为了像素可读性变得更饱满；头、胸、腹不必坚持现实比例。翼缘／斑纹若在最终尺寸只剩断续单点，应合并、加厚或删除。若某个必要特征依然无法稳定采样，可以明确标注后期手修，而不能宣称所有像素都来自未经修改的 3D 渲染。

## 3. Javin：Blender → Aseprite 等距精灵制作

**类型：离线图像精灵／sprite sheet。案例主体是砖块等场景资产，不是角色动画。**

作者先搭可复用的正交相机和渲染 rig，按最终像素尺寸调整 orthographic scale 与 shift；文章讨论 16×16 的等距 tile 轮廓，并实际展示 32×32、64×64 输出。序列图片再由 Aseprite 导入、导出 sprite sheet。[作者教程](https://www.javin-inc.com/blenderpixel/)

作者明确说明示例没有精细纹理或 bump mapping，因为在目标低分辨率里它们只会淹没于噪声；建模时应优先有趣的 silhouette，而非细节。[作者教程：砖块示例后的说明](https://www.javin-inc.com/blenderpixel/)

**迁移到 v2 的推论：**v1 的细翼纹、狭窄暗边不应继续按 3D 真实物件的标准累加。先用少量连续色区表达「深色边界＋主翼色＋特征斑块」。此例不提供角色动作证据，其旧 Blender 界面数值和等距 tile 相机角度也不应照抄给蝴蝶。

## 4. Blender Studio / Rik Schutte：3D Pixel Art in Blender 4.2

**类型：官方风格探索；EEVEE＋Grease Pencil 的像素化场景和动画。不是已验证的 16px 角色精灵生产管线。**

Schutte 希望在视口里直接看最终像素，从而随时调整；他组合 EEVEE 视口像素化与 Grease Pencil Pixelate。移动或缩放相机会造成两种像素化方式不对齐，因此这个实验固定相机。着色使用离散色阶，3D 环境另加 Grease Pencil 像素笔画加强定义。[Blender Studio 官方文章](https://studio.blender.org/blog/3d-pixel-art-in-blender/)

同一个固定相机实验里，作者仍将动态元素制作成动画 Grease Pencil，并提供制作延时录像及 `.blend` 文件。这证明该案例的固定相机约束与场景元素动画并存；并不意味着角色质心必须逐帧静止。[官方文章：Conclusion](https://studio.blender.org/blog/3d-pixel-art-in-blender/)

**迁移到 v2 的推论：**固定导出相机、画布、尺度与锚点，同时让身体在画布内有意升沉，是两个互不矛盾的设计选择。不要对每一帧单独 auto-fit 或重新居中，否则会把设计好的 body bob 抵消，或让不同翼姿引起非预期缩放。该文的限制属于当时组合两种渲染路径的实验，不能扩大成「所有像素渲染都必须固定相机」。

## 5. t3ssel8r：3D Pixel Art Game 攻击动画

**类型：引擎内使用真实 3D 资产的实时像素风游戏实验。这里借鉴动画设计，不把它列成预渲 sprite 成品案例。**

在 Cascadeur 官方采访中，作者说明使用真实 3D 资产是为了共享武器、混合动作及在角色间复用动画；因为目标是像素风，模型简单，动画短。他认为要主动选择保留或舍弃的帧，大动作可用 smear，小动作可用有限动画及重复帧，以清楚传达运动，而非直接接受均匀插值。[Cascadeur 官方对作者的采访](https://cascadeur.com/blog/general/making-an-animation-for-a-3d-pixel-art-game)

作者自己的制作视频按阶段展示 reference、第一遍动画、导入预览、smear、第二遍动画与 secondary motion。作者描述中的 6:03 章节明确标为 “secondary motion and tweaks”；可作为观察次级运动打磨过程的定位，不据此虚构某条蝴蝶关节规律。[作者原视频，6:03](https://www.youtube.com/watch?v=1FrIBkuq0ZI&t=363s)

**迁移到 v2 的推论：**拍翅主动作确认之后，再加身体升沉／俯仰，再加腹部跟随，分层比较，不能一开始把所有部分绑定到同一个同相正弦。胸、腹不必同角度、同时间达到峰值。此例没有验证「腹部一定滞后多少毫秒」，更没有证明蝴蝶的升力相位。

## v2 的具体制作与验收建议（本实验设计，不是文献事实）

1. **先定义像素预算。**在 16px 档原生尺寸检查身体是否仍只是细杆。优先保留可辨头部、较饱满胸部、渐细腹部；可先试让胸部至少占连续的 2px 宽，而不是只靠一列明暗像素。这是起点，不是普适标准。
2. **轮廓先于翅纹。**先用纯色检查开翼、闭翼、半开姿态。前／后翼轮廓需要能读成蝴蝶，不能只剩两片对称三角片。必要时为主要观看角度调整比例；几何面数多寡本身不是质量指标。
3. **大色块取代细碎装饰。**先保留暗体／暗边、主翼色、少量大斑块。把低于一个目标像素的狭长色带视作风险区域，逐帧检查它是否闪烁；合并或扩大后再判断，而非任意增加线条。
4. **动画分三层。**第一层只确定拍翅关键姿态和节奏；第二层加入固定画布内的身体升沉与轻微俯仰；第三层让腹部相对胸部有较小、错相的跟随，必要时加入轻量触角跟随。主动作和次级动作必须能够分别关掉对比。
5. **稳定导出不等于冻结身体。**固定相机矩阵、正交尺度、画布尺寸、导出锚点；帧间身体位置可变化。不要把整张 alpha mask 的质心恒定当成验收项：翼展开本来就会改变像素质心。应跟踪胸部或显式 rig anchor，判断位移是否来自设计。
6. **先看原生像素，再看整数放大。**原生尺寸判断可辨识度，最近邻整数放大判断断线、孤立像素、轮廓跳变。固定静态镜头下输出开翼／闭翼／过渡 contact sheet 和完整循环；不只选最好的一帧。
7. **动作指标与包装指标分开。**固定画布、alpha、帧数、循环无重复末帧等是交付结构检查；开闭翼节奏、身体质量感、胸腹跟随是动画质量检查。前者全部通过，也不能证明后者自然。

### 最小对照组

| 对照 | 固定项 | 变化项 | 要回答的问题 |
| --- | --- | --- | --- |
| A：仅拍翅 | 模型、颜色、相机、画布 | 翼姿态 | 新轮廓是否在 16px 档读得出来？ |
| B：拍翅＋身体运动 | 与 A 相同 | body bob、轻微 pitch | 身体是否更像承受动作的整体，而不是悬空细杆？ |
| C：再加胸腹跟随 | 与 B 相同 | 腹部错相／小幅跟随 | 次级运动是否增强生命感，还是增加像素噪声？ |

这些比较不应用「质心移动越少越好」评分。它们要隔离出有意运动的贡献，并避免因重新取景、自动裁切或逐帧缩放得到误导性差异。

## 证据边界

- 上述五个都是作者亲述或官方承载的第一人称制作资料；其中前三个输出预渲图像精灵，后两个仅作为像素化制作／动画设计对照。
- 只有 Dead Cells 文中明确提供约 50px 的角色尺寸；Javin 的 16px 讨论是等距 tile，不是 16px 角色。不能声称五个案例都验证了 16px 蝴蝶。
- 本次读取的是原文、过程图链接和视频作者描述；未逐帧审阅所有链接视频。t3ssel8r 的 6:03 仅作作者已标注的次级运动章节定位，未从不可用字幕推导具体 rig 细节。
- 本文没有蝶类飞行研究、真实高速摄影或物理参数证据。若要声称自然蝴蝶在某相位上升／俯仰，或腹部滞后具有特定数值，需要单独的一手生物力学来源。
