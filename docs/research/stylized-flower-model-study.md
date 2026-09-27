# 花草低模与像素化对照：8 种候选

## 调研与目标

在既有 [模型像素化预览台](../../experiments/model-preview/README.md) 中增加 8 个独立下拉选项，每种有完整低模、造型／配色参数和实时像素结果。不是下载一批版权不明模型，也不另建 HTML。模型是参考植物识别特征后编写的原创参数化网格，沿用苹果／叶片的 JavaScript 配方 → Three.js → 共享像素管线。

### 网上艺术化 demo 参考

- **[RRFreelance · Stylised Low Poly Flowers and Pots](https://rrfreelance.itch.io/stylised-low-poly-flowers-and-pots)**：作者展示整组淡彩低模花草、五瓣小花、放射状花盘、杯状花、宽叶与细茎；说明采用渐变贴图取得 pastel tones。借鉴的是「少量大色面 + 清楚花瓣轮廓 + 细长茎」的表达，不复制其网格／纹理。页面为付费资产，未购买、未下载、未打包其图片。
- **[Lunar64x · Low-Poly Nature Pack](https://lunar64x.itch.io/lunar64x-low-poly-nature-pack)**：作者页面提供可运行森林 demo，明确列出 daisies、sunflowers、lavender，以及可拼接叶片、joined / modular 版本与枢轴。支持本实验把花头与茎叶作为制作单元的方向；不把页面的 code license 当成所有素材的统一授权。本次只阅读介绍／展示，不引入第三方资产。

下列各植物外形依据 **NC State Extension 的 Plant Toolbox 物种页及照片**，每行给出直接来源。照片只用于在线观察，不复制到仓库。花数、低模面数和夸张比例是本项目美术选择，**不是精确植物学复刻**；菊科“花瓣”实际是舌状花，界面用易懂的美术称呼。

## 候选与建模方案

| 下拉名称 / ID | 名称与一手参考 | 可识别的样子 | 本次低模表达与观察重点 |
| --- | --- | --- | --- |
| 五瓣野花 · 野老鹳草 / `wild-geranium` | **Wild geranium / Geranium maculatum**；[物种页](https://plants.ces.ncsu.edu/plants/geranium-maculatum/) | 粉红至淡紫、向上开的浅碟形五瓣花，瓣上有细脉；掌状裂叶，花序有数朵。 | 两个错高的五瓣花头；圆钝瓣、浅凹花心、掌状低模叶。省去细脉，保留瓣间负空间。重点比较 32px 整株里五瓣能否辨认。 |
| 勿忘草 / `forget-me-not` | **Woodland forget-me-not / Myosotis sylvatica**；[物种页](https://plants.ces.ncsu.edu/plants/myosotis-sylvatica/) | 小型蓝色五裂花，黄／白色眼，成密集花序，叶片细长。 | 三朵不同朝向的蓝色小花、浅色喉圈与黄点；不做数十朵噪点。每一朵是一个完整像素单元，不把单瓣拆成精灵。 |
| 白色滨菊 / `oxeye-daisy` | **Oxeye daisy / Leucanthemum vulgare**；[物种页](https://plants.ces.ncsu.edu/plants/leucanthemum-vulgare/) | 白色狭长舌状花围绕扁黄盘；自然标本约 15–35 条白色射线，单头位于茎端。 | 用 16 条可读的窄瓣与低矮黄盘；小侧叶衬托大花盘。测试窄瓣在低像素下是否糊成一圈。 |
| 波斯菊 / `cosmos` | **Garden cosmos / Cosmos bipinnatus**；[物种页](https://plants.ces.ncsu.edu/plants/cosmos-bipinnatus/) | 粉、白、紫红色浅碟花盘，黄色中心，叶片深裂、轻盈如羽。 | 八片宽瓣、略有缺口的瓣缘、深粉内圈与细分叶；与滨菊区别在宽瓣、轻茎和羽状叶，不只换色。 |
| 虞美人 / `corn-poppy` | **Corn poppy / Papaver rhoeas**；[物种页](https://plants.ces.ncsu.edu/plants/papaver-rhoeas/) | 四片红色皱瓣形成杯／碟，瓣根常有黑斑；细长花梗，叶深裂。 | 四大片起伏杯状瓣、暗色瓣根与种荚状花心；少面数表达纸片感。侧／背面检查杯深，而不只看正面圆片。 |
| 桃叶风铃草 / `bellflower` | **Peach-leaved bellflower / Campanula persicifolia**；[物种页](https://plants.ces.ncsu.edu/plants/campanula-persicifolia/) | 蓝紫／白色、向外开的钟形合瓣花，五裂口；细直茎、披针叶。 | 两朵错高、略倾斜的五裂钟杯，闭合花底、可见内壁和花蕊。与自由花瓣完全不同的连体花冠，检验侧视和接茎。 |
| 紫松果菊 / `coneflower` | **Purple coneflower / Echinacea purpurea**；[物种页](https://plants.ces.ncsu.edu/plants/echinacea-purpurea/) | 粉紫色长瓣向外下垂，中央高起的棕橙色刺状圆锥盘，茎直且叶较宽。 | 12 条下垂瓣、橙褐色立体锥盘、大尖叶；省去密刺，用切面表示。检验高花心与下垂瓣的深度关系。 |
| 郁金香 / `tulip` | **Tulip / Tulipa**；[属级参考](https://plants.ces.ncsu.edu/plants/tulipa/) | 多为单朵直立杯形；六片花被，两轮排列；基部长而宽的叶。颜色与形状因品种变化很大。 | 六片交叠暖橘花被形成高杯，黄内侧、两片宽蜡质叶。作为园艺对照，不声称是特定野生物种；测试厚杯形与宽叶搭配。 |

## A/B 的明确含义

预览右侧新增 **“仅花头像素化，再拼接茎叶（实验 B）”** 复选框，仅花卉可用，默认不勾选。原来的保守覆盖开关仍独立存在。

- **A／未勾选：整株像素化。** 花、萼、茎、叶共同进入一个 N×N 缓冲，调用原有几何保守覆盖与八邻接补点；右侧最近邻放大。
- **B／勾选：完整花头像素化后拼接。** 每个完整花冠、花心与花萼共用一个 N×N 单元，而不是每片花瓣一张图。单元仍复用相同补点算法；茎叶保持低多边形连续渲染。所有部分使用同一世界姿态、光照、观察方向和锚点。右侧是较高分辨率透明合成，不冒充整张 N×N 图片。
- 同一个 N 在 A 中属于**整株**，在 B 中属于**每个花头**，因此 B 的花头得到更多细节预算；这本来就是要比较的资产拆分决策，**不是同预算性能 A/B**。界面和导出名必须明确标记。
- 左侧始终是完整原始 3D 模型，A/B 不改变模型、材质、相机或参数。B 只改变像素处理单元和组合方式，不通过放大花瓣、换模型或更改色彩作弊。
- 全部设置只在本次网页会话有效，切模型／重置恢复默认。不修改 `config/gui.toml`，也不新增游戏 Debug 控件。

## 实施边界

本次交付是美术验证候选，不发布游戏资产、不改 Rust／Slang。特别是 B 的茎叶是此网页的低模茎叶代理，不声称已经接入游戏原生植物茎。游戏中的世界尺度、风动、遮挡与批量实例预算，要在用户选择视觉方向之后再单独集成和 Release 验收。

花头像素化不需要重写覆盖／修复算法，但需要薄的分部取景与深度合成层；该层属于共享管线而非模型自己的 renderer。静态花卉不会伪造动画，保留相机转台便于检查立体形态。
