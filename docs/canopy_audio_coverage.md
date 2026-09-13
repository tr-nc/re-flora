# 树冠音频空间覆盖修复

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
