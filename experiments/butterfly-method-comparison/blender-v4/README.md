# v4：纯翼面模型与任意视角源

按用户要求，从源模型中删除头、胸、腹部及相应跟随节点，不是隐藏网格或只改 PNG。源工程只剩左右两块翼面网格、共享整体运动节点和两个翼铰链。保留 v3 的翼形与拍翅参数；暗色身体已不存在。

历史 v1/v2/v3 保留。新的实时预览使用本目录 GLB 的真实几何与动画，不使用固定方向图集选帧；离线的五方向图集现在仅用来做回归检查和历史对照，不限制相机方向。

## 复现与验证

```sh
# 从配方重建（会覆盖 v4 源工程）：
/usr/bin/python3 experiments/butterfly-method-comparison/blender-v4/run.py --rebuild
# 保留手动修改，仅导出现有工程：
/usr/bin/python3 experiments/butterfly-method-comparison/blender-v4/run.py
```

Blender 4.5.13 LTS、系统 Python + Pillow；可以用 `BLENDER` 环境变量覆盖可执行文件。`--draft` 不做独立重放，不能作为完整验收。

完整运行已通过：90 帧独立重放一致，15 组循环端点一致，75 个正式采样帧外边界透明，12/16px 索引 PNG 的 5 项 PLTE 与二值 alpha 合法（身体去掉后无需使用全部五个索引）。

GLB 验证：恰好两个网格节点，名称均为 wing；无 head/thorax/abdomen/body 节点；动画仅保留整体 translation/rotation 和左右翼 rotation，共 4 条通道。导出前后 `.blend` 哈希一致。详细数据和产物哈希在 `validation.json`，创建配方说明在 `source-manifest.json`。

未修改游戏、Rust 或 shader，也未作性能声明。实时网页是后续交付，非游戏已经接入。纯翼面可能在极端侧缘/合翼时消失成少数像素；不会用随相机转动的假翼面掩盖这类限制。
