# 原生模型有限视角量化

## 历史审计（基线 d9f5a66a）

结论：**基线普通 triangle runtime 已失去有限视角量化**。名字、存档字段和 Fibonacci 单测还在，不能作为运行时证据。

本研究直接依据本仓库历史/source，无外部二手资料：

- `cb46ae90` 引入 128-view live pixel previews；`9fe259dc` 将 Fibonacci count 做成保存滑杆；`64464b14` / `dfeb16c5` 引入独立 flower count；`f8999c9e` 曾恢复全局 count 32。这些是有限**球面方向数**，不是每个轴各 128 档。
- `eeb553aa^:shader/slang/model_pixel_object.slang::prepareModelObject`：每个物体从 `camera.pos - pivot` 得到观察方向，逆刚体 basis 到模型局部，遍历实际 N 的 Fibonacci bank，最大 dot 的方向胜出；严格 `>` 使相同得分取最小 index。不是全球 camera snapping，也不是世界 voxel 朝向量化。
- `eeb553aa^:shader/slang/model_pixel_views.slang` / `src/tracer/model_pixel_views.rs`：黄金角 azimuth 共用，但 latitude 依 N 重新计算；不能取固定 512-sphere 的前 N 个点。历史支持 8..512，包括 128/256。
- `eeb553aa^:shader/slang/model_pixel_projection.slang::modelObjectRoll`：最短弧将离散方向运送到真实观察方向，随后运送到 camera plane 来提取 roll。bank 本身只有方向、没有离散 roll；实例 roll 和 camera roll 连续保留，不是把 roll 忽略成恒零。
- 旧 `flower_pixel.comp.slang` / `flower_pixel.vert.slang` 以每个 head 的 `part.center_radius.xyz` 为 pivot；apples 和 mesh particles 使用各自实例中心。静态 geometry cache 可共享，但选择是每实例的，不是全场景一个 snapped direction。
- `eeb553aa` 删除 compute/cache runtime，改用 `flower_mesh.vert.slang`、`apple_mesh_{tree,dynamic}.vert.slang`、`particle_mesh.vert.slang` 和 `model_mesh.slang` 的连续 pose triangle projection；`Tracer` 把有效 count 清零，Debug 将 count 退休。
- 后续 `bc4bc878` 将 Rust azimuth 生成改成 `#[cfg(test)]`。基线 `U_GuiInput` 仍上传 count，但新 vertex graph 没有 nearest 消费者；遗留 `model_pixel_views.slang` 只由不再编译的 atlas modules 引用。

已运行 import-graph 探针，基线实报：`FAIL: native runtime vertex graph has no finite view selection: flower_mesh.vert.slang`。这区分了“只隐藏 UI”与“实际机制已丢失”。

## 实现决定

保留 scene-wide triangle raster 与像素后处理，不恢复 GPU tile/atlas/bake/relighting。一个共享 immutable 黄金角 bank + 一个纯 shader helper 完成真实 finite-direction nearest 与刚体修正：

1. 输入物体 published rigid frame、局部 rigid pivot 和真实 camera position；只在 render adapter 中用，模拟/碰撞不写回。
2. 局部观察方向选择 actual-N 球面 bank 的最大 dot，ties 取最小 index。
3. 最短弧旋转 `chosen -> actual local view` 施加到同一物体所有 positions/normals；绕 pivot，不绕每个 vertex。逆渲染 frame 下的 observer direction 因而恰为选中的 bank direction。
4. 保留平移、尺度、实例区别、growth/wind/animation 和连续 roll。真实 transformed world position 同时供 lighting 与 `camera.view_proj_mat`；深度由 hardware raster 生成，不伪造 `SV_Depth`。
5. GUI `Model View Quantization` 集中所有 controls。原连续路径默认 unchecked；保存 checkbox 和唯一 `model_pixel_view_count`（8..512）。复用老 count ID；独立 `model_flower_view_count` 只在 loader 迁移，不留第二个 owner。两者共存时全局值优先；仅 flower 值存在时继承它；旧 snap checkbox 不自动开启新的 A/B candidate。
6. 退化/非有限 camera vector 让 frame 原样返回；normalization 用 max component 缩放，避免 overflow。pole 不用 discontinuous up frame；刚体右手性保持。

