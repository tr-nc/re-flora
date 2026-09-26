# 树编辑蒙皮绑定优化：实现与 Release 验证

这是 `tree_edit_stall_diagnosis.md` 的后续实现报告，不覆盖其原始诊断数据。
用户在诊断交接后授权实施优化；仍只操作 `agent/tree-edit-stall`，不合并、不推送、不操作其他 Worker。

## 已实现

### 1. 跨地形编辑复用静止形状绑定（`c0d7f64c`）

`RasterTreeMesh` 为当前使用的树角点保留绑定。
复用条件为相同 tree id、相同不可变 `Arc<Tree>` 身份、相同 placement origin。
树替换、年龄重建或位移都不能复用旧绑定；删除/编辑后仅保留新 mesh 使用的角点，
不会累积所有历史上出现过的角点。

- 地形 occupancy、外露面、法线仍完整重建，不以“没有挖木”跳过表面更新。
- 每个体素的归属只判断一次，不对其 8 个顶点重复判断。
- 多树重叠仍按调用顺序对当前中心重新判断归属；旧缓存不能强行保留旧 owner。
- 缓存属于 mesh，跟随已发布 mesh 的生命周期；不引入全局可变缓存或额外 revision authority。

### 2. 精确圆锥距离查询使用 BVH（`a96fe488`）

复用项目已有 `RoundConeClearanceIndex`：

- `append_region` 和体素归属使用索引的 exact containment/clearance 查询。
- 新增 `nearest_cone`，为冷绑定或新角点找 signed distance 最小的圆锥。
- `RestSkinBinder` 绑定一棵不可变树和它的索引，每个 mesh 编译阶段只建一次，避免每个角点重建索引。
- 最终距离仍由原 `RoundCone::signed_distance` 决定；距离平局仍取原数组较小下标。
  对 AABB 内部不把无符号距离当作 signed lower bound；包含点的重叠节点仍要访问。
  外部剪枝使用保守浮点 allowance，不用最近圆锥中心或近似最近邻替代原语义。
- 原 `SkinBinding::at_rest_position` 全扫描路径保留，供少量独立查询和正确性对照；权重计算共用。

没有改默认配置、GUI、shader、生成文件或 publication 的同步一致性约束。
没有添加优化 A/B 开关：这是确定性、语义等价的修复，不是视觉实验。

## 实测：同一真实 Release app 对照

沿用诊断报告的机器、默认场景/seed、GUI SHA、brush 和 near/far 点。
5 秒暖机，7 秒退出，每次新进程，一笔 radius=0.055 移除；交错三对。

```bash
cargo build --release
python3 scripts/diagnose_tree_edit_stall.py target/tree-binding-index-2 --repeats 3
```

| 指标 | 优化前 | 仅缓存与体素归属去重 | 缓存 + BVH |
|---|---:|---:|---:|
| 树旁编辑帧 ms | 865.13 / 888.35 / 890.85 | 186.19（一次阶段测量） | 75.62 / 75.63 / 76.40 |
| 远处编辑帧 ms | 21.11 / 21.35 / 22.00 | 20.92 | 21.86 / 22.60 / 21.81 |
| 树旁热绑定 `mesh.bind_tree` ms | 717–738 | 34.28 | 6.52–6.79 |
| 初次无缓存绑定 ms | 原诊断初次约 700–740 | 542.49 | 11.78–12.45 |
| 树旁 `append_region` ms | 89–92 | 91.98 | 8.49–8.55 |
| 树 mesh upload ms | 约 32 | 32.96 | 32.36–33.56 |

- 原 near 卡顿帧中位数 **888.35 → 75.63 ms**，下降约 **91.5%**，约 **11.7 倍**。
- 热绑定约 **百倍**降低；冷绑定也明显改善，不是仅把成本转移到新角点。
- 正常帧编辑前 120 帧中位数仍约 **17.14–17.19 ms**。
- 最终三对脚本输出 `stall_captured=false`、退出 0；near 和 far 的后续窗口 max 见 report。
  这只是原 >=100 ms near-only 症状不再触发，**不代表达到 16.7 ms 无停顿目标**。
- near 移除仍为 583 个 dirt、0 wood，far 616 个 dirt、0 wood；最终 mesh fingerprint 仍为
  `ae11f950c7b738e1`，surface cells=5576、triangles=29636。
- 仍有表面重建和 GPU/secondary tree scene 重建开销；约 32–34 ms upload 是后续明确候选热点。
  本轮没有借机改可见 publication、异步资源生命周期或 DDGI。

最终性能证据：`target/tree-binding-index-2/report.json` 和 near/far 六份日志。
其 HEAD 为 `c0d7f64c`，同目录 `source.diff` 保存当时 BVH 变更，后提交为 `a96fe488`。
二进制 SHA256：`c713c4bc1df668da4c0a086387a368ede17555a9c3dd1856ea1bc44dc84c9d0b`。
仅缓存阶段证据：`target/tree-binding-cache-1/`。
原始对照：`target/tree-stall-final/`。

### 真正改木的 Release replay

```bash
env -u WAYLAND_DISPLAY RE_FLORA_TREE_EDIT_DIAGNOSTIC=trunk \
  target/release/re-flora --hidden --mute --perf --water-edit-soak --auto-exit 14
```

