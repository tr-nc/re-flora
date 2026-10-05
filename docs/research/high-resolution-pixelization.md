# 高分辨率输入 → 风格化低分辨率像素画面

## 本次需求澄清

目标不是以抗锯齿为验收标准，而是：**先在较高分辨率下渲染场景，再让像素化 Shader 决定低分辨率网格中每个像素应该留下什么颜色、轮廓和细节。** 例如将源图的 2×2 像素归为一个最终像素，但不规定必须求四个颜色的平均值。

三个量必须分开：

1. **显示窗口**：实际物理像素尺寸；UI 保持原生。
2. **最终场景网格**：已有 1:1、4:1、16:1、64:1 档位；比例指屏幕像素数量，4:1 是 2×2 屏幕像素一个显示块。
3. **场景源图**：可以比最终网格更密。例如源图 2560×1440 → 网格 1280×720，就是四个源像素归为一个最终像素；源图 640×360 → 网格 320×180 也是同样的四变一。

“高分辨率”是相对最终网格，不必意味着超过屏幕分辨率。高分辨率输入与像素风格并不矛盾；抗锯齿也不必被禁止，但它不是这次寻找算法的目的。

当前 `shader/slang/post_processing.slang` 已有“较密场景渲染 → 低分辨率颜色归并 → nearest 展示”的结构，但归并规则是 **linear HDR box average**。这可以做覆盖平均的比较基线，不能因此称为已经实现或认可了更好的像素画风。当前 Debug 的 antialiasing 命名也没有表达用户澄清后的目标。本轮只调研、没有改动 Shader 或控件。

## 最贴近需求的公开实现：PixelOE

PixelOE 明确以高分辨率图片生成像素画为目标。作者采用 **contrast-aware outline expansion → downsampling**，并提供 contrast、k-centroid、nearest 等方法，调色板量化是可选步骤。[1]

本次核对源码版本：`7ce444b36d3876a151d845d4493240e904454d89`。不仅阅读 README，还核对了实际 Slang 和 legacy 降采样代码。[1–3]

### A. 轮廓保护之后再缩小

作者先生成局部明暗对比权重，再混合 erosion / dilation，并用 opening / closing 清理形态。目的不是把边缘平均平滑，而是在缩小前改变关键特征的覆盖，让细节有机会留在最终网格中。[1]

**适用推断**：这和细枝、草叶缩小后消失的问题相近，值得做视觉实验。但它会主动改变形状和厚度，不能说成“无损保留细节”。深度遮挡下如何保护前景，也不能单凭图片算法解决。

### B. Contrast-aware 选色

核对的 `contrast.slang` 在局部块中计算 Lab 的统计量：[2]

- L 通道使用 median、mean、min、max，在条件满足时选择 min / max，否则使用块内约定的样本。
- a、b 通道取 median，再转换回 RGB。
- 它不是把全部颜色求平均，也不保证输出恰好等于某个原始 RGB 像素。

源码的 fallback 是 row-major 索引 `p*p/2`，不要把它不加区别地称为几何中心采样。

**适用推断**：这是明确可读、已有 Slang 实现的像素化候选，适合优先比较。但选择极值可能强化噪声和高光；只做该选色步骤，不等于复现完整 PixelOE，也不保证少数细枝像素不丢失。

### C. K-centroid 局部聚类

核对的 Slang 实现对每个块做 k=2 颜色聚类，最终通常选择样本更多的簇的中心；同样大小的簇按固定规则打破平局。输出仍可能是**簇内平均色**，不是“任何平均都不允许”，但避免直接把不同类别的颜色全部混成一个中间色。[3]

**适用推断**：适合比较草叶/背景的颜色分离与块面清晰度。不过多数颜色也可能恰好是背景，所以这种方法单独使用会丢掉少数细枝。迭代和额外读写的实时成本须在本项目 Release 下测量。固定输入上的确定性不等于运动画面的时间稳定性。

作者仓库同时提供 Slang GPU 内核，说明存在可研究的 Shader 实现；不代表可以不经接口、颜色空间、资源管理和许可证检查直接接入本项目。仓库许可证为 Apache-2.0；本轮未复制代码。[1,7]

### 接入风险：颜色空间和归并的位置

PixelOE 的 RGB→Lab 路径以 sRGB 图片为输入；本项目当前先平均 HDR，再 tone map。不能直接把 HDR 数值塞进它的 Lab 转换，也不能悄悄改变 tone-map 顺序后，把全部差异归因于选色规则。[2] 对照应固定曝光并明确共同输入域；若为选色增加显示域输入，对齐输入域的平均方法和现有 HDR-average 原版应分别标明。

当前后处理按每个物理输出像素执行，因此同一个放大块会重复计算归并。若加入 median / clustering，自然的候选结构是先按最终低分辨率网格归并一次，再 nearest 展示并合成原生 UI。减少重复归并是源码层面的工作量推导，不是已经测得的性能提升；也不是本轮已完成的管线改动。

## 工业工具参考：ProPixelizer

作者文档描述 Pixel Expansion：先以点状/抖色模式绘制对象，再通过后处理扩展成大像素，用更细的底层目标支持较平滑的运动和 per-object 像素尺寸。[4]

