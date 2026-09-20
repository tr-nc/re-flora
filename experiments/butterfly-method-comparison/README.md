# 蝴蝶实时像素翼面

**当前入口：[精简调试台](comparison-v6.html)**（目录首页也跳转到这里）。

游戏现在只使用[实时翼面渲染器](../../docs/research/butterfly_game_pixel_renderer.md)。旧手绘 Aseprite/PNG、实验中的参考副本、手绘对照页面/GIF、分析脚本及运行时 spritesheet/色块链路均已退役；没有旧渲染回退开关。

## 当前源与工具

- `blender-v5/`：已批准的无身体双翼模型、动画及制作/验证记录。
- `export-runtime-mesh.py`：从批准源导出 `assets/butterfly/wing-mesh.json`，游戏实时摆翼并按每只蝴蝶的固定 N×N 分辨率采样，不读取方向图集。
- `comparison-v6.html`、`debug-preview.js`、`custom-color-picker.js`：同步相机的原始/像素双预览，共用翼面颜色，自定义 HSV/HEX 色盘，8–64px、2–60 FPS；见 [使用与验证](debug-preview.md)。网页临时设置不自动同步游戏。
- `validate-debug-preview.cjs`：浏览器交互、尺寸和像素输出检查。
- `../../scripts/validate_butterfly_mesh.py`：实际游戏 GPU 原生像素、深度与自阴影检查。

```sh
python3 -m http.server 8788 --bind 127.0.0.1 --directory experiments/butterfly-method-comparison
```

打开 `http://127.0.0.1:8788/`。

## 保留的建模研究记录

`blender/`、`blender-v2/`、`blender-v3/` 是三维模型迭代及离线采样研究，不是游戏回退资源；`blender-v4/` 和 [v4 页面](comparison-v4.html) 记录无身体任意视角预览；[v5 页面](comparison-v5.html) 记录已弃用的浏览器边框实验。AI 候选只保留为失败案例记录。

历史 Markdown、日志与 JSON 中的旧路径和验收结论是当时的记录，不代表旧手绘资源或旧对照页面仍存在。新制作流程与当前入口以上文为准，不再维护手绘参考链路。

- [实时局部像素管线调研](../../docs/research/butterfly_realtime_pixel_pipeline.md)
- [原生分辨率研究](resolution-study/README.md)
- [Blender 制作案例研究](blender-v2/research-3d-to-pixel.md)

重新导出模型时使用对应版本脚本的 `--export-existing` 以保留手工修改的 `.blend`；不带该选项会从脚本重建模型。依赖 Blender、Python/Pillow、ffmpeg。第三方库及许可证仍随目录保留。