串行重复 3 次，均完整成功退出，无 ERROR/panic/VUID。
三笔编辑帧分别为：

| 轮次 | 第一笔 ms | 第二笔 ms | 第三笔 ms |
|---|---:|---:|---:|
| 1 | 81.23 | 79.37 | 67.71 |
| 2 | 79.89 | 80.04 | 66.52 |
| 3 | 82.05 | 79.32 | 68.32 |

第一/二笔命中并移除木体素，mesh cells 从 5576 到 5373、5108；第三笔从上命中地面，不称为第三次挖木。
原诊断中相同序列每笔树 compile 本身为 750–829 ms。
日志：`target/tree-binding-trunk-2-{1,2,3}.log`。

## 正确性验证

### 持久单元测试

- 编辑暴露/移除体素后，缓存绑定逐顶点与独立旧算法一致，同时 faces/normals 由新表面产生。
- 相同 tree id 的替换、位移必须拒绝旧缓存；用 sentinel 检测错误复用。
- 重叠树调用顺序变化不能被缓存覆盖。
- 新旧 signed-distance winner 对照，包括负距离、重叠体积、重复圆锥、退化圆锥、细圆锥和同距下标平局。
- seed 1/122/923 的真实生成树，中心/端点/中点及偏移采样的绑定、归属与全扫描一致。

`cargo test` 完整通过：主程序 **1168 passed、4 ignored**，另一测试目标 **4 passed**。
耗时 181.77 秒，主要是既有 climbing plant 长测试；不是性能证据。
第一次 200 秒工具时限内没完成的运行已被中止，随后以更长时限重新跑完整套，不能把首次当通过。
最后又补充了离散等距圆锥的 tie-break 测试断言，并重跑 geom 索引测试通过（3 项）。

### 标准树 smoke 的既有阻塞

```bash
env -u WAYLAND_DISPLAY cargo run --release -- \
  --hidden --mute --raster-tree-smoke --auto-exit 30
```

未修改版本 **ec1e0498** 和本次缓存/BVH 版本均在局部点光源 fixture 加入后报相同错误：

```text
publish ready DDGI staging Volume
DDGI staging publication lost its owner radiance tuple
```

对照日志：`target/tree-binding-raster-smoke-baseline.log`、`target/tree-binding-raster-smoke-1.log`、
`target/tree-binding-raster-smoke-2.log`。未修改 DDGI 修复这个无关问题，**完整标准 smoke 仍受阻**。

### 临时隔离验证（非性能证据，已清理）

仅为验证，临时加 `RE_FLORA_BINDING_VALIDATION=1`：

1. 省略 smoke 的新增点光源 fixture，其他风、GPU readback、姿态命中编辑、年龄、删除、替换和 A/B 循环保持。
2. 每次树绑定后，把每个已绑定顶点与原全扫描算法逐项比对（昂贵，不用于性能测量）。

实际执行：

```bash
env -u WAYLAND_DISPLAY RE_FLORA_BINDING_VALIDATION=1 target/release/re-flora \
  --hidden --mute --raster-tree-smoke --auto-exit 40

env -u WAYLAND_DISPLAY RE_FLORA_BINDING_VALIDATION=1 RE_FLORA_TREE_EDIT_DIAGNOSTIC=trunk \
  target/release/re-flora --hidden --mute --perf --water-edit-soak --auto-exit 14
```

两者成功退出；记录中 44608、44592、6872、42984、40864 顶点的各代 mesh 均与旧全扫描绑定一致。
隔离 smoke 完成 160 帧，包括真 posed hit 编辑、年龄、删除、替换和静止/风动画切换。
GPU/CPU position 最大误差约 **4.18e-7 world units**、normal 最大误差约 **2.80e-7**。
这支持本轮树绑定正确性，**不算完整点光源/DDGI 验收通过**。

临时改动补丁：`target/tree-binding-validation-only.patch`；日志：
`target/tree-binding-isolated-smoke.log`、`target/tree-binding-fullscan-trunk.log`。
源码已恢复，仓库 `src/` 中没有该临时变量/日志；随后重新 check、build Release 和运行普通 smoke。
这些带验证代码的计时不参与任何上述性能结论。

## 最终检查与限制

- `cargo fmt --check`、`cargo check`、Release build、全量测试、`git diff --check` 通过。
- 最终原生路径 `cargo run --release -- --hidden --mute --auto-exit 0.5` 成功；
  `target/tree-binding-final-smoke.log` 无 ERROR/panic/VUID。
  `cargo run --release -- --latest-log` 输出在 `target/tree-binding-final-latest-log.txt`。
- GUI/camera 均按字节恢复；没有生成文件变化。未自动启动可见游戏。
- 只做顺序测量并使用 GPU 文件锁；未关闭用户 GNOME/Steam 等进程，桌面负载噪声限制与原报告相同。
- 仍未测用户原存档、密集多树的 Release 规模曲线或持续平滑；重叠归属有单元测试，完整多树视觉/性能待补。
- 剩余 75 ms 笔触仍可能被感知；不把原红灯消失包装为所有编辑卡顿彻底解决。
  下一步可独立测量相同表面避免重复资源/scene publication 的方案，但必须完整比较占用、法线、绑定及生命周期，
  不能只比较三角形数量或 hash，也不能跳过真正的树旁法线改变。