## 覆盖与接口

已接入 flower heads（每个 head 的历史中心 pivot）、attached/dynamic apples、mesh butterflies 与 modeled detached leaves。纯 world/voxel terrain、tree cells/attached canopy leaves、未选择 mesh 的 square particles、stems、UI 和玩家 camera 不接入。花头和苹果均在原 vertex 入口消费；particles 保留实例 shape、palette、GLB articulation frame 与 published quaternion。没有更新模拟 pose/碰撞。

实现提交：`60a8f3e3`。`model_view_quantization.slang` 是无资源的纯 Module；`model_mesh_view.slang` 是 native Adapter，统一读取 GUI uniform 和不可变 azimuth bank。Rust `model_pixel_views::azimuths()` 现在是实际 runtime upload producer，不再是 test-only 数学。CPU `direction/nearest` 用于 fixture reference；实际 GPU 选择在公共 shader helper 内。原 atlas shader 的遗留模块不是 native renderer 的消费者，也不是本机制的 authority。

### 给 stone/controller 的稳定 interface

```slang
import model_pixel_types;
import model_mesh_view;
import model_view_quantization;

// physical.center: 世界实例 anchor；axes: 右手正交单位 rigid basis；scale: uniform。
// localPivot: canonical mesh 坐标；一整个 rigid object 的每个 vertex 用同一值。
ModelPixelFrame rendered = modelMeshViewFrame(physical, localPivot);
float3 position = modelViewWorldPoint(rendered, localPosition);
float3 normal = normalize(modelViewWorldVector(rendered, localNormal));
// position 同时用于 lighting 与 camera_info.view_proj_mat，保留 hardware depth。
```

在 stone 的 `prepare_draw_descriptors` resources 中追加
`self.model_mesh_frame.view_bank_binding()`：名 `model_view_azimuths`、set 1/binding 19、512 个 `float4`（8192 bytes）。在 loading warmup 完成后调用；buffer 不因 count、shape 或 resize 替换，整个 renderer 生命周期由 ModelMeshFrame 持有。共享现有 set 0 的 `U_GuiInput` / `U_CameraInfo`，读 `model_view_quantization_enabled` 与唯一 `model_pixel_view_count`，不新增 stone count、不重写 nearest math。若独立 renderer 不便借 bank，可用同一个 Rust producer 创建 immutable bank，但不要另造公式/设置 owner。

纯 helper `quantizeModelView(physical, localPivot, cameraPosition, enabled, requestedCount, bank)` 也可供不采用上述 bindings 的 Adapter 调用；`bank` 实现 `IModelViewBank`。skew/nonuniform scale 需先烘焙进 canonical positions，并以 inverse-transpose 处理 normals，不能传不正交的 axes 冒充 rigid frame。Worker 未修改 stone checkout/生成器；控制器已在 main 接入 direct stone vertex，复用此公共 frame 和相同 bank/count。其原生覆盖对照与 global-dither 联合验证见[石材集成证据](../evidence/stone-style-integration.md)。

## 验证结果

