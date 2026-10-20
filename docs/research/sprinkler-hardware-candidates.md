# 单方向、断续出水、慢转喷头：外观候选调研

## 结论与边界

最接近玩家描述的是 **impact / pulse sprinkler（冲击式／摇臂式）**，而不是三臂反作用
旋转喷头。识别轮廓应是：固定供水轴、短喷嘴、偏置摇臂／配重、弹簧与支架，以及扇区调节环。
慢转与断续感来自机械作用；不要把游戏的周期性 0/1 阀门当作这些实物的精确工作原理。

本轮做造型候选，不修改喷幅、RPM、Duty、频率、源位置、空气／接触／湿土模型。
候选借鉴结构与轮廓，使用原创程序几何、游戏自身配色，不复制商标或产品资产。
摇臂先保持一个明确姿态，不伪装成已经模拟了弹簧、敲击、分度或真实压力驱动。

## 一手资料

1. [Rain Bird 25BPJ](https://www.rainbird.com/products/25bpj-series)
   - 官方产品图可见外露回位弹簧、上方支架、向前的摇臂、竖直轴承连接和调节环。
   - 产品页明确列出黄铜压铸 PJ 摇臂、不锈钢弹簧／支点销、分流针及可选距离挡片；
     规格中的喷射仰角为 25°。
   - [官方正面照片](https://www.rainbird.com/sites/default/files/styles/max_550x550/public/media/images/2018-01/25bpj.jpg?itok=ioaQaUYH)
2. [Rain Bird impact troubleshooting guide](https://www.rainbird.com/sites/default/files/media/documents/2017-06/ImpactTroubleshootingGuide.pdf)
   - 第 1 页说明 diffuser screw 打散水束、增加近处浇水；distance control flap 将水束下偏并重新分布。
   - 第 3、6 页给出摇臂弹簧、支点销和转向机构的检修／拆解，支持上述机械轮廓判断。
   - 本轮读取了官方 PDF 正文；原始提取仅作为本地调研材料，不将整份说明书复制进仓库。
3. [Rain Bird Mini Bird P5R/P5R+](https://www.rainbird.com/products/mini-birdr-p5r-and-p5r-plastic-impact-sprinklers)
   - 官方明确说明加重摇臂减慢旋转、导流臂约束水束、diffuser screw 产生更细水滴。
   - 同一类动作并不要求裸黄铜外壳，也可以是聚合物主体、金属配重的园艺机头。
4. [GARDENA Premium Full or Part Circle Pulse Sprinkler](https://www.gardena.com/int/products/watering/sprinklers/premium-full-or-part-circle-pulse-sprinkler/901153701.html)
   - 官方说明金属／塑料组合、黄铜喷嘴、地插固定和大尺寸射程旋钮。
   - [官方产品图](https://media.husqvarnagroup.com/image/GA110-1113.png?height=700&width=700&fit=bounds)
     展示轻薄宽臂、深色主体、醒目调节件，而不是细长弯管。
   - 游戏候选将地插适配为可读的便携底座；这是美术取舍，不是产品复刻。
5. [Hunter PGP-ADJ](https://www.hunterirrigation.com/irrigation-product/rotors/pgp-adj)／
   [Rotor FAQ](https://www.hunterirrigation.com/rotor-frequently-asked-questions)／
   [Hunter Rotors: A Revolution Still Running Strong](https://www.hunterirrigation.com/headlines/hunter-rotors-revolution-still-running-strong)
   - 搜索索引返回的官方正文描述了水轮、密封齿轮机构与旋转喷嘴塔，以及连续水束。
   - 适合作为封闭圆柱造型的对照，但不应声称它天然具有游戏当前的整束开／关脉冲。
   - 本轮直连 Hunter 正文与 PDF 返回 403；此项依据官方页面的搜索索引正文，未读到原始 PDF，
     不据此推断内部齿轮尺寸或动画。

## 三款候选与原款

| 选择 | 轮廓意图 | 与当前动作的关系 |
| --- | --- | --- |
| 原款 | 细立管、细弯臂、小圆底 | 保留未勾选时的比较基准 |
| 黄铜摇臂 | 短粗铸造机头、外露弹簧、拱形支架、前后不对称臂 | 最接近 impact；本轮不模拟敲击 |
| 园艺脉冲 | 宽底、深绿主体、宽摇臂、铜橙旋钮 | 更日常、柔和的园艺用品方向 |
| 封闭齿轮 | 矮圆柱、顶盖、单侧短喷口，无外露摇臂 | 更简洁；脉冲仍沿用游戏模型 |

## 实现约束

- Debug 中提供持久化 A/B 复选框：未勾选为原款；勾选后使用三项候选选择器。
- 候选全局应用到现有和新放置的喷头；切换不重置世界时钟、不改变浇水参数。
- 每款仍是 16 个**完整整机**内部方向；底座／机头一起烘焙，固定外部相机框架。
  外部观察方向数仍与苹果共享。所有款式的喷口中心与同一个 RotorPose 出口对齐。
- 原生几何保留各款，表面样本只烘焙所选款；切换期间走对应款的原生路径，不能显示另一款的旧缓存。
- 使用相同相机／光照的近景与正常游戏距离截图，另测缓存开关、运行时切换、保存／加载及源位置对齐。
- 先让玩家比较造型；正确性验证不等于最终视觉批准或大量喷头性能验收。
