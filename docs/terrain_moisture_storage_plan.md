# Terrain Moisture Storage Plan

## 背景

当前喷水器和调试 Water brush 的潮湿效果使用 CPU 维护的 moisture patch 列表，再把 patch 作为 uniform 上传给 tracer shader 做视觉变暗。这条路适合快速验证视觉效果，但不适合作为真正的土壤状态：

- patch 数量有固定容量，Water brush 画很多地方后，旧 patch 会被回收，旧湿区会突然变干。
- patch 是视觉层数据，不是 voxel world state，后续植物生长、土壤规则、保存/加载都无法可靠复用。
- 喷水器湿度本质上可以由喷水器位置驱动，但“土壤已经湿了多久/湿到什么程度”应该成为 terrain/voxel 状态。

因此 moisture 需要进入 voxel 数据路径，而不是继续作为独立临时 patch。

## 当前相关数据结构

### `chunk_atlas`

- 类型：dense 3D atlas，目前是 `R8_UINT`。
- 范围：完整 voxel 体积。
- 当前含义：每个 voxel 存一个 material / voxel type，例如 empty、dirt、sand、wood、rock。
- 作用：原始 terrain/material source。terrain build、surface extraction、solid sampling、water collider sampling 等流程都从这里判断 voxel 是否存在、属于什么类型。

### surface 中间贴图

- 类型：per-chunk `R32_UINT` surface texture。
- 范围：只保留 surface voxel 数据。
- 当前含义：打包后的 surface data：voxel type、normal、normal_valid、hash。
- 作用：给 contree leaf 构建和 flora surface 查询使用。

### contree / 64-tree 加速结构

- `contree_node_data`：节点 child mask、child pointer、leaf flag。
- `contree_leaf_data`：surface leaf 的打包数据，目前包含 voxel type、normal、normal_valid、hash。
- `scene_tex`：每个 chunk 存 contree node/leaf offset，用来从 chunk grid 找到对应 contree 数据。

contree 是派生的 ray traversal / surface hit 加速结构，不是完整 voxel source of truth。

## 设计约束

1. moisture 必须成为 terrain/voxel 状态，不能继续依赖有限 patch 列表。
2. contree hot path 不应变胖。增加 node/leaf 大小会增加 ray traversal/命中后的带宽压力。
3. 不应为了动态 moisture 高频重建 contree。
4. 短期要控制显存，不引入和 `chunk_atlas` 同尺寸的 dense parallel atlas。
5. 后续要保留扩展空间：可能继续增加 fertility、tilled、nutrients、temperature 等土壤状态。

## 方案比较

### 方案 A：moisture 存进 contree leaf

不采用作为主方案。

优点：

- tracer 命中后可直接从 leaf data 读取。

缺点：

- contree leaf 结构变大，影响渲染 hot path。
- contree 只覆盖 surface，不是完整 voxel 状态。
- moisture 动态变化会要求更新或重建派生加速结构。
- 扩展更多土壤状态时问题会更严重。

### 方案 B：新增 dense parallel voxel state atlas

短期不采用。

优点：

- 结构清晰，`chunk_atlas` 保持 material，state atlas 存 gameplay state。
- 扩展性好，tracer 命中后按同一 voxel 坐标查询即可。

缺点：

- 显存成本高。`256^3` voxels 下：
  - `R8_UINT` 约 16 MiB / chunk
  - `R16_UINT` 约 32 MiB / chunk
  - `R32_UINT` 约 64 MiB / chunk
- dense state atlas 会让每个 chunk 固定增加大量显存，即使大部分 voxel 没有状态。

### 方案 C：短期把 moisture pack 到 `chunk_atlas` 高位状态位

短期采用。

当前布局：

```text
chunk_atlas R8_UINT
bits 0..3 = voxel type
bits 4..5 = moisture, 0..3（0=dry，1..3=更湿）
bits 6..7 = reserved soil state bits（后续 fertility/tilled 等）
```

当前 voxel type 值较少，可以暂时放在低 4 bit。moisture 只用 2 bit 量化为 4 档；视觉反馈和基础植物规则短期只需要少量离散湿度档位，剩余高位保留给后续土壤状态。

优点：

- 0 额外 dense atlas 显存。
- moisture 真正存在 voxel data/source of truth 中。
- 索引简单，仍然按原 atlas voxel coordinate 读取。
- 旧湿区不会因为 uniform patch 容量被回收。

缺点：

