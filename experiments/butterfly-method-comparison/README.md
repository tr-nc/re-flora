# 蝴蝶实时像素翼面

**当前入口：[统一模型像素化预览台 · 蝴蝶](../model-preview/?model=butterfly)**。目录首页和 `comparison-v6.html` 均只保留跳转，当前场景逻辑已迁移，不再维护独立蝴蝶查看器。见 [工具说明与模型接入接口](../model-preview/README.md)。

游戏现在只使用[实时翼面渲染器](../../docs/research/butterfly_game_pixel_renderer.md)。旧手绘 Aseprite/PNG、实验中的参考副本、手绘对照页面/GIF、分析脚本及运行时 spritesheet/色块链路均已退役；没有旧渲染回退开关。

## 当前源与工具

- `blender-v5/`：已批准的无身体双翼模型、动画及制作/验证记录。
- `export-runtime-mesh.py`：从批准源导出 `assets/butterfly/wing-mesh.json`，游戏实时摆翼并按每只蝴蝶的固定 N×N 分辨率采样，不读取方向图集。
- `../model-preview/models/butterfly.js`：使用同一个批准 GLB、原动画、统一翼色与光照/阴影；相机、双视图、时间轴、8–128px、2–60 FPS、HSV/HEX、像素处理及预设均由统一工具维护。网页参数不自动同步游戏。
- `../model-preview/tests/browser.cjs`：当前两模型共用的浏览器验证；`validate-debug-preview.cjs` 只是兼容转发。迁移时 24 个蝴蝶 A 样本与旧版逐字节一致。
- [v6 历史验证记录](debug-preview.md)：描述迁移前的界面，不作为当前功能清单。
- `../../scripts/validate_butterfly_mesh.py`：实际游戏 GPU 原生像素、深度与自阴影检查。

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory experiments
```

打开 `http://127.0.0.1:8765/model-preview/?model=butterfly`。请服务整个 `experiments`，不再仅服务蝴蝶子目录。

## 保留的建模研究记录

`blender/`、`blender-v2/`、`blender-v3/` 是三维模型迭代及离线采样研究，不是游戏回退资源；`blender-v4/` 和 [v4 页面](comparison-v4.html) 记录无身体任意视角预览；[v5 页面](comparison-v5.html) 记录已弃用的浏览器边框实验。AI 候选只保留为失败案例记录。

旧 v6 场景脚本与色盘源可在提交 `86b4b81c` 查看；当前已删除重复实现。v4/v5 页面仅为历史研究，不再新增功能。

历史 Markdown、日志与 JSON 中的旧路径和验收结论是当时的记录，不代表旧手绘资源或旧对照页面仍存在。新制作流程与当前入口以上文为准，不再维护手绘参考链路。

- [实时局部像素管线调研](../../docs/research/butterfly_realtime_pixel_pipeline.md)
- [原生分辨率研究](resolution-study/README.md)
- [Blender 制作案例研究](blender-v2/research-3d-to-pixel.md)

重新导出模型时使用对应版本脚本的 `--export-existing` 以保留手工修改的 `.blend`；不带该选项会从脚本重建模型。依赖 Blender、Python/Pillow、ffmpeg。第三方库及许可证仍随目录保留。
