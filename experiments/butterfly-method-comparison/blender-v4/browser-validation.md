# v4 任意视角像素预览：验证与边界

> 历史资产验证记录：下列旧页面与脚本已从当前树移除，可在提交 `86b4b81c` 查看。当前使用[统一预览台](../../model-preview/README.md)，没有编号网页入口。

入口：`../comparison-v4.html`，逻辑：`../free-view-preview.js`。

## 实现

- 本地 Three.js r183 + GLTFLoader 读取同一个 v4 GLB，两个 WebGLRenderer 使用同一 Scene、Camera 与 AnimationMixer 求值后的场景。
- 两个 OrbitControls 直接引用同一 Camera 和 target，拖动任一个无需角度复制或互相触发事件。转镜头和拍翼独立，5fps 模式也不会把镜头锁在五方向。
- 上区正常分辨率、开启 MSAA；下区真实 N×N drawing buffer、DPR 固定为1、无 MSAA。下区 CSS 以整数倍最近邻放大，不是截图缩小，也不请求 PNG 图集。
- 12/16/24/32/48px、正交/透视、连续/5fps 动作切换，两个区共享同一动画时刻与投影。
- 模型使用已有自发光色块；没有新增光照、色调映射、调色板后处理、描边或 TAA。当前 PNG 实测只有透明、(80,190,220)、(205,246,248) 三种 RGBA 值，未混入黑色身体或背景。
- 仅小像素缓冲保留 drawing buffer，用于导出当前透明 PNG。此方便诊断的设置不是游戏性能实现建议。

## 实际验证

`validate-free-view.cjs` 是手动浏览器检查脚本，不加入 `cargo test`。需要可从 Node 解析的 Playwright 包和本机 Chrome；可用 `NODE_PATH` 指向外部安装，避免给 Rust 项目增加 npm 安装目录。

```sh
# 终端一，仓库根目录：
python3 -m http.server 8793 --bind 127.0.0.1 \
  --directory experiments/butterfly-method-comparison
# 终端二，已配置 Playwright 的 Node 环境：
node experiments/butterfly-method-comparison/validate-free-view.cjs
```

可通过 `PREVIEW_URL`、`PREVIEW_ARTIFACT_DIR`、`CHROME_EXECUTABLE` 指定地址、截图输出和浏览器路径。

已使用 Playwright 1.63、headless Google Chrome/SwiftShader、DPR=2 完成：

- 上区拖拽、下区拖拽、再上区拖拽均改变共享相机和实际像素内容；键盘方向键也能旋转。
- 两区引用同一相机和 target；渲染时刻严格相等。相位改变会改变真实像素，回到原相位恢复原像素。
- 五种分辨率 backing buffer 尺寸准确，不被 DPR=2 放大；显示尺寸为像素尺寸整数倍。
- 正交/透视、连续/5fps、任意0.237秒相位通过；48px 导出的 PNG 实测48×48并保留透明。
- 网络请求全部本地，**没有任何 atlas/PNG 加载**；正常测试 console error=0、失败请求=0。
- 1280×900 和390×844两种视口截图检查；窄屏无全页横向溢出，下区仍能拖拽。
- reduced-motion 默认暂停；模拟 WebGL context loss 显示明确错误并禁用导出，而非假装仍在更新。
- HTML 本地引用检查、JS语法检查通过。

截图、PNG 与机器结果在当前 worktree 的 `target/butterfly-resume/v4-browser/`。截图检查确认上区只有翼面，下区是硬边色块，同姿态、同取景。

## 未验证 / 已知限制

- 自动化验证运行在独立 Chrome，不接管用户 Zen；Zen 通过普通 URL 打开供用户检查，没有声称跑过 Zen 自动化套件。
- 这是单只蝴蝶的网页实验，没有地形、枝叶遮挡、多虫深度合成、运行时实例化或性能测量。未改变 Rust、shader 或生产资产，未运行游戏。
- 任意角度“能渲染”不等于每个角度的12px轮廓都好看；极薄侧缘仍可能丢失。没有逐帧按包围盒缩放、居中或相机朝向造假。
- 从 Blender Cycles 换到 WebGL 会有光栅覆盖与色彩处理差异；网页不承诺与离线12px图逐像素相同，也没有拿旧图集替代实时结果。
