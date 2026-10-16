# 小地面障碍通行与防爬墙闭环

日期：2026-10-17。起点：`665467fe`。实现前的行业调研见[玩家地形通行](../research/player-terrain-traversal.md)。

> 当前状态：用户已确认采用新方案，A/B 开关已移除，保留高度滑杆。下文保留初次 A/B 交付记录，采纳后的清理与验证见文末。镜头离地跳变另见[后续修复](walking-camera-support-transition.md)。

## 影响范围

已复现的代表场景：平地上1体素高、2体素厚的横向小凸起会阻挡4/8体素玩家胶囊。孤立小石粒同属本次覆盖的行为类别。用户未提供最初遇到的具体地形，因此不声称合成夹具就是其原场景。

新增可保存的 `Debug → Walking Traversal` 控件：

- **Player: traverse small ground obstacles (A/B)**：默认开启；关闭为原始严格落脚判定。
- **Player: small ground obstacle height (voxels)**：默认3，范围0–4；关闭开关时禁用滑杆但保留数值。0等同原始严格规则。

普通宽台阶仍使用原有12体素上限；该滑杆不是整体步高或跳跃高度。超过容许量的窄脊仍会阻挡。未实现翻越高墙、自动跳跃或跨空沟。割草机、垂直落点预检及已有场景检查继续走原接口，行为保持原样。

## 根因与行为证据

原 `character_support::patch_is_supported` 的每条射线只能到达接触高度附近（允许碰撞 skin 误差）。因此当胶囊接触一个窄小凸起时，即使周围有正常地面，只要地面比凸起顶面低一个体素，也不计入可用支撑。

这同时影响显式上/前/下跨步和 KCC 的非跳跃上升；结果是“低小地面细节”和“高墙窄脊”都被要求提供顶面高度的完整采样落脚区。

修改前的红测试：

```text
cargo test --manifest-path crates/re-flora-physics/Cargo.toml \
  --test character_obstacles low_narrow_obstacles -- --nocapture

low obstacle blocked: hz=30 height=1 pebble=false
end=Vec3(9.316719, 13.1, 16.00003)
0 passed; 1 failed
```

修改后，相同场景启用3体素容许量可前进到目标位置并回到地面，保持前进方向且不穿入障碍；0容许量仍复现旧阻挡。只改变地面支撑高度带即可区分两种结果，未提高普通步高或修改接触法线。

## 何时引入

严格支撑规则来自 `783526e3`（2026-10-09），由 `8ad5cb8b`（2026-10-11）移入当前主线。该代码机制解释并复现了上述低凸起阻挡。用户原始现场尚未回放，其具体引入点仍未独立确认。

## 干净修复

物理权威仍是 `CollisionWorld::move_capsule_character_with_ground_detail`；原 `move_capsule_character` 以0容许量调用它，保留兼容入口。

支撑采样区的宽度、深度、方向搜索和表面法线要求不变。候选规则允许采样区的实际地面低于当前障碍接触点一个**受限高度**，同时保留完整胶囊扫掠、正式步高、头顶空间和原贴地/跳跃规则：

- 每次查询均用当前已发布的碰撞几何；不是放行所有矮接触，也不是沿用上一次成功的地面高度。
- 对每个采样点要求实际支持面位于接触点以下的容许高度内。窄脊越来越高时，原来的低处地面超出此高度带，必须拒绝继续上升；不能用已升高的角色位置刷新预算。
- 缺少支撑、射线起点在实体内部、法线不合格仍失败。不存在以 miss 代填地面、修改相机高度掩盖阻挡或跳过碰撞的路径。
- 物理边界拒绝负数/非有限值，且将容许量硬限于一个胶囊半径，调用方不能用过大数值开启任意爬墙。
- 两条上升路径共用同一个判定，避免只修显式跨步、KCC 仍能绕过保护。

这是局部粗糙地面容许量，不是任意空中翻越路径规划。原有3×3采样并非连续地面面积的精确证明，本次没有宣称消除所有特殊几何的采样歧义。

控件声明、范围、依赖和默认值由 `config/gui.toml` 持有；旧配置通过统一 loader 补齐，现有值不被覆盖，加载不改写磁盘。`gui_adjustables_gen.rs` 由 `cargo check` 生成；没有手写保存分支、App-only 控件或额外地形缓存。

## 验证结果

