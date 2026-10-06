# 苹果的像素表达与尺寸调节

## 范围与结论

初次调研只做外观参考和实现检查；后续按用户要求增加苹果尺寸调节，见文末实施记录。果皮外观尚未修改。

- 检查到的三个样本都很抽象，但都不是一块纯红：用少量成片的明暗色阶、轮廓和果柄建立识别。
- Minecraft / Stardew Valley 的本次样本是物品图标，不能直接代表挂在树上的 3D 苹果。Minetest Game 的苹果纹理同时用于世界节点和物品栏，世界节点使用 `plantlike`，不是实体球状 mesh。
- Re: Flora 的果肉基色确实只有一种红色，但仍经过环境光和阴影计算；不能把它描述成完全没有光照。
- 调研时没有独立的 GUI 苹果尺寸参数；现已新增 Apple size scale。Fruit Cycle 调成熟阶段，Apples pre-cache resolution 调采样分辨率，都不是尺寸旋钮。

## 已直接查看的参考

下列外观描述是对源图片的观察，不是对作者设计意图的推断。

| 样本 | 观察 | 可借鉴内容 |
| --- | --- | --- |
| Minecraft，Mojang Bedrock samples 的 apple.png | 深红/棕红轮廓，数种红色块，肩部粉红高光，棕色果柄；轮廓顶部不呈完美圆弧 | 用少量大色块表达体积；高光与暗边比细密果皮噪声更容易读懂 |
| Stardew Valley，游戏 Wiki 展示的 Apple 图片 | 暗红边缘，红色主体，偏橙红/浅红的亮块，短暗色果柄；不规则圆润外形 | 暖色亮面和深红暗面能在很小的图像里保留苹果辨识度 |
| Minetest Game，default_apple.png | 绿色柄部，深红外缘和底部，红色主体与亮红块 | 即使更简略，也用明暗与顶部附属形状打破单色团块 |

### 原始来源

