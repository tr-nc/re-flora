# 蝴蝶初始配色调研

日期：2026-09-13。代码基线：`f0e2215d`。本轮仅调研，不改配色、生成数量、飞行或用户配置。

## 结论

当前值得优先改进的是「同一纹样的七种颜色等概率出现」，不只是某个颜色太鲜艳。建议以奶白、淡黄、橙褐、土褐构成日常底色，蓝色作为点缀；紫色和红色改为有明确纹样依据的方案，再决定是否加入初始池。这是面向本游戏的美术建议，不是自然界的颜色丰度排序。

不能把蓝、紫或红简单判成假颜色：现实中有蓝蝶、带紫光的蝶，也有深红色主翼。自然感来自主色、暗边、斑点、条带以及上下翅面的组合，不是把一种通用图案轮流染成彩虹色。

## 当前代码事实

- `src/particles/emitters.rs` 在现有发射器中对 `0..ButterflyPalettePreset::COUNT` 均匀抽样；七种各有约 14.3% 的生成概率。紫、红、蓝合计期望约 42.9%，不代表某次运行的实测个体比例。
- `src/tracer/butterfly_palette.rs` 定义七组调色，每组包含边缘、暗部、中间色和亮部。
- `src/tracer/resources.rs` 对同一个动画图集进行七次重映射；`src/tracer/palette_remap.rs` 根据原图颜色亮度推断角色。它不是七种有独立翅纹的蝴蝶。
- 活跃图集为 `assets/texture/butterfly_16px/butterfly.png`。目录里其他命名颜色文件的存在，不能代表当前生成概率。

| 预设 | 中间色 RGB | 中间色 hex | 对当前通用换色方案的判断 |
| --- | --- | --- | --- |
| Yellow | 232, 185, 48 | #E8B930 | 偏金黄；可另做粉蝶式淡黄，而非认定所有黄蝶都应变淡 |
| Purple | 145, 82, 190 | #9152BE | 紫色渐变不等同于黑白翅纹上随光出现的紫光 |
| Orange | 225, 118, 32 | #E17620 | 可以保留，值得加强橙与褐／黑的区域对比 |
| White | 205, 210, 218 | #CDD2DA | 中间色偏冷灰；可尝试更暖的奶白，但亮部本来已接近暖白 |
| Red | 205, 48, 54 | #CD3036 | 红色有现实依据，宜和暗底、条带或眼斑一起考虑 |
| Blue | 70, 130, 220 | #4682DC | 蓝蝶有现实依据，保留暗边／浅缘比一味去饱和更重要 |
| Brown | 165, 105, 50 | #A56932 | 当前偏暖赭褐，可以补充更朴素的深褐配浅斑方案 |

这些数值是源代码色值，不是自然翅色测量，也不等于经过光照和显示变换后的屏幕颜色。小尺寸下纹样可能难辨、进而显得像色块，这是待实机比较的解释；本轮没有证明存在 gamma、曝光或着色器错误。

## 自然史依据

资料以自然保护机构、博物馆和政府物种资料为主。英国花园／草地案例与香港亚热带案例可提供设计参考，但不能外推为全球或中国所有地区的物种比例。

| 配色家族 | 可核验案例 | 对设计的启发（非生物学定律） |
| --- | --- | --- |
| 白、奶白 + 灰黑 | 英国常见白蝶通常有暗翅尖或斑点；香港东方菜粉蝶为白翼配黑斑，康文署称其为本地最常见蝴蝶之一。[BC 白蝶识别](https://butterfly-conservation.org/news-and-blog/how-to-identify-white-butterflies)、[香港湿地公园](https://www.wetlandpark.gov.hk/en/biodiversity/beauty-of-wetlands/wildlife/pieris-canidia)、[康文署](https://www.lcsd.gov.hk/en/green/butterfly/whites_yellows.html) | 很适合作为日常基础色，但保留灰黑翅尖，避免整只发白 |
| 黄、浅绿白 | Brimstone 雄性为硫黄色，雌性更浅、近奶白。[BC](https://butterfly-conservation.org/news-and-blog/look-out-for-brimstone-butterflies) | 可提供淡黄与浅奶白，不能把雌雄都概括成同一种黄色 |
| 橙褐、深褐 + 浅斑 | Meadow Brown 为褐底配橙斑；Speckled Wood 为深褐配奶黄斑。[The Wildlife Trusts 花园识别](https://www.wildlifetrusts.org/wildlife/identify-british-butterflies) | 保留暗区与明暗纹样，不必让整个翅面都是亮橙 |
| 蓝 + 褐边／白缘 | Common Blue 雄蝶蓝翼有暗边、白缘，雌蝶可从褐到蓝。[自然历史博物馆 Common Blue 小节](https://www.nhm.ac.uk/discover/rainbow-nature-life-in-blue.html) | 蓝色并非只属于热带巨蝶；可以保留，不能据此推出场景应大量出现蓝蝶 |
| 紫光 + 黑白纹样 | Purple Emperor 黑翼有白带，雄性在阳光下呈紫色光泽。[BC](https://butterfly-conservation.org/news-and-blog/the-emperor-returns-to-norfolk) | 不宜用普通整翼紫色渐变直接声称复刻该物种；也不能由此断言全球紫蝶稀有 |
| 深红 + 黑斑／眼斑 | Peacock（Aglais io）深红主底有黑斑和蓝眼斑，英国资料列为常见、广布。[The Wildlife Trusts](https://www.wildlifetrusts.org/wildlife-explorer/invertebrates/butterflies/peacock) | 红色主翼确实自然存在；若减弱当前亮红，是美术选择，不是纠正不存在的颜色 |

## 建议的下一步（尚未实施）

1. 日常初始池优先使用奶白、淡黄、橙褐、土褐，保留少量蓝。先降低通用紫／红方案的存在感，不要一刀切地降低全部饱和度。
2. 每套配色同时设计暗边与亮部关系。现有角色调色可承载有限变化；真正的眼斑、红带、翅尖区域需要相应图案，不能靠全局换色凭空生成。
3. 如果后续调整抽样权重，仍在现有发射器、数量和栖息地权威中完成，不增加平行生成器。权重标为游戏美术参数，不伪装成野外调查百分比。
4. 实机对比时保持当前小尺寸、运动和场景光照一致，检查近处纹样与玩家视距的整体观感。本轮未完成改色后的视觉验收，也不建议在没有比较画面的情况下定死最终色值。

## 验证与范围

已核对当前调色、抽样与图集重映射路径，并交叉核对上述自然史资料。部分 BC 页面直接访问受限，相关描述采用其搜索索引摘要；没有将未读到的全文作为额外证据。本次只新增本报告，执行 `git diff --check`；无 Rust、shader 或生成文件变更，因此不运行构建和 GPU 验证。运行中的游戏及用户配置不作处理。
