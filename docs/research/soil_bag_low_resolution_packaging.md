# 营养土袋：低分辨率贴纸设计与采样

## 目标与范围

用户希望贴纸用 Nearest 而不是 Linear，并去掉缩小后不可读的包装文字。目标是在整个袋子只有 32／64 像素时，仍主要靠大轮廓区分包装；不靠增加模型取景尺寸、独立后处理或密集文案。仍是网页候选，不发布原生资产或替换 Soil 交互。

## 官方包装参考

本轮查看以下产品页面及其官方包装图，只借鉴视觉层级，不复制商标、字形、照片、配色组合或版面。以下是对图片的观察，不是市场占有率／产品效果结论；不同地区、容量、年份的包装可能不同。

1. [Miracle-Gro Potting Mix 产品页](https://scottsmiraclegro.com/en-us/brands/miracle-gro/products/potting-mix/miracle-gro-potting-mix/75686301.html)／[正面官方图](https://smg.widen.net/content/qbzhrholg2/webp/75686300_0_F.webp?w=800&h=800)：上部绿色、大片黄色底与下部醒目的花朵，宏观色块和大图比微小说明更抢眼。游戏候选借鉴「大色面＋主体图」，不保留标题、宣传文字、圆形品牌标记或花卉照片。
2. [COMPO SANA Qualitäts-Blumenerde 产品页](https://www.compo.de/produkte/erde-kompost/universalblumenerde/compo-sana-qualitaets-blumenerde)／[官方包装图](https://www.compo.de/dam/jcr:32df09db-cdc4-4995-82a6-8f7e323411f4/cs_5l_3d_de_at_fp_pim.jpg)：上部花／植物、下部土壤，主体分区明确，文字和标记叠在其上。借鉴植物／土层这一题材关系，不复制其 logo、花土照片或印章。
3. [Westland Jack's Magic 当前产品页](https://www.gardenhealth.com/jacks-magic-all-purpose-compost-2)／[官方包装图](https://www.gardenhealth.com/wp-content/uploads/2021/09/0000_10400255-Jacks-Magic-50L-3D.jpg)：蓝色区域中的大向日葵，下部是土壤和粗标题。借鉴单一大主体与背景的强对比；不采用品牌叶形、照片、宣传标语或下方小图标群。没有以官网已停产的另一产品页面作为当前包装来源。

参考图只下载到 `target/soil-bag-sticker-references/` 供本地查看，不提交或作为运行资产使用。

## 原创候选取舍

- 三个独立的大符号：**双叶芽、三层土、五瓣花**，都用同一个底色／图案色接口。不同包装不只是换颜色；同色也有不同轮廓。
- 每张仅保留一个主体与一条实心标题色块；没有字体、边框细线、细根、容量字样、英文副标题或宣传说明。色块只是排版元素，不表示库存或数量。
- 使用确定性的 **32×40 二值图案配方**，整数矩形写入原有 512×640 不透明画布。原始贴纸精确只有底色与图案色两种颜色，没有曲线抗锯齿或字体依赖；源图和像素图仍接受同一 PBR 灯光。
- 原有三套包装预设、改色、大小、侧面扁平度、贴纸导入和两个动作均保留；没有新增保存设置或发布动作。

## 模糊的来源与修正

`THREE.CanvasTexture` 的默认放大是 Linear，默认缩小使用线性／mipmap 过滤；此外原来的 `drawImage` 缩放也启用 Canvas 平滑。后处理画布即使已经最近邻放大，也不能撤销前两层混色。默认值已在实际使用的本地 vendor [`three.core.js`](../../experiments/butterfly-method-comparison/vendor/three-0.183.0/build/three.core.js) 的 `Texture` 构造器与 `CanvasTexture` 子类中核对（Three.js 0.183.0）。

本轮只调整袋子的贴纸：`minFilter = magFilter = THREE.NearestFilter`、`generateMipmaps = false`；绘图时 `imageSmoothingEnabled = false`。自定义图仍等比放入原有画布，透明度正常合成到底色上，不能绕过共享不透明材质契约。半透明输入与底色的合成不是过滤错误。

这不会关闭照明渐变、袋体纸纹或共享修复，也不能凭空增加低分辨率下的像素。Nearest 在旋转／缩小时仍可能跳像素；16² 等极小完整袋子不承诺符号全部可读。实际区分与最终美术接受仍需用户验收，不用 hash 不同冒充人类识别测试。

## 验证入口

新增纯函数与绘图测试检查二值／大面积轮廓、8×10 中心缩小样本的差异、无文字／细线／分数边缘和自定义最近邻等比缩放。网页验收检查真实贴纸材质的 min／mag／mipmap 设置、画布的两色／不透明数据、透明小图缩放不引入插值色，以及 32／64／128² 下同色的三种图案、原有动作和资源释放。运行记录与环境限制见 [网页验收证据](../evidence/web-preview-soil-bag.md)。
