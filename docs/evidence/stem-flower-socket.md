# 花头接合面约束与网页贴纸编辑清理

## 用户范围

删除网页画笔、撤销、权重 PNG 导入／导出及编辑器专用逻辑；保留程序化贴纸和 palette 展示。用户选用 World direction pixels，并报告滨菊（黄心、多片黄白窄瓣）有时被茎穿出，要求从源头规避，不按花另加补丁。

## 网页清理（独立提交 592f707f）

删除 `weight-map-editor.js`、viewer 的 weight-map 控件分支／palette 同步／编辑器导入、花模型的编辑控件，以及共用文件中只服务画笔的 `paintMask`。同步移除这些交互的测试，替换为明确检查不存在编辑器、上传控件、贴纸导入／导出按钮或编辑器请求。

程序化模板、透明权重验证和 Linear-sRGB palette 解码仍是模型颜色来源，不能误删。保留形态和 palette 调整、原有像素结果透明 PNG **截图导出**；它不是贴纸保存，也不发布页面设置。

Node 34 项、11 花型工作台浏览器回归、cargo check 通过。权重源码指纹变化由 publisher 更新 `flowers.json`，默认图集颜色未改变。

## 茎问题的复现与判据

真实生产 `traceStem` 回归：滨菊仰角 42°，茎半径倍率 1.33，终端朝花头法线发射射线。旧实现把茎作为端点球的凸包并一直生成到头部 attachment：终端圆帽自然伸到 attachment 的外侧，薄平花盘尤其容易露出。当前滨菊平心位于 attachment 外侧约 0.008 authored unit，终端球半径约 0.020，不能仅靠深度遮挡假定花心永远盖得住。

已先运行红测试：

```sh
python3 scripts/run_slang_tests.py > target/stem-socket-red.log 2>&1
# flower_stem_socket returned non-zero exit status 2
```

判据为命中点必须满足 `dot(headNormal, hitPoint - liveAttachment) <= tolerance`；旧 trace 返回正值。此命令直接执行生产 analytic stem 数学，不是“运行没有报错”的替代验证。

曾区分三种可能机制：源圆帽越界、方向格像素平面重投影越界、花头缓存视向／深度差。红测试确认第一种是无需相机量化也存在的源几何违规；第二种则需额外约束实际显示点。没有据此修改花头深度或认为缓存更密就能修好。

## 根因修复

1. **共用生成器声明接合面**：由同一个 tilt 变换给出 `socketNormal`，沿 attachment 原点定义外向面。publisher 写入 `socket_normal`；Rust 验证有限／单位法线并上传。无物种名字判断，不依据中心颜色、截图或三角形扫描猜面。
2. **截断真正的 analytic 源几何**：所有侧面／圆帽候选只接受接合面内侧点；沿平面与源圆锥球凸包 union 的实际交集补一个闭合平盖。不是取了最近命中后直接丢弃，也不是偷偷把整根茎缩短／降低深度，内侧射线仍能得到正确的下一次交点。
3. **动态 attachment 相同**：plane offset 使用 `stemCenter(s,0,1)` 的真实末端，包含当前层数、rest bend 和风位移；法线经同一 plant yaw frame 使用。花头与茎共享末端位置，不为各帧猜偏移。
4. **约束实际 direction texel 显示范围**：固定源射线的样本平面重投影到实际 display ray 后，检查其实际世界／深度点仍在内侧。跨面部分返回既有 transparent/far-depth miss，不用 depth bias 把绿色藏到花瓣后面。连续与表面模式也复用相同源约束。

`FlowerPart` 从 64 到 80 字节新增 float4 socket；Rust 和 Slang 字段顺序一致，ABI 回归更新。32-byte baked surface 和 UV/color atlas 没有改变，改变茎采样设置仍不重建花头 cache。

此契约解决当前单头模型从 attachment 向花头外侧穿出的茎；不声称修复所有花头离散视向／最终像素重采样的遮挡与跳变。原版 voxel A 模式保持不变，未用修复名义删除用户比较入口。未来若支持下向花头或多头分枝，应明确设计对应的接合布局，不能假定同一个全局切面仍适用。

## 绿测试与真实运行

- Slang 28 项通过。新回归覆盖六种正式 tilt、普通／细／最大半径、极短茎、弯曲风位移、test branches、24 个绕轴视点、128–2048 direction grids。验证外侧候选拒绝、plane cap、内部射线退出，以及显示样本平面跨接合面的判定。
- Node 34 项通过，发布的法线与共用生成器逐值一致且为单位向量。最终再次通过原有叶／蝶／苹果浏览器回归、无编辑器的 11 花型工作台，以及 242 个花头姿态／8 遮挡夹具；日志在 `target/stem-socket-web/`。
- cargo fmt/check 通过；cargo test 1260 passed、4 ignored。追加法线数据拒绝校验、80-byte ABI 和每个 FlowerPart 的上传值检查。
- 隐藏 muted Release smoke 通过，shutdown failures=0。
- `env -u WAYLAND_DISPLAY node scripts/validate-stem-sampling.mjs --seconds 70`：四个 renderer 捕获、16 个 live phase、固定位置转头／orbit、风、半径／密度端点、近裁面／resize、切回 original 通过；没有 ERROR/panic/VUID，stem 切换没有重建花头 bank。
- 查看 `target/stem-sampling-review/stem-direction.png` 的实际 native 结果。原先的细枝 direction 采样缺失仍可见，未用加宽／时间滤波隐藏；本修复不声称另行通过美术或 Release 性能接受。
- 保留用户已经保存的 `config/gui.toml`（experimental=true、direction=550、radius=1.33、surface cell=1.43），它是任务开始前的工作区改动，不提交／重置／改默认值。native 验证前后 GUI hash 一致。

日志：`target/no-sticker-editor-*.log`、`target/stem-socket-{red,slang,node,fmt,check,rust-tests,smoke,native}.log`。native 图片与 summary 在 `target/stem-sampling-review/`，不提交截图。
