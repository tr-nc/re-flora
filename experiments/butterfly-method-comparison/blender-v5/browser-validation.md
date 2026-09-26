# v5 材质、阴影与像素边框

> 历史资产验证记录：下列旧页面与脚本已从当前树移除，可在提交 `86b4b81c` 查看。当前使用[统一预览台](../../model-preview/README.md)，没有编号网页入口。

页面：`../comparison-v5.html`。共享场景/相机逻辑在 `../material-preview.js`，低分辨率目标与边框 GPU pass 在 `../pixel-outline.js`。

## 控制与语义

- 分辨率改成 **8–64、步长1** 的动态滑杆，显示当前 N×N；高/低画布保持同取景与整数放大。
- 边框开关、颜色、独立 alpha 滑杆（0–1）；勾选“向外”在原轮廓之外扩一层，取消则覆盖已有轮廓内侧一层。使用整体 alpha 轮廓的八邻域，不对翼面内部色差描边。
- 外扩不改变原有翼面像素；内侧模式在原有颜色上按 alpha 混合，不降低原有覆盖率。边框宽度为一个最终输出像素。极低分辨率或细轮廓上，内侧模式可能覆盖大部分色块；向外模式可能在画布边缘被裁切，未做自动缩放规避。
- 上表面与下表面各一个颜色选择器，上表面已没有旧的前部色块；薄侧缘属于下表面。材质保持不透明，alpha 控制只属于像素边框。
- 背景改为 Color Picker，两个区共用，仅作 CSS 预览底色，不写进 PNG。
- 阴影开关明确为“**三维光照与阴影**”：开启时采用 MeshStandardMaterial + 固定场景方向光 + 环境补光 + 1024² PCF shadow map，翼面投射并接收阴影；关闭时关闭灯光/阴影并以自发光显示纯材质色。没有地面承影平面；包含的是法线明暗与翼面间/自身遮挡。
- 恢复视角按钮已删除；拖动、键盘旋转、缩放、相位与投影控制仍可用。
- 桌面控制区随滚动停靠，并用动态 scroll margin 避免遮住获得焦点的画布；窄屏不启用停靠。参数是临时实验值，刷新重置，页面明确显示不保存。

## 实际管线

`同一 v5 几何/动画/材质/方向光 → 无 MSAA 的 N×N 颜色+深度目标 → GPU alpha 轮廓描边 → 最近邻显示`

上区正常高分辨率 MSAA 渲染，不加像素边框。下区渲染目标与采样都用最近邻、无 mipmap。上/下区都真实计算三维光照和 shadow map，低区并非在最后把整图压暗；阴影会带来更多颜色，目前没有固定调色板量化。

边框处理在线性色彩空间混色，最后转到显示 sRGB；透明输出使用预乘约定。先对最终 RGBA8 精度取整再预乘，避免 alpha=.5 时 A=128、R误取127，导出红色变253的舍入暗边。小像素缓冲保留用于透明 PNG 导出，未读取背景，也没有每帧 CPU readback。

## 已执行验证

手动验证脚本（不接入 `cargo test`）：

```sh
python3 -m http.server 8793 --bind 127.0.0.1 \
  --directory experiments/butterfly-method-comparison
# 另一个终端，需要 Node 可解析 Playwright，系统 Python 有 Pillow：
node experiments/butterfly-method-comparison/validate-material-preview.cjs
```

支持 `NODE_PATH` 指向外部 Playwright 安装，以及 `PREVIEW_URL`、`PREVIEW_ARTIFACT_DIR`、`CHROME_EXECUTABLE`、`PYTHON`。本次为 Playwright 1.63 / headless Chrome / SwiftShader / DPR=2。

- GPU 边框逐像素对照 CPU 八邻域计算：外扩和内侧模式均通过；alpha 0/0.5/1 通过；外扩不改变原像素，内侧不改变原覆盖。
- 8到64全部 **57个** 原生缓冲尺寸通过，不受 DPR=2 放大；显示尺寸为整数倍。
- 同色、无光照时只有一种不透明 RGB，证明旧上表面分色已删除；红上表面/蓝下表面从上、下观察时分别出现正确颜色。
- 同色开启光照后产生多级明暗；两区 shadow map 开启，实际地图1024²；关闭后恢复完全相同的无光照像素。此检查不是对每个动画时刻的阴影艺术质量保证。
- 画布背景改变不改变导出的 RGBA；32×32 PNG 经 Pillow 独立解码，alpha 恰为0/128/255，半透明红边保存为(255,0,0,128)，无底色污染。
- 双向拖动同步、正交/透视、连续/5fps、任意相位、PNG导出正常；不存在恢复视角按钮。
- 检查1280×1000桌面、390×844窄屏截图及浅色背景；窄屏无全页横向溢出。测试过程中修复了停靠面板遮住画布自动滚动位置的问题。
- 最终正常测试 **console errors=0，失败请求=0**。HTML引用和 JavaScript语法检查通过。

机器结果与截图：当前 worktree 的 `target/butterfly-resume/v5-browser/`。自动化使用独立 Chrome；Zen 用于用户查看，没有声称运行过 Zen 自动化套件。

本轮仅改实验 Blender / 网页 WebGL 代码，未修改游戏 Rust/Slang 或生产素材，未运行游戏，也没有作性能声明。浏览器 shader 已通过实际编译、渲染和像素测试；没有拿网页测试替代未来的 Vulkan 集成验证。
