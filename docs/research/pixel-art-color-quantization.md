# 像素风颜色量化：叶片预览的选择

范围：低分辨率模型瓦片的**颜色**减少，不涉及几何覆盖或像素尺寸。以下把工具文档中的事实与本项目建议分开。

## 一手资料中的常见做法

1. **索引色／限定调色板**：Aseprite 的 Indexed 模式让每个像素引用调色板颜色（最多 256 色），修改一个调色板色会同时改变引用它的像素。这和独立对整幅图亮度取整不是一回事。[Aseprite Color Mode](https://aseprite.org/docs/color-mode/)。GIMP 可生成适合当前图像的有限调色板，也可使用自定义调色板；显式的黑白二色调色板是另一种可选模式，而不是所有低色数图像的必然结果。[GIMP Indexed mode](https://docs.gimp.org/3.0/en/gimp-image-convert-indexed.html)。
2. **Posterize／每通道色阶**：GIMP 的 Posterize 把 RGB 各通道分别限制为 2–256 级，例如每通道三级最多产生 27 种组合；“三级”不等于整张图仅有三个颜色，也不等于单一的亮度三级。[GIMP Posterize](https://docs.gimp.org/3.2/en/gimp-filter-posterize.html)。这种方式实现简单，但未必保住给叶面、叶脉、叶背分别指定的配色关系（项目判断）。
3. **抖动是可选的空间混色**：当目标调色板缺少某种中间色时，GIMP 用邻近像素的已有颜色组合来近似；提供无抖动、误差扩散和定位抖动，并指出定位抖动适用于动画。[GIMP Indexed mode](https://docs.gimp.org/3.0/en/gimp-image-convert-indexed.html)。Aseprite 的索引色转换接口也明确提供 ordered/Bayer 与 error-diffusion 选项。[Aseprite ChangePixelFormat](https://www.aseprite.org/api/command/ChangePixelFormat)。不是像素风必须使用抖动；8px 瓦片上的纹理可能反而喧宾夺主（项目判断）。

## 原预览为何变黑（已修正）

原 `experiments/model-preview/postprocess.mjs` 的 `quantizeImage` 先算**线性 RGB 的全图亮度** Y，再乘 `round(Y * steps) / steps / Y`。它实际使用 `0, 1/N, …, 1` 共 N+1 个亮度值，而且把非黑色像素在 `Y < 1/(2N)` 时变为纯黑。例如默认叶面 `#81852c` 的线性亮度约为 0.216，N=1 和 2 时甚至叶面基色也直接映射到 0；N=3 的局部暗处仍会变黑。黑色不是透明，也不是漏采样。当时该函数在 `experiments/model-preview/pipeline.js` 中**先于**几何覆盖／连接修复执行：后补像素可能取未经同样量化的高分辨率样本或邻近色，导致同片叶子色阶不统一。

## 给本项目的建议与实现状态

- **网页预览已实现动态材质色带**：叶片材质另绘不受光照影响的叶面／叶脉／叶柄／叶背基色 pass，蝴蝶使用用户选的翼色；算法围绕各基色生成 0.72～1.28 倍线性亮度的阴影／高光并选取最近档，不会自动把非黑基色变成纯黑。档数 N 指每个局部基色的 N 个明暗档，N=1 只用基色。细线边缘混色可能产生额外 RGB 值，因此并非整张图只用 N 种颜色。这是适应本项目可编辑材料与配色预设的设计选择，不是上述工具文档规定的唯一行业标准。
- 若目的是整个像素画的统一有限色数，可另选**自定义整图调色板 + 最近色映射**，将叶片配色预设导出的颜色与其阴影、高光色组成目标调色板；不把黑色纳入目标调色板就不会自动得到纯黑。与“每区 N 个色阶”应分清名称和含义。
- 抖动初期默认关闭，先审核 8/16/32/64px、正背面和不同配色。若渐变断层明显，再在**瓦片坐标中固定**有序抖动；避免每帧换随机种子。定位抖动更适合动画的依据见 GIMP，但瓦片坐标方案仍需实际视觉验证。
- 量化现已移到原始采样与新增覆盖像素合并之后，统一作用于右侧显示结果。保留原中心采样的几何覆盖规则与是否改变其**显示颜色**是两个独立决策。
- 历史版本的“叶片受光色阶（默认 8）”是材质自身的分级，会与后处理动态色阶叠加；目前已移除这个重复阶段。网页效果未同步游戏，且尚未做正式性能验收。

## 是否需要两个色阶滑杆？

- **两种处理在工程中都存在，但作用不同**。Godot 的 StandardMaterial3D 提供材质级 Toon 漫反射（光照明暗有硬边）；Godot 也另行支持全屏自定义后处理。[Godot Toon 材质](https://docs.godotengine.org/en/4.4/tutorials/3d/standard_material_3d.html)、[Godot 自定义后处理](https://docs.godotengine.org/en/4.5/tutorials/shaders/custom_postprocessing.html)。这证明技术路径可并存，**不能据此推断其他项目通常给玩家／美术两个同义滑杆**。
- 第三方工程 [PixelLight](https://github.com/SalvaPixel/PixelLight) 把离散光照与画师指定的调色板同时作为渲染约束；[GodotPaletteLightingShader](https://github.com/Deab22/GodotPaletteLightingShader) 先编码材质颜色 ID 和照明强度，再经一个调色板／梯度解码。这些实例更像“一个最终的明暗→调色板决策”，而非先量化一次亮度、再以另一档数重复量化。另一个项目 [godot-color-dither](https://github.com/Donitzo/godot-color-dither) 提供可独立选择的材质或全屏调色板替换，并明确全屏抖动固定在屏幕坐标，会随移动对象滑动；这是更广义的**统一全画面调色**用途。
- **修改前本预览的两档实际重叠**：`experiments/model-preview/models/leaf.js` 的“叶片受光色阶”把材质 illumination 用 `floor(illumination*steps+.5)/steps` 分档（默认 8），影响右侧的原始叶片着色；`experiments/model-preview/postprocess.mjs` 的“后处理动态色阶”随后按叶面／叶脉等基色再将完整右侧结果（含几何补点）映射到 N 个明暗档（默认关）。前者只对叶片有效，不限定最终颜色数量；后者对蝴蝶也有效，并能控制最终色带。两者不是数学等价，却会对同一片叶子的亮度连续处理两次。
- **已采纳的设计（非“行业统一惯例”）**：预览主界面只保留一个“动态色阶（最终颜色）”滑杆，作为右侧最终颜色的唯一量化控制；叶片材质的光照在它前面连续计算，再统一映射到动态色带。0=关闭；叶片默认 8 级，蝴蝶默认关闭。对比原默认叶片截图，新结果形状相同、颜色分档略有不同，仍需用户视觉认可。如果后续确实需要研究光照硬边与最终色带的**独立**艺术效果，再以明确区分用途的高级实验控制加入；不要简单地将两个档数绑定为同一个值，那仍是双重量化。
