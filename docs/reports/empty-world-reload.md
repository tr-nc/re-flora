# 全空世界运行时重载修复（scope v3 / empty_load）

## 影响范围

已确认：从全零 atlas、零植物和零树的存档启动，添加一棵树，再通过正常 runtime Load
替换回原全空存档，地形/植物恢复完成后一直等待水地形发布，世界更新被冻结。
旧 smoke 在 20 秒后明确失败。普通花园、挖掘种植、挖掘割草的同一路径不受此案例影响。
全空 startup/resave 原本可退出，但这不能证明水地形已 Ready。

所有输入均为 `target/garden-delivery/world/implementation/` 内既有一次性审计副本，
没有读取或覆盖用户存档。设置及相机库无变更。

## 根因与不变量

权威 owner：`src/app/core/water/terrain.rs` 的 `WaterTerrainRuntime`。
旧 `water_terrain_has_startup_collider` 除了要求队列排空，还要求域内**存在非空 collider**。
全空域的八个采样均为 `solid_samples 0/32768`，正确地不产生 collider，却因此永远不能
`initialized=true`；`WaterTerrainStatus::is_ready` 始终为假，水 coordinator 无法发出
`WaterPublicationResumed`，持久化模块继续冻结世界更新。这不是 GPU 上传失败或 timeout 太短。

最小化回归：真实观察队列 → 保留 active source 作为依赖 → 交付不可变全空采样 → 完成 source
和 collider 队列。旧代码在 `observed empty domain must be Ready` 断言失败（0.00 秒）。
它不需要树、GPU、窗口或音频；树是原始真实场景的触发编辑，未从最终 native 验收删除。

诊断排序为：① 初始化误要求实体；② 源 revision 去重漏推进；③ 最后 collider 删除漏缓存完成。
确定性 red 证明①；②是同一完成推进位置的关联风险，一并覆盖；③现有移除/缓存序列在回归中通过，
没有为了修复而改写它。

正确不变量是：**完整域已安排观察，且所有 source/collider/cache 依赖完成，空结果也能 Ready**。
未观察不能冒充空域；pending 或 stale 工作不能发布新一代的完成。

## 已验证基线与引入时间

- 集成起点 `1366a91c` 的现有 Release 原始 smoke 实际复现：`red.log`。
- 保留的优化前 Release `performance/baseline` 也实际复现完全相同的 20 秒超时：
  `baseline-red.log`。其性能报告记录构建来源为 `a7b7243c`（2026-10-21），早于本轮花园实现。
  `binaries.sha256` 保留该 ELF 与修复后 ELF 的 SHA256；这不是重新编译旧版本的推测。
- 旧 ELF 首次直接运行因当前新增 GUI 参数不匹配而提前退出，不能算复现。
  随后使用证据目录内隔离 root（旧提交 `config/gui.toml`、复制的其他 config、只读资源链接），
  原输入、原 smoke、原参数运行，确认失败。未改主工作树配置或构建缓存。
- **准确引入提交与日期未知**；这里只确认至少上述保留基线受影响，不做完整历史考古。
  stage 2 原报告中的失败记录保留，不倒写成当时已通过。

## 干净修复及验证

新增 owner 内部 `full_terrain_observed` 事实，只有完整域安排观察后才允许初始化。
完成条件保留全部 pending/inflight 检查，不再要求实体存在。完成推进集中在每次正常 running
terrain advance 尾部，因而空结果、去重、无输出 advance 都有进展。未延长任何超时、未伪造实体、
未绕过水 Ready、未更改 snapshot 格式、移除语义、revision 或工作线程命令顺序。

smoke 保留两次“添加树 → 原文件 Load → 精确 GPU bytes/植物身份生长/树身份比较”，并加强断言：
真实水 phase 为 Running、水地形为 Ready、持久化允许世界更新。不是只看进程退出。

证据根目录：`target/garden-delivery/empty_load/`。

| 检查 | 实际结果 / 证据 |
| --- | --- |
| 修复前最小真实队列回归 | 失败，`unit-red.log` |
| 修复后水模块测试 | 41 passed，`water-tests.log` |
| 全量 `cargo test` | 应用 1666 passed / 8 ignored，库 4 passed，集成 1 passed / 4 ignored，`all-tests.log` |
| `cargo fmt --check` / `cargo check` | 通过，`fmt.log` / `check.log` |
| 默认验证层 `cargo build --release` | 通过，`build.log`；交接前再次构建见 `final-build.log` |
| 原全空未缩减 smoke，两次精确 runtime replacement | 通过，`green-minimal-empty.log` |
| 普通保存花园，两次 replacement | 通过，`green-garden-final.log` |
| 挖掘种植 / 挖掘割草，两次 replacement | 通过，`green-dug-planted.log` / `green-dug-mown.log` |
| 全空单独 startup/resave | 0 flora、0 authored、0 trees，水初始化完成，未建立花园，`empty-startup.log` |

确定性测试使用真实 queue revision、collider/cache 构建和 publication 接口，只将工作线程 transport
改为测试内部同步通道；无 GPU/window/app 纳入普通单测。覆盖未观察不能 Ready、source active 阻塞、
空/相同 revision no-op/新空 revision、重复 empty↔全 solid、最后实体移除、缓存 pending，以及旧非空
worker 晚到不能覆盖更新空观察。没有更改任何本地 crate，故无额外 changed-crate 测试义务。

全部 native 运行均为 Release `--hidden --mute`、自动选择 present mode；有 DISPLAY 时仅在子进程
移除 Wayland 环境变量。所有编译/测试/app 组均使用共同 `flock -o`，未并发占用重资源。
逐份检查新 native 日志：正常退出、`phase=complete failures=0`，无 ERROR/Vulkan validation 错误。
全空 smoke 的树印章仍有已知的 atlas 边界裁剪 WARN（修复前同样存在）；这是原树→空操作的一部分，
未移动树或改输入来隐藏它。其余验收日志无 WARN/ERROR。不是声称全部日志零警告。

生成文件无变化；无 shared-hook 请求；不涉及听感/艺术审批或性能收益声明。
集成后的汇总验收仍由 controller 执行。

### 可重跑的原始真实循环

```bash
flock -o /tmp/re-flora-garden-delivery-heavy.lock bash -ec '
  if [ -n "$DISPLAY" ]; then unset WAYLAND_DISPLAY WAYLAND_SOCKET; fi
  RUST_LOG=info RE_FLORA_GARDEN_SNAPSHOT_SMOKE=verify target/release/re-flora \
    --hidden --mute \
    --terrain-load target/garden-delivery/world/implementation/audit/minimal-empty.rflterrain \
    --auto-exit 0.5 > target/garden-delivery/empty_load/recheck.log 2>&1
  grep -F "passed startup_and_repeated_runtime_load=true" target/garden-delivery/empty_load/recheck.log
'
```
