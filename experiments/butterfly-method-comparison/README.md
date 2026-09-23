# 蝴蝶实时像素翼面

**唯一预览工具：[模型像素化预览台 · 蝴蝶](../model-preview/?model=butterfly)**。目录首页只保留跳转，不再保留带版本编号的比较页面或独立蝴蝶查看器。见 [工具说明与模型接入接口](../model-preview/README.md)。

游戏现在只使用[实时翼面渲染器](../../docs/research/butterfly_game_pixel_renderer.md)。旧手绘 Aseprite/PNG、实验中的参考副本、手绘对照页面/GIF、分析脚本及运行时 spritesheet/色块链路均已退役；没有旧渲染回退开关。

## 当前源与工具

- `blender-v5/`：已批准的无身体双翼模型制作源与历史输出。当前正式资产为 `../../assets/models/butterfly.glb`，游戏和网页直接共用；见[共享资源约定](../../assets/models/README.md)。
- `export-runtime-mesh.py`：从批准源导出 `assets/butterfly/wing-mesh.json`，游戏实时摆翼并按每只蝴蝶的固定 N×N 分辨率采样，不读取方向图集。
- `../model-preview/models/butterfly.js`：使用同一个批准 GLB、原动画、统一翼色与光照/阴影；相机、双视图、时间轴、8–128px、2–60 FPS、HSV/HEX、像素处理及预设均由统一工具维护。网页参数不自动同步游戏。
- `../model-preview/tests/browser.cjs`：当前两模型共用的浏览器验证；`validate-debug-preview.cjs` 只是兼容转发。迁移时 24 个蝴蝶 A 样本与旧版逐字节一致。
- `../../scripts/validate_butterfly_mesh.py`：实际游戏 GPU 原生像素、深度与自阴影检查。

```sh
node scripts/serve-model-preview.mjs
```

打开 `http://127.0.0.1:8765/model-preview/?model=butterfly`。使用统一服务，同时提供预览目录与 `assets/models` 正式资源。

## 保留的建模研究记录

各 `blender*` 目录保存模型源与离线采样记录，内部资源路径维持不变，不是用户需要选择的网页版本或游戏回退模式。AI 候选只保留为失败案例记录。

旧比较页面、边框实验及其独立脚本不再保留在当前树中，可在提交 `86b4b81c` 查看。当前只维护统一工具。

历史 Markdown、日志与 JSON 中的旧路径和验收结论是当时的记录，不代表旧手绘资源或旧对照页面仍存在。新制作流程与当前入口以上文为准，不再维护手绘参考链路。

- [实时局部像素管线调研](../../docs/research/butterfly_realtime_pixel_pipeline.md)
- [原生分辨率研究](resolution-study/README.md)
- [Blender 制作案例研究](blender-v2/research-3d-to-pixel.md)

重新导出模型时使用对应版本脚本的 `--export-existing` 以保留手工修改的 `.blend`；不带该选项会从脚本重建模型。依赖 Blender、Python/Pillow、ffmpeg。第三方库及许可证仍随目录保留。
