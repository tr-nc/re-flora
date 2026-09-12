# 果实落地持续抖动诊断与修复

基线 `95851a6f5f0968299f671049797ecb6deb057812`，独立分支 `agent/fruit-ground-jitter`。
本任务 session `01a0948d-443a-7e51-8050-1b4e6e070c51` 的 `turn_context` 实际记录
`model=gpt-6-astra`、`effort=xhigh`；进程启动参数与 worktree 路径也吻合。

## 可重复的失败信号

CPU 命令（诊断阶段标记 ignored；修复后会纳入正常回归）：

```sh
cargo test -p re-flora-physics --test fruit_ground -- --ignored --nocapture
```

相同 4-voxel 直径苹果凸包、256 voxel/world-unit、重力 `-2508.8 voxel/s²`、
120 Hz、原摩擦／恢复系数／阻尼／0.15 voxel 接触皮肤，落到单块静态平坦 Voxels 地面。
模拟 30 秒仅需约 0.04 秒；最后 10 秒竖直范围 `0.12323189 voxel`，休眠 `0/1200`，断言失败。
16 块地面缩减到 1 块仍复现；没有地形编辑、外力、渲染或果实生命周期调用。

真实游戏入口（只在显式设置环境变量时运行诊断，复用正常 registry/drop/render）：

```sh
mkdir -p target/fruit-ground-evidence
flock --close /tmp/re-flora-summer-gpu.lock \
  env RE_FLORA_FRUIT_GROUND_TRACE=target/fruit-ground-evidence/baseline.jsonl \
  cargo run --release -- --hidden --mute --auto-exit 45
python3 scripts/check_fruit_ground_trace.py target/fruit-ground-evidence/baseline.jsonl
```

第 60 帧重新挂果，第 90 帧请求掉落，不修改保存的 GUI 值。前 8 个动态果实逐渲染帧记录：
位置、四元数、线／角速度、自然休眠／计时、接触数／距离／法线／冲量、用户力／矩、
实际送往渲染实例的姿态、帧 dt／固定 dt／步数／丢弃时间／余量，以及待更新碰撞砖数量。
正常游戏不启用该入口；CPU 诊断另外逐固定步采样，避免只看单个帧末速度。

第一次 45 秒 release 基线：`target/re-flora-logs/re-flora-20260912-154717.558-460871.log`。
最终 8 个果实最后 10 秒全部未休眠，Y 范围 `0.1183–0.1653 voxel`。
成功退出，`[SHUTDOWN] phase=complete failures=0`，无 error/panic/VUID。
GUI SHA-256 前后均为 `2aeedce8e2d2f3036920313b7aec11589b7e538214d4e3986d1c56e8e3255ae3`。
这证明真实掉落后持续运动，不代替用户视觉验收或性能基准。

## 根因与反证

锁定 Rapier 0.34.0 / Parry 0.29.0。在 CPU 第 2400 步（20 秒）后重复出现：

| 步 | Y | 竖直速度 | 接触流形 | 休眠计时 |
|---|---:|---:|---:|---:|
| 2400 | 4.0088134 | 0.0000157 | 1 | 0.025 |
| 2401 | 3.8999248 | -20.896204 | 0 | 0.0333 |
| 2402 | 3.9362338 | 1.2531691 | 1 | 0 |
| 2406 | 4.0126495 | 0.0000147 | 1 | 0.025 |
| 2407 | 3.9037604 | -20.896206 | 0 | 0.0333 |

Rapier 给接触生成器传入 prediction + 两个 collider 的 contact skin。
但 Parry 的 `contact_manifolds_voxels_shape` 在枚举体素候选时只用形状的原始 AABB，
没有按 prediction 扩展；形状刚离开实际地面就丢失全部预测接触。求解器推动果实维持皮肤
间距，候选枚举又丢失支撑，下一步重力拉回，形成永久循环并持续打断自然休眠。

- 单个静态砖仍失败：排除砖接缝／碰撞体重建是必要条件。
- 没有渲染仍失败：不是物理到渲染的姿态转换导致。
- 用户力、矩始终为零，没有主动唤醒：排除外部扰动与频繁生命周期唤醒。
- 仅关闭 CCD 仍失败：不是 CCD 的充分原因。
- 仅将 skin 设为 0 后自然休眠，但地面穿入约 `0.0365 voxel`：仅作诊断，不采用这个改参数方案。
- 现有 5 秒测试只验水平速度和角速度，在抖动周期的大部分帧末仍可通过，遗漏 Y 与连续状态。

上游已有完全对应的修复：[Parry d299cdca](https://github.com/dimforge/parry/commit/d299cdca86767680285bb838f37c699b59cfa17f)。
其变更是在该候选 AABB 上调用 `.loosened(prediction)`，并新增间隔 0.05 的预测接触测试。
这是支持现有皮肤契约的接触生成修复，不改变果实形状、求解参数或运动状态。
直接升级到含该修复的上游版本还会包含 0.30 系列 BVH、GJK、形状扫掠等 83 个文件的变更，
本任务采用有明确版本来源的窄范围回移。

相关文档：[体素碰撞架构](voxel_collision_architecture.md)、
[行走平滑及方向回归](character_walk_smoothing_validation.md)、[花园存档生命周期](garden_snapshots.md)。

诊断步骤复核：`cargo fmt --check`、`cargo check`、物理 crate 的 15 个单元测试／21 个角色测试／1 个地形更新测试通过；新复现显式 ignored 并手动运行判红。第二次 45 秒运行日志 `re-flora-20260912-155122.608-463386.log` 同样正常退出，记录实际渲染实例字段，检查器判红；结果见 [基线摘要](evidence/fruit-ground-jitter/baseline-summary.json)。
