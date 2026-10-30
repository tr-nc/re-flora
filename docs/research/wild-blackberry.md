# 野生黑莓：形态调研与原型取舍

调研日期：2026-10-30。目标是 re-flora 的野生、舒展、略凌乱的中层灌木；本轮只做 HTML 形态原型，不改游戏。视觉交付入口：`experiments/blackberry-prototype/index.html`（通过仓库现有预览服务器访问 `/blackberry-prototype/`）。

## 依据与边界

1. **确实是灌木，但黑莓不是单一株形。** RHS 将 bramble 描述为有粗壮、带刺、拱垂长枝的蔓生灌木，枝长约 1.8–2.5 m，枝尖触地可生根。其 `Rubus fruticosus agg.` 是集合名称，不应当作严格单一物种。[RHS · Brambles](https://www.rhs.org.uk/weeds/brambles-and-other-woody-weeds)
2. **持久的是根系／根冠，不是每根枝条。** OSU 指出黑莓根系与根冠多年生，典型枝条两年生：第一年 primocanes 长枝叶，第二年 floricanes 开花结果后死亡；成年植株可同时有两代枝条。也有一年生枝结果品种，不能把两年生规则写成所有黑莓的绝对定律。[OSU Extension · Growing blackberries in your home garden](https://extension.oregonstate.edu/catalog/ec-1303-growing-blackberries-your-home-garden)
3. **直立、半直立、匍匐型都存在。** OSU 分别列出 erect、semierect、trailing，半直立型有粗壮拱枝；匍匐型不能独立支撑，通常需要架扶。因此本轮选择野生 bramble 的自由拱垂形态，不借果园牵引架或修剪成排的轮廓。[OSU，同上]
4. **复叶沿枝附着，不是球形冠层里的随机叶云。** RHS 的集合条目描述深绿、三小叶；NC State 的 `Rubus allegheniensis` 条目记载互生、常为三小叶／掌状复叶、卵形和粗重锯齿。后者是另一黑莓物种，只用作结构交叉参考，不宣称原型是该物种的精确复刻。[RHS · Rubus fruticosus agg.](https://www.rhs.org.uk/plants/16132/rubus-fruticosus-agg/details)，[NC State Extension · Rubus allegheniensis](https://plants.ces.ncsu.edu/plants/rubus-allegheniensis/)
5. **花、果应挂在结果枝的短侧枝／末端花序，不均匀撒满所有新梢。** RHS 描述去年枝上成簇的五瓣白／粉花，随后果实由绿转红再转紫黑；OSU 的枝条世代说明支持把结果与营养枝分开。RHS 给出的六月至九月是其地区语境，不作为全球统一物候。[RHS · Brambles]，[OSU，同上]
6. **黑莓颗粒不是一串葡萄。** OSU 说明单个果实由多个 drupelets（小核果）组成，黑莓采摘时保留中央果托。造型应当是小颗粒紧密组成一颗卵圆形聚合果，再由数颗果实组成果簇。[OSU，同上]
7. **生态价值与扩张风险并存。** RHS 记载花为传粉昆虫提供资源，果实供鸟与哺乳动物取食，灌丛提供庇护；枝尖压条可迅速扩张。不能因为是“野生花园”就默认无限蔓延或适用于所有地区。[RHS · Brambles]。本原型不启用扩散、自动生根、动物交互或碰撞。

## 在线照片观察

仅在临时目录查看，下列照片未导入仓库，也不做网页热链；来源页面可直接查看原图。公开可浏览不等于可再分发。

- RHS，页面中的 “Bramble stems can quickly grow over other plants”：长枝有明确弯曲方向与节间，叶片由叶柄连接，锯齿和折面可见；并非沿每厘米长满叶子。[原图](https://assets.rhs.org.uk/05836e9f-2692-012f-b4fe-e3f22d127fa9/0a26e624-9237-4d5a-a6df-4fd94cb31f5f/bramble-growth-smothering-shrub.jpg?w=1200&fit=clip&fm=jpg&auto=format)。摄影者／单图再分发许可未确认。
- OSU Figure 3B，`Rubus armeniacus` 果簇，标注 Bernadine Strik / © Oregon State University：同一局部能看到绿、红、黑成熟度差异，小核果构成紧密凸面；这一图只用于果实视觉参考，不把其物种身份混入原型命名。[原图](https://extension.oregonstate.edu/sites/extd8/files/styles/full/public/images/2022-04/fig3b_0.jpeg?itok=YpICkjWG)。未取得再分发授权。

## 本轮美术假设（不是测量或植物学定量结论）

- 以 `Rubus fruticosus agg.` 野生黑莓意象为主，做风格化形态研究，不声称精准物种鉴定。
- 一丛由数量有限、长短不一的基生枝组成；一部分向上，一部分拱垂，一部分向外低伸。避免等角度、同高度的喷泉式对称。
- 分开表现绿色营养枝和较褐的结果枝；不模拟完整两年生生长过程。果期仍保留无果新枝。
- 三小叶为本轮简化约定，叶片有中折面和少量锯齿；尖刺用稀疏轮廓提示。近景才看锯齿，远景优先看主枝走向、叶簇与空隙。
- 花期和果期是展示预设，不是时间模拟；果期允许同簇绿、红、紫黑果并存，不同时堆满盛花和成熟果。
- 先不做圆球式灌木冠、修剪绿篱、真实攀附、枝尖生根、收获、生命周期和动物交互。
- 风只做根部固定、末端增强的共享位移示意，不冒充力学仿真；网页像素预览只做低分辨率栅格化，不等同游戏的模型像素管线，也不是性能证据。
- 叶密度变化不改变骨架，花果阶段不改变枝条和叶片拓扑。同一 seed 可复现，便于讨论究竟是轮廓问题还是装饰问题。

## 看原型时要回答的问题

1. 关掉叶与果后，多根拱枝是否仍像灌木，而非树或一捆花茎？
2. 加叶后还能否读出至少几段枝条，并保留内部空隙？从背面／侧面看是否也成立？
3. 不依赖果实，株形是否足以与现有直立草花区分？
4. 果实是否像紧密聚合的小黑莓，而不是葡萄、圆珠或过大的装饰？
5. 与 1.7 m 的人形尺标比较，横向占地和中层高度是否适合花园？原型单位为美术约定，尚未绑定游戏体素尺度。

待用户确认形态后再讨论原生接入、动态枝条所有权与 Release 性能；不把网页视觉通过视为游戏可交付。
