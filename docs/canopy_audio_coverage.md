# 树冠音频空间覆盖修复

## 当前版本：可保存的每树预算与 PetalSonic 0.9.1

- Debug → Wind → **Canopy Audio Samples / Tree**：1–64，默认 16，使用声明式 GUI 保存，
  不新增单独的保存钩子。所有已有树、新种树、存档恢复的树采用同一设置。
- 改动预算时从已提交、共享保存的原始叶位置和局部树干重采样，不重新生成树，
  不重建地形或叶子渲染几何。声音使用既有 0.35 秒代际淡入淡出；发布失败回滚音频，
  成功后才更新树记录。短暂过渡期允许旧、新代际共存，不是每个点一个播放源。
- 后端从 crates.io 升级至 0.9.1；游戏调用 `weighted_samples_with_limit`。
  旧构造器仍默认 8，新接口接受调用方的任意正预算，未来业务调整不需再改音频库。
  全局射线预算仍然有效，按实际点数整源准入或延后，不隐式截断。
- 依照 codebase-design 的职责划分，树冠覆盖与配置属于游戏，
  合法性校验、声学求解与渲染属于通用音频库。

验证：

- PetalSonic：完整 `tools/publish.sh --publish` 通过，包括严格 clippy、
  181 库测试、6 集成测试、2 doc tests、release 实时门槛、Demo 构建及包验证。
  另有 7 ignored；16/64/257 点完整求解、缓存、预算拒绝回归通过。
  16 点 × 8 Voices 的 release 探针输出有限，p95 1208us；
  这不是任意点数或任意机器的性能保证。
- 0.9.1 已发布到 crates.io，并推送 tag `v0.9.1`，发布 commit `7c46a66`；
  随后的文档类型名称修正 `d186586` 也已推送。
- 游戏：fmt/check、978 主测试 + 4 库测试通过（2 ignored），包含通用设置保存、
  1/8/16/32/64 点传入真实后端、多树复用已提交几何的回归。
- 持锁 release hidden muted 默认场景：
  `target/re-flora-logs/re-flora-20260913-235017.927-155176.log`。
  16 点、1 Voice；55 次 extent response，采样契约、权重聚合、身份校验错误均为 0。
- 持锁 release hidden muted 五树受限预算诊断：
  `target/re-flora-logs/re-flora-20260913-235137.618-159132.log`。
  80 点、5 Voices；故意低预算导致整源 Deferred，采样契约/聚合错误为 0。
  两次均正常退出，failures=0、无 ERROR；保留原有蝴蝶多 atlas 警告。
- 生成文件仅 `src/app/generated/gui_adjustables_gen.rs`，由 cargo check 再生。
  config/gui.toml 只新增预算声明，用户其它值不变。shader 未改。
- 尚未人工拖动 GUI 或验收听感，也未做大规模多树性能验收；未自动启动可见游戏。

以下为初版固定 8 点的定位与验证记录。

## 原因与选择

原来的八分区选点会把同区的独立稀疏叶簇交给远处代表；没有安全候选时还会跨区替代。
确定性案例（一个孤立小簇、一个密集大簇和另一侧叶子）在旧实现失败，
新实现让小簇有就近代表，权重保持 1/102，而非人为给各簇等音量。

改用确定性最远点优先覆盖（farthest-first k-center），随后最近中心分配叶子，
按成员数量计算权重。2 体素以内不再细分，最多 8 点。选点本身 O(8N)，
输入仍先排序保证稳定；只在树冠重建时执行。一棵树一个播放源不变。
每个覆盖区域独立寻找安全叶位置，全部受木头遮挡时沿用区域内叶簇外推。

## 验证

- 回归先红后绿：sparse_cluster_sharing_an_octant_keeps_local_coverage。
- 覆盖局部木头避让、超预算权重守恒、输入顺序确定性、空树冠、真实生成树。
- cargo fmt --check、cargo check、cargo test 通过：976 主测试 + 4 库测试，2 ignored。
  音频测试有 ALSA 设备探测诊断输出，测试无失败。
- flock --nonblock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5
  成功退出，shutdown failures=0；默认场景发布 canopy_samples=8、canopy_audio_sources=1。
- 日志：target/re-flora-logs/re-flora-20260913-233157.309-139665.log。
  无 ERROR；存在多蝴蝶 atlas 选择警告。

## 边界

固定 8 点是空间近似，不保证任意多孤立叶簇各有一个代表，也不保证
避木外推后仍满足原始中心的覆盖距离。区域是空间最近邻区域，不是植物学枝系。
保留稀疏区域的位置不意味着它与密集区域同样响亮。
隐藏静音运行不是听感验收；尚未人工确认用户指出的默认树具体小簇的听感。
未改 GUI 默认值、音频后端、shader 或生成文件。