- 根目录 `cargo fmt --check`、`cargo check`、`git diff --check` 通过。
- `cargo test`：主程序 **1618 passed / 7 ignored**；碰撞 benchmark 逻辑测试 **4 passed**；显示授权测试 **1 passed / 1 ignored**，剪贴板原生测试3项按原配置忽略。无失败。日志：`target/walking-app-tests.log`。
- 独立物理 crate `cargo test --manifest-path crates/re-flora-physics/Cargo.toml`：**69 passed**。日志：`target/walking-physics-tests.log`。
- 新回归覆盖1/2/3体素窄凸起与石粒、0–4容许量边界、5/12/13/24体素障碍、宽台阶、反向/斜向输入、低顶、缺失地面、跳跃/下落/静止保持、运行时切换、地形改高后的查询更新和无效参数。
- 持续顶墙回归覆盖30/60/120 Hz、0.5/1/2倍体型、旋转、正负砖坐标、4秒推墙，以及故意传入1000体素后被半径限制的情况。全程独立检查胶囊与夹具方块不穿透，窄脊不能成为通往墙顶的楼梯。
- 真实 Camera `prepare → move → apply` 调用链在30/60/120 Hz下启用候选后无需跳跃可跨过3体素窄凸起；严格模式阻挡；最后落回地面。离开圆底边缘时可短暂进入原来的重力下落路径，未改写 grounded 语义来假装一直站地。
- 应用物理适配测试验证256倍单位转换、三角模型小障碍、0→3→0实时切换。三角面扫掠落点约0.089体素离地，属于0.1体素 skin 内的无穿透位置；断言真实不穿透/不悬浮，不要求求解器恰好落在0.100。
- GUI 新旧文件升级、开关/滑杆保存回读以及全量通用保存测试通过。
- `cargo build --release` 通过。`cargo run --release -- --hidden --mute --auto-exit 0.5` 正常首帧、正常退出；日志 `target/re-flora-logs/re-flora-20261017-005715.292-55202.log` 无 WARN/ERROR/VUID/panic。
- 原生隐藏截图运行检查了搜索结果中的同组开关与滑杆，默认勾选、3.00数值可见：`target/walking-traversal-controls.png`；日志 `target/re-flora-logs/re-flora-20261017-005735.765-55294.log` 无 WARN/ERROR/VUID/panic。
- 原生运行前后 `config/gui.toml` 和 `config/camera_snapshots.toml` SHA256 一致；无运行污染或手改生成文件。

独立物理 crate 的全量 `cargo fmt --check` 仍报告**基线已有**的 `src/lib.rs`、`src/deforming_geometry.rs` 和 `tests/terrain_rewrite_regression.rs` 格式差异；已从 `HEAD` 导出这三份原文件复查，同样失败。此次新增/修改的独立测试和角色步进/支撑模块已格式化，未为清除基线格式差异扩展无关改动。证据：`target/walking-baseline-physics-fmt.log`。

以上验证不是 Release 性能测量，也不是用户原始地形和主观手感的最终接受。候选可直接在同一运行中关闭对照；之后的具体现场反馈应继续用同一移动权威和回归路径处理，而不是叠加场景特例。

## 用户确认后的开关退役（2026-10-17）

用户认可新通行及镜头修复，要求去掉开关、固定使用新方案。已删除 `walk_ground_detail` 的配置声明、生成字段、Debug 控件及玩家运行时分支；保留 `walk_ground_detail_height` 的 ID、默认3、0–4范围及统一保存。滑杆不再依赖开关，旧配置无论保存开或关都清除旧开关及滑杆依赖，保留原高度值，加载不主动改写文件，下一次 Save 不再输出旧字段。

验证：`cargo fmt --check`、`cargo check`、106项 GUI 配置/保存测试、16项相机测试、10项 surface-motion 测试通过；本次未重跑全量套件。`cargo build --release` 和隐藏静音0.5秒启动通过，日志 `target/re-flora-logs/re-flora-20261017-012656.561-69720.log` 无 WARN/ERROR/VUID/panic。原生截图 `target/walking-traversal-adopted.png` 已确认 Walking Traversal 只显示一个3.00高度滑杆，无开关；对应日志 `target/re-flora-logs/re-flora-20261017-012659.637-69783.log` 正常退出。运行前后 GUI/相机配置 SHA256 一致。生成文件由 `cargo check` 更新，未手改；未启动可见游戏，也未宣称通过新的性能验收。
