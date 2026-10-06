# 石材与统一像素风格：集成验证

从 `main d9f5a66a` 分出的三个独立 Worker 已集成。石材的权威源仍为 ordinary indexed triangle solid；体素化与直接三角形不是两个独立生成器。

## 接线

- Direct stone vertex 使用 `stonePhysicalFrame`（canonical 尺寸已烘焙，局部底部 pivot、刚体 quaternion）、公共 `modelMeshViewFrame`、唯一的 `model_pixel_view_count` 和 immutable view bank（set 1/binding 19）。位置和法线消费同一 rendered frame；不改变物理 pose、camera 或碰撞。
- 模型视角量化现已固定启用，只保留全局 direction count；direct stone 不再保留关闭分支，没有重新引入 atlas 或局部像素采样。
- Stone lighting 复用普通 mesh 的相机/环境 uniform 声明，避免同一 binding 的重复 owner；体素路径也用这些共享声明，但不量化 voxel geometry。
- 全局 dither 位于全部 scene 合成与 tone map/resolve 后，因此直接石材与体素石材同样进入它；HUD/Debug UI 不处理。当前只保留全局 Bayer 4×4；局部 terrain ambient dither 与其它分项处理已移除。

## 首次集成验证（历史记录）

以下 continuous/128/256 mask 与开关结果保留为首次集成的历史证据。当前脚本比较始终启用的 32/128/256，style-cycle 只调整 count、全局 Bayer 与石材路径，不再切方向量化开关。

- `cargo fmt --check`、`cargo check`、`cargo build --release` 通过。
- 完整 Rust：**1365 app + 4 library passed，5 ignored**。37 Slang CPU tests 全通过；stone CPU shader 检查 quaternion/frame 位置等价、底部 pivot、刚体右手性和法线一致。
- `scripts/validate-stone-preview.sh` 在 native Release、hidden/mute 下完成双路径、同源 hash、关闭/重开、extent cycles、GUI 搜索与真实 Save 输入回放→同 binary 重启。`config/gui.toml` 最后逐字节恢复，未写世界/地形。
- 新增连续 / 128 / 256 / global / combined captures，1600×900；实时 style-cycle 交替开关、128/256、两种 pattern、路径和源模型变化。Khronos validation 与同步验证均启用，无 ERROR/panic/VUID/device-lost。
- 静态 Rock 上部 ROI `440×280+580+270` 的 neutral-bright coverage predicate 为 `r,g,b > 0.58`。连续模式与重复连续模式 mask SHA-256 相同：`fd67c5666cc12b3f581b42867436c8cd3bb32f587ff3e0d030a5b30ad3ee8013`；128：`fa60d75def1a33051763d0d9dfff2db66caa02c10bf64edda7dd9c27ecc96245`；256：`15e7e87c7fc3e8ab69224179144185f888c80678eb93c5ee5dff22f5b8383c14`。三者两两不同，证明 checkbox/count 到达实际 native stone draw，不是只改 GUI 或假颜色。
- 最初更高的 ROI 包含底部动态花朵，重复 A 的 128 个像素不同，导致验证失败；分析差异 bbox 后将 ROI 限定于石材上部，排除该污染并完整重跑通过。没有放松渲染正确性或用全图变化冒充有限视角效果。

完整原始图片/日志位于主工作区 `target/stone-native/`；可再执行脚本生成。该脚本会备份/恢复 GUI 文件，且要求 Slang/native Vulkan display/ImageMagick。

## 证据边界

这是功能/资源生命周期和可见候选的闭环，不是最终美术、长期运动、性能或其他平台验收。预览没有 collider、铺设玩法，也不永久改变庭院。有限方向有预期 bin 跳变与逐 vertex O(N) 成本，尚未做专门 Release 性能优化；运行中的 physics hitch 警告仍存在。已有纯 source-color resolve 保证不延伸为“经过 dither 后仍属于原 RGB 源样本集合”。
