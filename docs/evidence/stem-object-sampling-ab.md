# 花茎采样契约：角度 A / 固定物体 B

当前入口见 [`stem-selector-controls.md`](stem-selector-controls.md)：World-direction 从统一 Flower stems selector 选择，不再使用总 experimental checkbox；下面保留原始实现记录。

## 玩家入口

**R → Debug → Pixel Sampling — Flower Stems**，启用 experimental，选择 **World-direction pixels**。

- `World-direction stems: fixed object pixels (B; off = angular A)`：实时复选框；**不勾选 = 原来的角度采样 A**，勾选 = 固定物体画布 B。默认不勾选，不擅自替玩家选最终方案。
- `Flower stems A: direction cells per cube face`：仅 A 使用原 direction resolution。
- `Flower stems B: pixels per complete stem (fixed object grid)`：仅 B 使用，每完整茎一个正方形源网格，默认 **256×256**，范围 32–512；包含主茎及启用的 test branches，不是每段／分枝各 N×N。
- 两项新增字段都声明在 `config/gui.toml`，走统一 SavedControls／自动生成映射和保存；没有 App-only 状态或专用保存钩子。其他采样方法不受这个复选框影响。

用户原有 experimental=true、direction=550、surface cell=1.33、radius=1.43 的工作区保存值保留；提交只包括新声明和 A 控件标签，不提交这些已有私人调整。

## 契约差异

A 保留原相机原点、固定世界 cubemap 角度格：摄像机后退时物体的角度面积缩小，所覆盖的源方向格减少。B 的正交源射线来自**物体自身固定画布**，不再来自相机。

B 复用花头的：

1. 实际已发布 view count 与 `canonicalModelView` / `nearestModelView`，而不是另建一份方向表。
2. `modelOrthographicQuad` 的正交物体图像显示：相机远近改变显示大小，不改变源网格 N。
3. `modelOrthographicDepth` 的源深度 → 场景深度转换。
4. 抽出的共用 `modelObjectRoll`（花头和茎都调用），原地转头／屏幕 roll 只改变显示投影。

茎的 canonical 方向保持世界取向，不跟着屏幕旋转。改变观察位置可能切换离散 source view，正如花头；**固定预算并不表示任何视角都使用同一张内容图**。

## 完整茎的固定源图像

`stemObjectFrame` 根据 authored height、rest bend、半径、分枝范围，加上已有 maximum wind displacement 得到旋转安全球。它不读取相机，也不使用当前 wind pose 的外轮廓来撑满画布。风改变内容但不改变画布范围；增长／整体 scale 按物体自身变化。

源图像是一个明确的 N×N 逻辑网格，`stemObjectCell` 将显示 UV 映射到固定源 cell center；射线从 `center + x*u + y*v + z*radius` 沿 `-z` 查询同一个生产 `traceStem`。源 origin 与 direction 不含相机位置或距离。采用**按需求值**：每个屏幕 fragment 查询对应源 cell 的解析颜色／深度，不为每株分配 N² atlas，也不宣称每帧实际执行 N² 次 tracing。对于固定姿态／source view，任意 cell 的数据等价于预先生成固定源图再 nearest-display。

接合面截断、原始半径／分枝／风和真实 shading 仍共用上一轮修复。B 对实际重投影显示点继续检查 socket 内侧，不能因 billboard 深度投影又穿出花头。没有加宽细茎、降低深度把绿色藏到花瓣后面、TAA 或随距离增加样本数。

切换 A/B 或改变 B 的 N 只更新 live uniforms。现有 `flower_stem_shape.w` 从保留值变为 B resolution（A／非 direction 模式为 0）；GPU GUI struct ABI 不变。共享 view/cache buffers 通过 frame-owned descriptors 绑定，花头缓存无需重建。B culling 增量是源 cell 的空间边界，而非 A 随相机距离增长的 angular padding。

## 已验证

- `cargo fmt --check`、`cargo check`。
- `cargo test`：1262 passed、4 ignored（新增契约策略和 review 配对测试）；全量 saved-control round-trip 与 live render frame mapping 也覆盖新字段。
- Slang 29 项通过：新 `flower_stem_object_grid.slang` 使用生产 object frame/cell/ray-origin、trace 及共用 quad/depth 函数。在 32/128/256² 与三个 camera distance 下比较实际源命中数量和深度和，完全一致；不是只检查 resolution 设置值。另验证 A 的角度格数量随远近减少、B 包围不会随风呼吸、源球包含主／分枝、socket 和有效深度。
- `env -u WAYLAND_DISPLAY node scripts/validate-stem-contract.mjs --seconds 70`：4 张真实 Release A/B 近／远截图，12 个 live phase，所有六种花实际 draw；1x／2x／4x dolly 保持 B 256²，原地 turn、orbit、wind、32/512² 端点、resize、切回 A 通过；GUI hash 不变，head bank 只构建一次。
- `scripts/validate-stem-sampling.mjs --seconds 70`：原有 original / continuous / direction A / surface 四 capture 与 16 phase 回归通过。
- 隐藏 muted Release smoke 通过。B 开启 `RE_FLORA_MODEL_CACHE_REVIEW=1`：花缓存检查 3,145,728 个记录，mismatches=0；叶／苹果／蝶三个 bank 也为 0，shutdown failures=0。
- Native 首次 B preflight 暴露 reflected transient descriptor set 1 缺一项；SPIR-V reflection 确认 Slang 保留公开 RW `model_cache_validation`。修正为绑定完整 `CacheFrame::bindings()`，不放宽 descriptor 数量检查。后续 A/B、旧模式与 cache review 全部无 ERROR/panic/VUID。
- 已阅读实际近／远截图。128² 初版近处极细末端存在源中心采样缺失，默认提高到 256² 后重跑四 capture／12 phase 和 Slang；不是把远处分辨率动态提高。画面未自动作为最终美术或性能接受。

日志：`target/stem-contract-{check,fmt,tests,slang,smoke,native,cache-review,old-modes}.log`。
最终图像／阶段报告：`target/stem-contract-review/{a-near,b-near,a-far,b-far}.png`、`sweep.log`、`summary.json`。截图不进 Git。
生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，由 cargo check 更新，未手改。

## 仍然存在的边界

- **固定源分辨率不等于固定屏幕像素数量**：远处整根茎窄于一屏幕像素，nearest-display 仍可能跳变／断续。最终远图确实还能看到细茎断续，不能声称完全消除了 aliasing。后续缩小过滤需要单独视觉实验。
- B 是解析中心采样，并非花头 triangle conservative coverage 的完整复制；很细的 test branch／低 N 的末端仍可能在源图中缺失。不同 source view 的内容／命中数量也可以不同，固定的是每个 view 的完整物体网格预算和尺度。
- 最近 canonical view 的切换可能 pop，保留 wind 最大范围会让静止细茎只占源画布的一小部分；B 的物体图像深度／socket 裁切也可能改变接合处像素边界。都没有隐藏特例。
- 不分配 per-plant atlas，不代表免费：按需 tracing 与新增 view selection / 投影仍需 GPU 工作。截图 HUD FPS 不是同场景性能 benchmark；没有旧／新 Release timing 对照，不声称性能接受或优化成功。
