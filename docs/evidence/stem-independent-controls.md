# Flower stems：独立几何、着色、像素化

## 当前 UI

**R → Debug → Pixel Sampling — Flower Stems**：

- **Geometry**：base dimension scale、radius scale、two tapered test branches。
- **Shading**：Surface-attached cell shading（不勾选为连续着色），以及茎底／尖端颜色。
- **Pixelization**：World-direction pixel sampling；勾选时显示 direction cells per cube face。

两个 Checkbox 独立保存、实时组合，共四种状态。像素化量化世界方向采样射线，影响轮廓与深度，并非屏幕空间后处理；表面分格只改变着色坐标／法线，不改变几何求交或深度。两者一起启用时，在像素化射线的命中点计算分格着色。

删除 Original cube stems 下拉模式、立方体 shader/pipeline/CPU 网格生成，删除 Fixed object pixels 的控件、对象投影及采样逻辑，删除 Disable wind / rest bend 控件与对应覆盖逻辑。茎与花头始终使用正常风动／rest bend。这里只是不再提供茎专属冻结开关，并不强行覆盖全局风设置。

统一连续锥段几何，仅分配六个代理顶点。共享表面缓存只含花头三角形；高度上限用显式 layer count，不再从旧立方体 triangle count 推导。`model_flower_voxel_scale` 保留保存 ID，现用于连续茎基础尺寸，不代表立方体渲染。

## 兼容

旧 direction 模式迁移为 pixelized=true、surface_cells=false；旧 surface / original / 禁用实验迁移为 pixelized=false、surface_cells=true。旧 Fixed object 设置不再参与渲染；迁移移除旧字段。已有新 Checkbox 值优先，不会被残留旧元数据覆盖。加载不写盘；下一次正常保存清理旧字段。

保留工作区原有用户值：direction resolution=550、radius scale=1.43；原 direction 选择对应像素化开、分格着色关。

## 验证

- `cargo fmt --check`、`cargo check` 通过；`cargo test`：4 + 1269 通过，4 ignored。
- `python scripts/run_slang_tests.py`：28 个 Slang CPU 测试通过。
- `cargo run --release -- --hidden --mute --auto-exit 0.5` 通过，检查本工作区运行日志；组合验证最后一次原生日志为 `target/re-flora-logs/re-flora-20261002-223730.060-396344.log`，`failures=0`，无 ERROR / VUID。
- `node scripts/validate-stem-sampling.mjs --seconds 35`：四张组合截图、16 个实时阶段（固定／环绕／远近／近裁剪相机），正常风动、resize、每个普通阶段所有花种均提交绘制；切换不重建花头缓存。
- 输出：`target/stem-sampling-review/summary.json`、各组合 `.png` / `.log`。脚本检查正常退出 `failures=0`、无 ERROR / panic / VUID / validation warning，并验证 GUI 文件哈希未变。
- Rust 测试覆盖四种 UI 组合、隐藏分辨率值不丢失、旧字段迁移、新 Checkbox 值优先与保存重载、CPU/GPU 高度上限元数据契约。

旧证据文档记录的是历史实验；已移除原始立方体／fixed-object 专用验证脚本，统一使用上述脚本。

这不是性能验收或用户视觉批准。方向采样原有的细茎漏采样／断续轮廓仍可能出现，尤其是远距离和低分辨率；本次没有声称修复它。