1. Minecraft 第一方资源：[Mojang/bedrock-samples apple.png](https://github.com/Mojang/bedrock-samples/blob/main/resource_pack/textures/items/apple.png)。
2. Stardew Valley 游戏图像：[Apple 页面](https://stardewvalleywiki.com/Apple)，[原始 PNG](https://stardewvalleywiki.com/mediawiki/images/7/7d/Apple.png)。Wiki 是游戏社区资料站，图像是本次直接观察的素材；不将社区文字当成开发者的渲染说明。
3. Minetest Game 源素材：[default_apple.png](https://github.com/luanti-org/minetest_game/blob/master/mods/default/textures/default_apple.png)。[节点定义 nodes.lua](https://github.com/luanti-org/minetest_game/blob/master/mods/default/nodes.lua) 中 `default:apple` 的 `drawtype = "plantlike"`，`tiles` 和 `inventory_image` 都引用该文件。

本地放大对照图：`target/apple-research/reference-apples.png`，从左到右是 Minecraft、Stardew Valley、Minetest Game。以最近邻放大，没有重新绘制或增强细节。下载图片仅放在忽略的 target 下，不作为本项目的游戏资产发布；学习表达方法，不直接复制素材。

## 当前 Re: Flora 实现

### 不是缺少高精度几何，而是缺少小尺度的外观信息

`assets/models/apple-source.mjs` 已有：

- 24 个周向分段、16 个纵向环；
- 上宽下窄的肩部变化、顶部凹陷和底部凹陷；
- 单独的果柄和叶片。

但 `shader/slang/model_mesh.slang` 的 `appleMeshVertex` 中，native 和 cached 两条分支都按 material ID 选择固定色：

- 果肉：RGB `(203, 48, 47)`；
- 果柄：RGB `(116, 80, 46)`；
- 叶片：RGB `(77, 145, 62)`。

`shader/slang/apple_pixel_tile.slang` 中也有同样的色表。颜色之后通过 `meshShade` / 对应照明函数处理，存在环境光和阴影，但没有果皮红黄变化、局部亮块等专门的材质表达。

**解释性判断：** 当果子只占很少的最终像素时，细小的凹陷、柄和叶可能不容易保留；仅依赖模型法线和场景光照，容易呈现为均匀的红色团块。这是代码和参考图支持的判断，本次没有新增当前场景截图来量化其屏幕覆盖。

### 调研时的尺寸实现（新增滑杆前）

- `src/tracer/leaves_construct.rs`：`TREE_FRUIT_MAX_RADIUS_VOXELS = 2`。
- `src/app/core/physics.rs`：附着果子随成熟阶段取半径 1 或 2，成熟/待掉落阶段使用最大半径。
- `shader/slang/apple_pixel_tree_pose.slang`：根据附着果子编码的半径计算模型 scale。
- `src/tracer/apple_preview.rs`：模型适配使用两体素半径的基准。
- `config/gui.toml` 与 `src/tree_gen/tree.rs`：有结果概率、位置偏移、摆动等设置，没有独立尺寸倍率。

不能把缓存分辨率、树大小或 Fruit Cycle 当成苹果尺寸调节：它们改变的是不同概念。

## 对本项目的建议（尚未实现）

### 外观优先级

1. 保持圆润、略有肩部的轮廓，不增加更多微小几何细节来解决纯色问题。
2. 用少量、相对大的红色色块：暖红主体、深红/紫红暗部、偏橙红或粉红的亮部。
3. 让顶部凹口和棕色柄在常用视角下更容易识别；叶片作为可选辅助，不是必须依赖的符号。
4. 少量黄红变化或条纹可作为第二轮候选，不优先添加逐像素随机噪声；小苹果上的细密斑点可能只造成闪烁和脏感。
5. 材质变化应固定在苹果局部坐标，不随相机或风移动。图标中画死的高光不能直接照搬为永远面对相机的 3D 高光；应明确区分果皮底色变化与当前光源造成的亮面。
6. 本项目 native / pre-cache、挂树 / 落地苹果应共用一个材质表达，避免各路径独立硬编码色表。

以上是面向本项目的设计建议，不是声称参考游戏使用了相同的 3D shader 算法。

### 尺寸调节

建议增加一个可保存的 `Apple size scale`，默认 1.0，初步视觉比较可以尝试 0.7–0.8。这只是候选，不是调研得出的通用最佳比例。尺寸变小时细节更容易消失，因此应同时验证常用相机距离下的可读性。

尺寸由一个权威参数驱动，至少覆盖：

- 挂树苹果和掉落苹果的可见模型；
- 阴影模型与剔除范围；
- 落地果子的碰撞形状、接触高度，以及其他依赖果子尺寸的物理/编辑范围。

若只提供视觉倍率而不改变碰撞，必须明确标成视觉调节并披露不一致；更推荐做一致的真实尺寸调节，避免画面缩小后果子悬空或仍按大果子碰撞。

## 后续实施：GUI 大小滑杆

已添加 Debug → Growth & Fruiting → `Apples: size scale (attached and fallen)`：默认 1.0，范围 0.25–2.0，可保存。旧配置自动获得 1.0，不在加载时改写文件。

挂树苹果的 native / pre-cache 路径及其阴影共用该倍率；落地苹果的模型、阴影、凸包碰撞和 contact skin 同步缩放。更新现有碰撞形状不重建刚体 ID，保留位置、旋转和速度，并唤醒刚体重新求解。成熟阶段和掉落时序不变。

扩大已落地果子前会检查更大的地形碰撞覆盖；若尚未导入，会排队补齐，期间保持落地果子的旧显示/碰撞尺寸，准备好后同步切换。挂树苹果的主画面和阴影剔除边界会在倍率大于 1 时扩大，并保留原有运动余量；不需要每次调节重建实例缓冲。
