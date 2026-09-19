# 接手复验与低分辨率诊断

## 本轮实际做了什么

- 归档基线提交：`39ece7f0`。本轮未改 Rust、shader、游戏素材或 v2 源模型。
- 在 `target/butterfly-resume/blender-v2/` 副本运行 `/usr/bin/python3 run.py --export-existing`，使用 Blender 4.5.13 LTS、系统 Python Pillow 11.3.0、ffmpeg。
- 75 张独立重放逐像素一致，75 张边界检查通过，128px/16px 循环端点相等；源 `.blend` 和四张蓝/灰 16/32px 图集与归档逐字节一致。
- 导出 GLB 与 256×256、15fps 转台视频成功；扫描本轮日志未发现 error/traceback/exception/failed。验证数据保存在 [replay-validation.json](replay-validation.json)。原始运行日志留在当前 worktree 的 `target/butterfly-resume/`，未混入历史日志。
- 迁入检查：550 个未修改的交付文件与外部来源逐字节一致；HTML 静态本地链接存在，Python 语法与 JSON 解析通过。未重新做浏览器交互验收，未运行游戏；本轮没有 Rust/shader 变更。

## 游戏实际消费方式（代码检查，不是屏幕测量）

- `src/particles/animation.rs`：单帧 **16×16**，5 帧，0.2 秒/帧；只取原图第 1、3 行（零起点），对应两种逻辑方向。
- `src/tracer/resources.rs`：加载 `assets/texture/butterfly_16px/butterfly.png`，按固定帧尺寸取图并重映射调色板。12px 图集不能直接替换现有输入格式。
- `src/particles/system.rs`：普通与 guided-flight 模式的显示尺寸路径不同；guided sprite 用 `STANDARD_PARTICLE_SIZE` 和 2.1 倍覆盖率补偿。配置里的 `butterfly_size=0.03` 不能概括所有模式的实际显示尺寸。
- 因此“看起来分辨率高”尚不能只归因于源图大小；本轮未声称测出相机距离、屏幕占用或游戏内实际观感。

## 同源 8 / 12 / 16px 实际渲染

[静态逐帧对照](native-resolution-contact.png) · [含原手绘的 5fps 动画对照](resolution-comparison.gif) · [测量](metrics.json)

不是把 16px 图缩小：重新打开保存的 `.blend`，保留相同相机、2.95 正交尺度、几何、动画、材质，直接在三种分辨率渲染五个方向。每个方向额外渲染第 26 帧检查循环端点。复用 v2 的调色板映射；展示时统一放到 96px 格子，最近邻缩放，避免以缩小显示尺寸假装降低像素密度。原画行顺序是参考，不表示与 Blender 相机严格等价。

| 原生单帧 | 最少不透明像素 | 最薄姿态高度 | 所有帧边界透明 |
| --- | ---: | ---: | --- |
| 8px | 8 | 2px | 否 |
| 12px | 16 | 3px | 是 |
| 16px | 30 | 3px | 是 |

三种尺寸、五方向的首尾帧都相同；本轮 16px 图集与归档 v2 的最终蓝色图集 RGBA 逐像素相同。注意：8px 触边是这个新诊断候选的失败，不是归档 v2 回归；不得当作可接入素材。

### 看图后的判断

已检查静态完整 contact sheet：

- 8px 的粗块感最明显，但身体与翼面的区分进一步消失，少数斜向帧变成零散亮块；不建议直接降到 8px 上线。
- 12px 比 16px 更简洁，值得作为下一轮设计目标，但仍继承 v2 的薄横条过渡姿态与斜向碎块。降低分辨率本身没有解决动作设计。
- 16px 保留更多轮廓细节，但 v2 的几何投影仍不如专门设计的像素关键姿态可控。
- GIF 已生成，但没有通过浏览器动态播放检查；静态图判断不能替代节奏与游戏内观感验收。

**建议下一步先做针对 12px 的翼形/关键姿态迭代，保留 16px 对照。不要先做全套通用导出系统，也不要立即换掉游戏原画。** 这是实验优先级，不是最终分辨率定案。

## 复现

在仓库根目录：

```sh
BLENDER="$HOME/.local/opt/blender-4.5.13-linux-x64/blender"
"$BLENDER" --background --factory-startup \
  --python experiments/butterfly-method-comparison/render-resolution-study.py -- \
  --output target/butterfly-resume/resolutions
/usr/bin/python3 experiments/butterfly-method-comparison/assemble-resolution-study.py \
  --input target/butterfly-resume/resolutions \
  --output experiments/butterfly-method-comparison/resolution-study
```

渲染脚本不保存 `.blend`；拼图脚本会覆盖本目录的图集、GIF、contact sheet 与 metrics，不会更新独立的完整复跑记录。默认 `python3` 在本机环境中没有 Pillow，明确使用 `/usr/bin/python3`。
