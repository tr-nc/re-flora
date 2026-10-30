# 野生黑莓形态研究

从仓库根目录运行（不需要 npm install）：

```sh
node scripts/serve-model-preview.mjs --port 8767
# http://127.0.0.1:8767/blackberry-prototype/
```

需支持 WebGL 的浏览器；ES modules 需经 HTTP 访问，不能直接双击 `index.html`。依赖仓库现有的 [Three.js 0.183.0](../butterfly-method-comparison/vendor/three-0.183.0/README.md)（[MIT 许可](../butterfly-method-comparison/vendor/three-0.183.0/LICENSE)）；无 CDN、外部字体、在线图片或构建步骤。移动／复制时需保留相对 vendor 路径。

[调研与一手来源](../../docs/research/wild-blackberry.md)。HTML 下半页是可视化调研摘要。网页与原生游戏共用 `assets/models/blackberry-structure.mjs` 和 `blackberry-source.mjs`；网页的阶段、种子和参数修改仍仅用于预览，不自动发布资产或修改游戏配置。原生默认果期网格由 `node scripts/publish-flower-models.mjs` 发布。网页光照、风示意和低分辨率预览不代表原生管线。

## 复核

```sh
node --test experiments/blackberry-prototype/tests/model.test.mjs
# 先启动上述服务器；浏览器检查另需已安装的 agent-browser：
node experiments/blackberry-prototype/tests/browser-check.mjs
# 可用 BLACKBERRY_URL 指定其他本地端口。
```

浏览器检查使用独立会话，截图写入忽略目录 `target/blackberry-prototype/`。这是网页交互／WebGL 检查，不是原生 Release 性能测试。视觉判断仍需打开网页或查看截图。

## 图形来源

- `model.mjs` 是共享形态配方的兼容入口；`viewer.mjs` 从 `assets/models/blackberry-source.mjs` 取枝叶花果几何，并绘制网页地台与人形尺标。参数与种子均在源码中，可按上述命令复现。
- `index.html` 的三幅内联 SVG 是本次为调研说明绘制的示意图，不是植物照片或精确测绘。
- 未导入或生成独立图片／音频资源，未调用图像／音频生成服务，未复制参考网站的网格、贴图、照片。在线照片的出处与未确认的再分发权限见调研笔记；这些照片不在本目录、网页或发布包内。
- 浏览器测试生成的 PNG 仅为本地网页截图，不是额外模型资源。若以后选入仓库作为证据，应随截图记录对应提交、视口、阶段与生成命令。
