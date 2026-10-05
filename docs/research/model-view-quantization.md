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

计划接入 flower heads（每个 head 的历史中心 pivot）、attached/dynamic apples、mesh butterflies 与 modeled detached leaves。纯 world/voxel terrain、tree cells/voxel leaves、普通 square particles、stems、UI 和玩家 camera 不接入。stone Worker 的 canonical triangle vertex adapter 可复用 `model_mesh_view.slang::modelMeshViewFrame(physicalFrame, localPivot)`；公共 helper 不包含 flower/material 专属决策。最终接线由控制器完成。

验证/证据及最终调用细节在实现验证后补充。艺术效果、花头 socket 连接观感与运动稳定性仍需要用户后续认可；本任务不以 AA 或性能指标作为成功条件。
