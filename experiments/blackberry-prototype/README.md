# 野生黑莓形态研究

从仓库根目录运行（不需要 npm install）：

```sh
node scripts/serve-model-preview.mjs --port 8767
# http://127.0.0.1:8767/blackberry-prototype/
```

需支持 WebGL 的浏览器；ES modules 需经 HTTP 访问，不能直接双击 `index.html`。依赖仓库现有的 [Three.js 0.183.0](../butterfly-method-comparison/vendor/three-0.183.0/README.md)（[MIT 许可](../butterfly-method-comparison/vendor/three-0.183.0/LICENSE)）；无 CDN、外部字体、在线图片或构建步骤。移动／复制时需保留相对 vendor 路径。

[调研与一手来源](../../docs/research/wild-blackberry.md)。HTML 下半页是可视化调研摘要。本轮是待评审的独立视觉研究，不是游戏资产发布；不修改游戏配置。尺度、风与像素预览的边界标注在页面内。

## 复核

```sh
node --test experiments/blackberry-prototype/tests/model.test.mjs
# 先启动上述服务器；浏览器检查另需已安装的 agent-browser：
node experiments/blackberry-prototype/tests/browser-check.mjs
# 可用 BLACKBERRY_URL 指定其他本地端口。
```

浏览器检查使用独立会话，截图写入忽略目录 `target/blackberry-prototype/`。这是网页交互／WebGL 检查，不是原生 Release 性能测试。视觉判断仍需打开网页或查看截图。

## 图形来源

- `model.mjs` 是本次原创的确定性形态配方；`viewer.mjs` 程序化绘制枝、叶、花、果、地台和人形尺标。参数与种子均在源码中，可按上述命令复现。
- `index.html` 的三幅内联 SVG 是本次为调研说明绘制的示意图，不是植物照片或精确测绘。
- 未导入或生成独立图片／音频资源，未调用图像／音频生成服务，未复制参考网站的网格、贴图、照片。在线照片的出处与未确认的再分发权限见调研笔记；这些照片不在本目录、网页或发布包内。
- 浏览器测试生成的 PNG 仅为本地网页截图，不是额外模型资源。若以后选入仓库作为证据，应随截图记录对应提交、视口、阶段与生成命令。