- voxel type 临时限制在 4 bit，最多 16 类。
- moisture 只有 4 档。
- 所有读取 `chunk_atlas` voxel type 的 shader/Rust 路径都必须 mask 低 4 bit。
- 所有写 `chunk_atlas` 的路径都必须明确是保留 moisture 还是清除 moisture。

### 方案 D：长期 sparse/bricked voxel state overlay

长期目标，不在短期实现。

思路：

```text
chunk_atlas         = dense material atlas
voxel_state_overlay = sparse/bricked state pages, default state implicit zero
```

例如按 `8x8x8` 或 `16x16x16` brick 做 page table。没有分配 page 的区域代表 moisture/fertility 等状态全为默认值。

优点：

- 显存随实际有状态区域增长，而不是随完整体积增长。
- `chunk_atlas` 继续专注 material/solid。
- 扩展 fertility、tilled、nutrients 等状态更自然。
- contree 仍然不变胖。

缺点：

- 实现复杂，需要 page allocation、page table 查询和写入同步。
- tracer hit 后比直接读 dense atlas 多一次索引。

## 当前决策

短期采用方案 C：**把 moisture pack 到 `chunk_atlas` bits 4..5，并保留 bits 6..7 给后续土壤状态**。

长期保留方案 D：**sparse/bricked voxel state overlay**，当 soil state 超过当前 packed bits 或材料类型超过低 4 bit 容量时，再升级。

contree 保持为派生加速结构，不作为 moisture source of truth。

## 短期实施计划

> 本文档只记录计划，不包含代码改动。

1. 定义统一的 atlas byte packing helper。
   - low nibble：voxel type。
   - bits 4..5：moisture，0..3。
   - bits 6..7：reserved state bits。
   - 所有 shader 中读取 type 时必须使用 helper，而不是直接 `imageLoad(...).r` 当 type。

2. 修改 `chunk_atlas` 写入路径。
   - terrain init：写入 voxel type，moisture 初始为 0。
   - terrain edit 添加/替换 material：明确 moisture 策略。
     - 写 empty：清除 moisture。
     - 写 non-soil material：倾向清除 moisture。
     - 写 dirt/sand：可以保留局部 moisture，或按设计重置；需要在实现前确认。
   - smoothing 或 surface rebuild 不应无意清空 moisture。

3. 修改 `chunk_atlas` 读取路径。
   - surface extraction、solid sampling、water collider sampling、flora surface query 等判断 solid/type 时只看 low nibble。
   - `is_solid` 基于 masked voxel type。

4. GPU 写 moisture。
   - Water brush：用 compute brush 写 `chunk_atlas` moisture bits，提高 moisture。
   - Sprinkler：基于喷水器位置周期性写附近 soil voxel 的 moisture bits。
   - 写入时保持 low nibble 的 voxel type 以及 reserved state bits 不变。

5. tracer 读取 moisture。
   - contree ray hit 后已有 `center_pos`，可以换算 atlas voxel coordinate。
   - 读取 `chunk_atlas` bits 4..5 得到 moisture level。
   - 仅对 dirt/sand 等可湿润 voxel 做额外读取/着色，避免所有材质都增加成本。
   - 不需要把 atlas index 编码进 contree leaf，因为 hit 后天然可从 world/voxel coordinate 索引回 `chunk_atlas`。

6. 移除或降级 current patch 系统。
   - uniform moisture patches 可作为过渡 debug overlay，但不再作为真实 soil state。
   - 最终 Water brush 和 Sprinkler 都应写入 `chunk_atlas` moisture bits。

## 风险与注意事项

- 必须集中定义 packing/unpacking，避免某些 shader 忘记 mask low nibble。
- `VOXEL_TYPE_MASK` 当前若是 `0xFF`，短期实现时需要调整相关 helper；不要散落手写 `& 0x0F`。
- 所有 material writer 都必须考虑高位状态位，否则容易误清 moisture/reserved state 或把 state 当 voxel type。
- 如果未来 voxel type 超过 15 类，此方案必须迁移。
- 2 bit moisture 是短期量化方案，视觉和 gameplay 阈值需要围绕 0..3 调整。

## 长期迁移触发条件

考虑从 packed atlas state bits 迁移到 sparse/bricked state overlay 的信号：

- voxel type 数量接近或超过 16。
- soil state 超过 moisture 单字段，例如 fertility、tilled、nutrients、temperature 都需要持久化。
- 需要高于 4 档的 moisture 精度或更复杂的 per-voxel gameplay state。
- dense atlas bit packing 开始让 shader/Rust 写入路径难以维护。
