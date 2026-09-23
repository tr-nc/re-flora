# 精简调试台（v6，历史记录）

> 已迁移至 [统一模型像素化预览台](../model-preview/README.md)。以下记录适用于提交 `86b4b81c` 中的旧 v6；其中旧脚本路径与独立服务方式不是当前入口。现在 `comparison-v6.html` 只跳转，`validate-debug-preview.cjs` 转发统一验证。

入口 `comparison-v6.html`，场景逻辑 `debug-preview.js`，自定义色盘 `custom-color-picker.js`。

## 当前界面

- 只有顶部操作栏和左右两个画布：左原始三维，右像素化。没有介绍文案、历史链接、制作记录链接或页脚。
- 删除边框全部 UI **以及当前渲染路径中的边框 pass**。低分辨率画布直接绘制场景；不加载 `pixel-outline.js`，不是仅把其开关默认设为关。
- 蝴蝶只有一个颜色，两面及侧缘使用同一颜色。复用 v5 无身体翼面 GLB，其两类面片材质在运行时接受同一个颜色；无需重新生成几何。
- 蝴蝶和背景都使用自定义连续 HSV 色盘：二维饱和度/明度区域、色相滑杆、可键盘操作的 S/V 滑杆及 HEX 文本。无预设色块、无原生 `input[type=color]` 系统弹窗。支持 #RGB / #RRGGBB，无效文本不改变颜色。
- 分辨率仍为8–64px；动画改成2–60 FPS滑杆，全程固定采样：`floor(phase * fps) / fps`。60是最右端、默认值，无连续/离散分支。FPS改变动画采样密度，不改变一秒循环时长；镜头仍随交互实时更新，不限制到采样方向。
- 保留同步拖动、光照/阴影、投影、相位及透明 PNG 导出。参数不保存，工具栏 title 提示刷新重置。
- 布局按可用宽度**和高度**计算两个画布的共同显示尺寸，正常情况下整数最近邻放大；极小窗口空间不足时缩小到可用区域，不靠藏住内容实现免滚动。

## 验证

```sh
python3 -m http.server 8793 --bind 127.0.0.1 \
  --directory experiments/butterfly-method-comparison
# 另一个终端，Node 能解析 Playwright：
node experiments/butterfly-method-comparison/validate-debug-preview.cjs
```

支持外部 `NODE_PATH` 和 `PREVIEW_URL`、`PREVIEW_ARTIFACT_DIR`、`CHROME_EXECUTABLE`。本次实际为 Playwright 1.63、headless Chrome/SwiftShader、DPR=2。

- 2–60全部59个采样帧率逐项核对公式，两幅画面的实际动画时刻一致。
- 8–64全部57个原生缓冲尺寸与同屏布局通过。
- 自定义色盘任意 HEX、短 HEX、无效输入保护、HSV滑杆和二维区域取色通过；同一个值更新所有翼面材质。
- 无光照时单一不透明颜色，无边框；背景色不进入 RGBA 导出；光照开启产生真实几何明暗。
- 双向相机拖动、投影切换、播放与 PNG 导出通过。
- 1280×720、1024×600、390×844、844×390：两幅画布左右排列、完整位于视口内，页面无横向或纵向滚动；色盘弹层也在视口内，可用 Escape 关闭。
- 页面无链接、页脚、边框控件、上下表面独立控件、恢复视角按钮或采样下拉选项；网络不请求旧边框模块。
- 正常测试 console errors=0、失败请求=0。JavaScript语法与本地引用检查通过。

结果与截图位于当前 worktree `target/butterfly-resume/v6-browser/`。自动化使用独立 Chrome，Zen 用于用户查看。没有改 Rust/Slang、游戏素材或源模型，没有游戏运行或性能结论；FPS是采样设置，不是实测 GPU 帧率承诺。

v1–v5 保留作仓库内历史记录，但不再出现在当前调试界面中。
