# v3 对照页验证

页面：`../comparison-v3.html`。静态资源全部随仓库提供，不依赖 CDN。

## 实际完成的检查

- HTML 静态本地链接存在，内联 JavaScript 经 `node --check` 通过。
- 使用 Playwright 1.63 + headless Google Chrome / SwiftShader 加载实际本地 HTTP 页面。
- **100 次 canvas 内容检查**：四组图 × 五方向 × 五相位，与源图集相应 crop 的完整 canvas 数据一致。
- **25 次 GLB 时间检查**：五方向×五相位，模型 `currentTime` 与 sprite 的 0/.2/.4/.6/.8 秒一致。
- 点击关键帧会暂停；下一帧与第 5 帧环回正常；播放与速度选项正常。
- 背景切换、192px 显示尺寸、390px 视口无整页横向溢出通过；系统 reduced-motion 时默认暂停。
- 最终测试：**console errors 0，失败网络请求 0**。
- 检查 1280×900 桌面完整截图、390×844 窄屏完整截图和模型区域截图。截图在当前 worktree `target/butterfly-resume/v3-{desktop,mobile,model,paper}.png`，测试结果在 `browser-v3-validation.json`。

## 测试中修正的实际问题

1. model-viewer 不选动画 clip 时 `duration=0`，只设置 `currentTime` 不会同步动画。现在 load 后明确选择导出的 `Scene` clip，等待组件更新再暂停并设置相位；时间检查通过。
2. 默认相机最大距离限制会把请求的 5.8m 钳到约4.67m，模型在窄屏被裁切。现在明确放宽最大轨道距离和最大视场角；最终模型区域截图确认留有外边距。sprite 相机与输出完全未改。
3. GLB 设置 eager 加载，不必等用户滚动到三维区域才初始化。

浏览器工具能读页面树但截图/脚本工具失败，因此截图和可重复断言改用独立 headless Playwright；没有自动化用户 Zen 的已有页面。这些检查证明页面功能与源图一致，不等同于用户批准新的美术效果，也没有验证游戏内观感。

## 查看

```sh
python3 -m http.server 8793 --bind 127.0.0.1 \
  --directory experiments/butterfly-method-comparison
```

打开 `http://127.0.0.1:8793/comparison-v3.html`。默认 45°、5fps、96px 显示尺寸；重点比较中间的 v2/v3 12px，之后看135°。模型随相位离散播放，可拖动旋转，不以连续 3D 动画代替像素验收。
