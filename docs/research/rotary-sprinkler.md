# 三臂旋转洒水器：造型调研与实现契约

## 一手参考

1. [Orbit Brass 3-Arm Adjustable Sprinkler with Wheel Base 58180](https://www.orbitonline.com/products/brass-3-arm-adjustable-sprinkler-with-wheel-base)。官方说明为三臂旋转、360° 覆盖、可调喷嘴；[官方产品图](https://www.orbitonline.com/cdn/shop/products/58180N_600-2-1_d7d76e40-c789-4dfc-ab8b-f95957d0d814.jpg?v=1741884205)可见黄铜中心轴、三个同向弯曲的细喷臂、深色喷嘴及绿色轮式底座。已查看原图，不从其他商家的参数推断形状。
2. [GARDENA Comfort Circular Sprinkler Mambo](https://www.gardena.com/int/products/watering/sprinklers/comfort-circular-sprinkler-mambo/900979001.html)。官方确认可独立调节喷嘴、360° 配水和稳定的滑橇底座。借鉴低重心和可读的喷头结构，不照搬品牌配色或实际米制喷洒参数。
3. [Hunter MP Rotator](https://www.hunterirrigation.com/irrigation-product/mp-rotator/standard-mp-rotatorr-nozzle) 是另一条多股旋转喷流路线；本轮仅搜索结果可用，详情页抓取为 403，未据此确认内部机构或采用具体外形。

## 选型

采用原创的三臂花瓣式转头，而不是把原四嘴头整体旋转：鼠尾草绿圆矮底座、奶油色立柱、黄铜轴承及三个同向弯曲的铜管，末端有厚喷口和暗色内孔。保留花园工具的柔和材质区域；省略实物的轮子和品牌标志。三臂轮廓能在低分辨率下区分新旧模式。

这是风格化建模与运动参考，不是水压/流体或真实灌溉工程复刻。初始一圈约 4 秒是游戏美术选择，需由玩家试用确认。

## 行为契约

- 新增可保存的运行时 `sprinkler_rotary` A/B checkbox。默认 false：原交替 X/Z 喷头；true：三臂持续绕竖轴旋转。旧存档只补缺字段，已保存的 true/false 保留；不自动覆盖用户尺度、速度或干燥倍率。
- 模型、水雾历史与实际灌溉共用世界时钟、每实例相位和同一组出口/方向公式。使用既有世界 tick 的小数余量实现连续运动，暂停保留余量；不引入独立计时器或另一个 emitter 注册表。
- 旧模式几何与运动保留。新底座固定、转头刚体旋转；两种模式都接入现有对象级 8–32 N×N surface cache 与原生回退，灯光/深度继续 live，不改变全屏像素化。
- 水雾按出生时的旋转角重建过去三秒，而不是拿当前角度旋转整片历史。保持既有地形约束、体积归一化和总视觉源量；不增加可见水滴。原 ellipsoid A/B 仍是原无方向雾，不能声称它也显示旋转喷流。
- 三个喷口同时工作。灌溉使用三个随头转动的单向扇区，中心仍受角度约束，合法湿斑外缘可跨角边。瞬时总许可角保持 180°（3×60°，对照原 2×90°），原半径/水量倍率不随喷口数放大。
- 视觉密度为零或普通粒子关闭不影响真实浇水。湿斑尺度、浇水速度、干燥 0.2、其他模型采样及花头 1.11 独立保留。

## 验证计划

CPU/Slang 对齐连续角度、三个真实出口、整圈和相位；验证旧模式不变。检查保存/迁移、旋转 rigid frame 的缓存/原生等价与整物体采样边界。通过隐藏静音 Release 检查开关、历史、实际土壤、零密度、粒子关闭、分辨率、resize 与生命周期，查看新旧模型及旋转雾带截图。正确性检查不是新性能验收，也不替代玩家外观认可。