它的重要启示是：**底层渲染分辨率和最终可见像素尺寸可以不同**。但它不是普通的 2×2 box resolve，也不自动符合本项目“全屏固定整数网格”的约束；不建议照搬整个扩展管线。

作者还把 silhouette outlines、normal/depth edge detection 和 outline depth testing 单独处理。[5] 这说明像素风的轮廓是独立的造型问题，不能只把高分辨率黑线与背景一起平均后，假设线宽自然会正确。

**本项目推断**：若需要结合几何信息，应在颜色归并之外明确规定 depth/normal/对象分类的处理方式，不能将这些数据和颜色一样无条件平均。使用这些信息保护前景，是候选设计，不是本轮已验证的效果。

## 博客参考：调色板与有序抖色

Alex Charlton 的《Dithering on the GPU》提供了实际 GLSL：根据颜色与调色板的关系，再使用有序阈值矩阵决定输出颜色，并讨论了局限。[6]

这是一层可选的像素画风表达，不是替代空间降采样的完整算法。第一轮不建议同时强制少色调色板、抖色和轮廓扩张，否则无法判断改善来自哪个步骤。若以后加入，阈值坐标应取**最终粗网格坐标**，再 nearest 展示；不能让同一个大像素块内部出现不同的屏幕级抖色。这是结合本项目约束的推导。

## 方法比较与建议

| 方法 | 每个最终像素如何获得颜色 | 主要取舍 |
| --- | --- | --- |
| 格点/中心附近单样本 | 选一个源位置 | 硬、清楚，但没有充分利用块内信息，容易漏细节 |
| 当前 box average | 所有源颜色平均 | 可作为基线；边缘贡献可能变淡，颜色混合 |
| Contrast-aware | 根据局部统计量选明暗，处理色度 | 保持局部对比的候选；可能强化噪声或产生颜色变化 |
| K-centroid | 选局部颜色簇的代表色 | 减少不同类别混色；可能丢掉少数前景、发生簇切换 |
| 轮廓保护 + 选色 | 缩小前先调整重要特征覆盖 | 更接近主动像素画造型；会改变形状与线宽 |

**建议下一轮是“高分辨率像素化”实验，而不是再强化 AA。**

1. 保留已有最终网格档位、整数显示块和原生 UI。
2. 用相同较高分辨率源图，比较当前平均与 **contrast-aware resolve**，先看归并规则本身的影响；保留原来的低分辨率直接渲染作为另一参考。
3. 如果主要问题仍是细枝/叶片消失，再单独加入轮廓保护，对比它的厚度与遮挡副作用。它应是明确的第二步，而不是偷偷把最小像素覆盖强制做大。
4. K-centroid 留作另一种块面/色彩风格候选；暂不同时叠加调色板。
5. 真正实施时用 Debug 的运行时 A/B 控件和统一保存。命名区分“最终像素比例”“源图渲染密度”“像素归并方式”，不要将这些都描述为 antialiasing。

观察移动镜头、风中的草叶、细枝、前后景交界、描边和高光。**清晰的单帧不代表动态稳定。** 先让用户看实际画面，再做 Release 成本测量；本轮没有实现候选、运行视觉比较或测性能。

## 来源与核对边界

1. [PixelOE README，固定源码版本](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/README.md)：高分辨率图片像素化目标、轮廓扩张、可选降采样与调色板、Slang 后端。其作者性能数据不作为本项目性能证据。
2. [PixelOE contrast.slang](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/contrast.slang)：已核对统计量、fallback 索引、Lab 转换及输出规则。
3. [PixelOE kcentroid.slang](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/src/pixeloe/slang/shaders/downscale/kcentroid.slang)：已核对 k=2、选择多数簇、tie 和空簇处理；另核对 legacy `k_centroid.py` 的最常见颜色规则。
4. [ProPixelizer：Pixelisation Controls](https://propixelizer.github.io/docs/usage/pixelization/)：已获取作者文档正文，含 Pixel Expansion 原理与运动取舍。它引用的 Medium 文章本轮返回 403，未声称已审计文章里的完整实现。
5. [ProPixelizer：Edges and Outlines](https://propixelizer.github.io/docs/usage/outlines/)：已获取作者文档正文，含 silhouette / normals / depth 分工。
6. [Alex Charlton：Dithering on the GPU](https://alex-charlton.com/posts/Dithering_on_the_GPU/)：已获取作者文章正文及 GLSL；抖色是可选风格层。
7. [PixelOE LICENSE](https://github.com/KohakuBlueleaf/PixelOE/blob/7ce444b36d3876a151d845d4493240e904454d89/LICENSE)：Apache-2.0。

Shadertoy 已检索，但候选页面直接获取返回 403，没有获得可以审计的 Shader 源码，因此不把搜索命中当成已经验证的算法。SIGGRAPH Asia 2022 的《Make Your Own Sprites》也找到线索，但全文获取受站点/文件大小限制，本轮不据此作实现建议。上述开源图片算法和工具文档证明方向存在，不证明它们是实时游戏统一采用的标准，也不证明在 Re: Flora 的动态 3D 场景中已经有效。
