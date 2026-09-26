# v5：可调上/下表面材质的纯翼面源

保留 v4 的两块翼面几何与动画；没有身体。删除上表面的前部异色色块和原有彩色翼框：所有上表面面片统一使用 `Wing upper`，下表面及薄侧缘统一使用 `Wing lower`。两者默认同为 `#50bedc`，网页可独立修改。

v4 镜像网格过去只用自发光材质，绕序不影响明显的明暗；v5 为真实光照重算闭合翼面的外向法线，并验证两侧所有上表面面片法线的局部 z 分量为正。材质归属是显式的面片标记，不按观察方向临时猜测上/下表面。

## 源验证

```sh
# 会重建本目录 .blend：
/usr/bin/python3 experiments/butterfly-method-comparison/blender-v5/run.py --rebuild
# 保留手动源文件编辑，仅导出：
/usr/bin/python3 experiments/butterfly-method-comparison/blender-v5/run.py
```

Blender 4.5.13 LTS / 系统 Python + Pillow。实际完整复跑：90 帧重放一致，15 组循环端点相同，75 个正式采样帧边界透明；GLB 恰好两块翼面源网格、4 条动画通道，只有 `Wing upper` / `Wing lower` 两种材质，无身体节点。导出过程不改写保存的源工程。数据见 `validation.json`。

离线图集继续采用无光照的自发光色块，作为几何/循环回归样本；网页里的方向光、阴影与边框是独立实时显示设置，不声称已烘焙进这些 PNG。离线索引图仍保留原调色板合同，不要求用满所有索引。

当前网页入口是[统一模型预览台](../../model-preview/?model=butterfly)，使用本目录批准源但不显示模型版本编号。旧编号网页及边框实验已移除；当时的技术与检查仅作为[历史页面验证](browser-validation.md)保留，不是当前界面功能清单。

历史 v1–v4 保留，游戏代码和正式素材未改变。
