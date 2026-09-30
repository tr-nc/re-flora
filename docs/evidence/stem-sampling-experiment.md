# 游戏内植物茎采样对照

基于 [`stem_pixel_rotation_stability.md`](../research/stem_pixel_rotation_stability.md) 的第一轮视觉候选。用户已确认接受像素倾斜／变形，并要求同时比较原地转头与绕行。**已实现运行时对照，不是最终美术、无闪烁或性能验收。** 未保留二维教学 HTML。

后续固定物体网格 A/B 见 [`stem-object-sampling-ab.md`](stem-object-sampling-ab.md)。下述 World-direction 描述仍对应原角度采样 A；新复选框默认关闭，保留该比较基线。

## 使用

普通游戏按 **R** 打开 Debug 面板 → **Pixel Sampling — Flower Stems**。需要测试植物时，可用同面板 **Terrain & Plants → Plant all flowers & grasses around me**。


- **Flower stems: experimental sampling (off = original)**：默认不勾选，恢复原有完整立方格茎的独立 pipeline。勾选后切换下面的方法。
- **Continuous silhouette (reference)**：连续、渐细的茎与测试分叉；按最终屏幕像素求覆盖。
- **World-direction pixels (rotation-stable source)**：在固定世界朝向的 cubemap 方向格上采样。方块会倾斜／变形；原地转头不改变源射线集合。相机平移改变射线原点，**不承诺绕行也稳定**。
- **Surface-attached cells (continuous silhouette)**：只把材质渐变与光照采样位置／法线量化到枝条表面的纵向环和周向格子。格子跟随植物，**轮廓仍连续，不是块状剪影**。当前绿色、细茎和较平的光照下，与连续参考的差别较细微，可增大表面格子和半径观察。
- 方向格密度（每面 128–2048）、表面格大小（原茎边长的 0.25–4 倍）、连续半径倍率（0.25–2）、两条测试分叉、关闭风／自然静态倾斜均可实时调整。方向密度仅影响方向模式，表面大小仅影响表面模式。
- 冻结变形同时作用于实验茎和末端花头，避免二者脱开；不停止世界时间或花头光照，也不改变原版模式。生长／种下动画仍正常。
- 全部控件声明在 `config/gui.toml`，使用统一 Save。不会自动修改玩家保存的设置或花园。已有花高、大小、颜色分布继续生效。

对当前六种模型花统一生效，不依赖测试场景。测试分叉是视觉骨架的两条裸侧枝，不增加花头、叶子、繁殖行为或碰撞物。

## 实现与边界

- `flower_stem_geometry.slang` 是三种候选共用的连续几何：12 段主干、每条测试分叉 4 段，由端点球的凸包（圆端渐细锥）组成并集。分叉根与主干采样节点重合，渐细、静态曲线、增长尺度与当前风偏移共同决定形状。
- **这是解析表面，不是新导出的连续三角网格。** 光栅化每株的保守包围四边形，在 fragment 内解析求交；没有逐帧 SDF 重建，也没有为每株枚举姿态图集。原版三角形茎的 shader/pipeline 保留。
- 方向模式按实际世界射线选择立方体面、量化 UV，再对同一株解析几何求交。这等价于按需计算方向格里的该茎表面，不是渲染／缓存整场景的六张 cubemap，也不是完整 Texel Splatting 实现。
- 方向纹素显示为过命中点、垂直源射线的小平面。用真实显示射线与该平面的交点写当前相机的 Vulkan 深度。连续／表面模式写实际交点深度；表面量化不移动几何覆盖。所有模式走原生场景深度与花头／地形遮挡，不最后盖 RGBA。
- 代理包围盒和 CPU 视锥裁剪包含方向纹素扩展；跨近裁剪面时使用保守全屏代理并做 fragment 深度范围检查。未命中返回透明／远深度，沿用模型像素的 `LESS` 深度惯例，不依赖设备未启用的 demote capability。
- `ModelPixelFrame` 继续拥有 descriptor／draw 配对。实验 stem 只画现有顺序索引缓冲的前 6 个索引，不绑定静态茎三角形 descriptor；原版仍绑定原三角形和原 draw count。新增 pipeline 接入帧槽、resize、DDGI publication 的既有生命周期。
- 茎设置独立于花头不可变缓存配置。16 阶段 native 验证确认只在开头构建一次花头 bank，采样切换不烘焙花头。

### 已观察到的限制

- **512/面方向模式在固定截图里就会漏细枝，局部主干也有断点。** 这是源覆盖缺失，不是测试通过就消失的问题。提高密度可减轻，但不会保证任何距离都连续；没有偷偷增粗、透明覆盖或 TAA 来掩盖。
- 方向格跨面、最终屏幕重采样、风动和走动仍可能跳点。原地转头的稳定源格不等于显示结果严格无闪。
- 连续／表面模式在当前截图中保留了分叉；最终屏幕上小于一个像素的尖端仍可能混叠。表面模式的光照取样是明确的风格化近似。
- 圆端锥并集的分叉连接不是统一平滑曲面，接点可能鼓起／有法线变化。未制作专门物种分叉美术。
- 茎是 Raster Consumer，没有新增真实阴影投射／DDGI 遮挡、碰撞几何。未改变花瓣、苹果、蝴蝶等的表面缓存路线。
- 解析求交候选尚未做大规模花园 Release 性能验收，不把 CPU shader 测试或截图里的 FPS 当性能证据。先让玩家选择视觉契约，再单独测量／优化。

## 验证

```sh
cargo fmt --check
cargo check
cargo test
python3 scripts/run_slang_tests.py
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
env -u WAYLAND_DISPLAY node scripts/validate-stem-sampling.mjs --seconds 16
```

- Rust：1259 passed，4 ignored；包含保存／重载、Debug 分类唯一性、frame input 映射与实验策略边界。
- Slang：26 个 CPU 执行测试通过；新增测试直接 import 生产几何模块，检查锥面／球帽解析距离、极短粗茎、分叉连接、风动包围、6 面方向量化及保守扩展、表面格稳定性。这不是 GPU 性能测试。
- 隐藏静音 Release：四种固定截图、16 个运行时阶段通过；原地 yaw/pitch 的三次 eye 记录严格相同而 focus 改变，orbit 的 eye 确实改变。覆盖风动、最小／最大方向密度、粗细、去分叉、近裁剪、实验 draw 后 resize 和返回原版。运行日志无 ERROR/panic/VUID，退出 `failures=0`，GUI 文件 SHA-256 不变。
- 已检查四张实际游戏截图、茎部裁剪和打开的 Debug 面板截图（`debug-panel.png`）。连续／表面模式可见连贯分叉，方向模式存在上述覆盖缺失；不是美术批准。
- 自动化只证明实际模式提交、相机轨迹、资源生命周期和数学性质，**没有自动量化时序闪烁，也没有完整场景遮挡的逐像素 oracle**。

本工作区产物（不提交二进制图片）：

- `target/stem-sampling-review/summary.json`
- `target/stem-sampling-review/stem-{original,continuous,direction,surface}.png`
- `target/stem-sampling-review/stems.log`
- `target/stem-tests.log`、`target/stem-slang-tests.log`

生成文件仅由 `cargo check` 更新：`src/app/generated/gui_adjustables_gen.rs`、`src/auto-generated/gpu_structs.rs`。未自动启动可见游戏。