全部在本 checkout，`CARGO_BUILD_JOBS=2`；GPU runs 均用 `flock --close /tmp/re-flora-summer-gpu.lock` 串行，无可见窗口、无 push/merge。

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --check` / `cargo check` | 通过；shader reflection 自动再生 Rust ABI |
| `cargo test` | 1346 app + 4 library passed，4 ignored（其中新增跨语言编译 test 按 policy 显式运行） |
| `python3 scripts/run_slang_tests.py` | 35 Slang CPU tests passed（含 production helper 的 poles/ties/invalid/count/rigid-pivot/normal/handedness tests） |
| `cargo test slang_cpu_matches_uploaded_bank -- --ignored --nocapture` | 336 cases passed；把真实 Rust upload 的 f32 bit patterns 原样送进 Slang executable，比较 nearest index 和 rigid basis，而非让两个独立 Fibonacci 生成器自证 |
| `cargo build --release` | 通过，当前 native Release binary 就绪 |
| `cargo run --release -- --hidden --mute --auto-exit 0.5` | 正常退出；已用同 worktree log helpers 检查，无 ERROR/panic/VUID |
| `scripts/validate-model-view-quantization.sh` | 通过；四个固定比较（含重复 A）、一个实时 sweep、foreground-mask assertions、真实 apple drops 与 post-consumer resize |
| GUI / migration | 真实 egui mouse event 搜索并勾选；实际 Save/reload 128/256；唯一 owner、flower-only/global precedence、0/MAX clamp、load 不写文件通过 |

source guards 要求四个 native vertex 入口仍到达该 helper，并且几何用真实 world position 投影、bank producer 及 descriptor 接线仍在。已运行初始失败的 import-graph 探针，四条路径现在全部通过。

生成文件仅由 check/build 产生：`src/auto-generated/gpu_structs.rs`、`src/app/generated/gui_adjustables_gen.rs`。控制器整合其它 Worker 的 config/uniform changes 后应从 sources 再生成，不能手合 ABI pads。

### Native 证据（不是 AA/性能/美术验收）

- 提交的 [continuous / 128 / 256 native crops](../evidence/model-view-quantization/native-ab-128-256.png)：仅截取原生 framebuffer、并排加英文标题，无重绘/插值/离线模拟；可看到 butterfly、leaf 和 flower 的形状差异。
- 提交的 [英文 GUI 搜索](../evidence/model-view-quantization/gui-search.png)：`model view` 显示 2 controls / 1 group，checkbox 与实际 count=128。正常游戏中不需要 fixture/CLI 才能比较。
- 全尺寸图片/36-frame sequence、日志及 hashes 在 `target/view-quantization/native/`；测试日志在 `target/view-quantization/`。复跑脚本需 Vulkan/native display、pinned Slang/Rust 工具链与 ImageMagick。
- 固定首只 butterfly 在 sky 背景上的二值 foreground mask：A 与重复 A 相同，A/128/256 两两不同。它是“checkbox 和 count 确实改变 native draw”的 red-capable signal，不是颜色变化、AA 清晰度或全场景噪声指标；不是普适的艺术评价。
- 同一 native process 在 frames 0/30/60/90/120/150/180 依次发布 off-128/on-128/on-256/on-8/on-512/off-256/on-128；4 butterflies、4 mesh leaves、flower heads、13 attached apples、13 dynamic apples 都实际准备原生 triangle draw，bank binding=19。frame 90 fruit cycle 触发 production drop。
- resize 真正发布 `generation=2 extent=1023x767 tracer_generation=2` 和 `generation=3 extent=1280x720 tracer_generation=3`；所有 run `SHUTDOWN phase=complete failures=0`，无 ERROR/panic/VUID。
- 最终 sweep 原始日志：`target/re-flora-logs/re-flora-20261006-043811.367-1427303.log`；摘要 `target/view-quantization/native/sweep-summary.log`。`MODEL_VIEW_REFERENCE` 明确标记 CPU reference，不冒充 GPU index readback；GPU 证据是 shader 接线与 native framebuffer coverage。
- app runs 前后 `config/gui.toml` hash 相同。native fixtures 只改内存中的真实 saved-field inputs；测试文件的迁移/保存均在 tempdir，没有写用户 GUI 配置。

## 比较与剩余风险

正常 Debug → **Model View Quantization**（或 Search `model view`）：`Models: quantized views (A/B)` unchecked 为原连续方向；checked 为 finite views。改 `Model direction count (128 / 256 or custom)` 后立即生效，Save 可保留。默认 unchecked，保留旧全局 count=32；128/256 是可直接输入的常用比较值。不要在内部 review fixture 中手调，它会为验证逐帧覆盖控件。

候选保留真实 perspective，而非历史 orthographic tile，所以不是旧 atlas 图像的 pixel-exact 重现。nearest bin 边界有预期的视觉跳变，roll 连续；未加 hysteresis（否则会违背精确 nearest rule）。极点不使用会跳变的 up-axis frame。花头绕历史中心保持中心不漂移，stem 本身没有量化，因此 socket 连接观感尤其在低 count 时需要看动态效果。高 N 的逐 vertex exhaustive selection 有 O(N) 成本，未做 perf acceptance/优化；美术效果和运动稳定性均仍需用户后续认可。本轮日志保留启动/resize 的 fruit physics hitch warnings，不声称它们已被解决或完成性能归因。
