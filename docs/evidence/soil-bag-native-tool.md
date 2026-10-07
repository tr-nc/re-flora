# Soil 工具：共享土袋与落土颗粒试用

## 交互与范围

- `B / Soil` 的实际地形悬停显示封口土袋；按住 LMB 立即切到已开口、袋口向下的固定姿态，不播放撕开或倾倒动画。松开恢复悬停姿态；没有地形目标、鼠标被 UI 占用或切换工具时隐藏袋子。
- 土量仍由既有立即出第一批、随后按保存频率的 Soil 调度器决定。个体沉积、形状、材料字节、肥力、有限背包、Infinite Materials 与同步 Visible Terrain Publication 未改。
- 只有成功的权威写入允许视觉落土流；空背包／拒绝放置不会制造颗粒假装成功。松开立即停止发射，已经飞出的颗粒继续落地后消失。
- 颗粒是共享 terrain-particle 渲染路径上的简单不透明方粒，90 粒/秒、最多128个、最多0.7秒寿命；重力轨迹使用逐帧扫掠地形/场景射线检测，接触即消失。无土块模拟、粒间碰撞、额外体素写入、落地结算或购买逻辑；视觉粒子数不是实际体素数量。

## 共享来源与渲染

`203078a8` 发布单一来源：`assets/models/soil-bag-source.mjs`、`soil-bag-sticker-art.mjs`，网页旧入口仅重导出。`node scripts/publish-soil-bag-model.mjs` 用同一 Three.js 法线/撕条铰链发布完整局部网格的 hover / pour 两个静态快照，以及原始512×640平滑矢量的 opaque RGB 贴纸。ImageMagick只用于作者重发布，不是游戏或 Cargo 构建的依赖。构建校验源文件与贴图指纹；不手改生成资产。

原生 `tool_model.vert.slang` 只适配姿态和材质，复用 `model_surface_cache`、`model_mesh_view` 与环境照明/硬件深度，和苹果/花头使用同一像素采样路径。新增第四种独立缓存拥有者，不另造土袋像素 renderer；初次试用固定32²，视向数读取共享保存设置。本地128视向下两种姿态均实际绘制 `cached_surface_cells`。旋转量化以真实袋口为枢轴，发射点不会与可见袋口漂移。姿态缓冲在已完成 fence 的 frame slot 中更新，其余在途 slot 保持资源所有权。

RGB 贴纸在 Linear-sRGB 解码后 Nearest 查找，不插值、不用 mipmap、不以 alpha 改写几何。网页灯光不等于游戏光照；自定义图片/颜色仍是页面内实验，不自动发布。现有 GLB/TRS 加载器没有获得动态袋口 morph 支持；静态快照是本次明确的原生适配，不伪称动画加载器支持网页顶点变形。

## 验证（2026-10-07）

- `cargo fmt --check`、`cargo check`、`cargo build --release`通过；无新增 Rust warning。
- 5项土袋/粒子/共享 shader 入口回归、24项沉积测试、13项工具测试通过；共享 model geometry/cache/render-frame tests通过。Node **50/50**通过（包括发布快照逐字节再生成和连续原始矢量守卫）。HTTP 实际服务的共享源码与文件一致。
- 中途完整 Rust 套件：**1424 passed / 5 known baseline failed / 6 ignored**，碰撞另有4项通过。仍是此前石头、玻璃、增益/采样默认值5项失败，没有修改无关默认值。随后增加 shader 入口守卫并修改视觉颗粒密度/颜色后，重新通过上述定向测试。
- 首次 GPU smoke 暴露新 shader 的 `SV_VertexID` 触发 DrawParameters capability；改为与现有原生模型一致的 `SV_VulkanVertexID`，不扩大设备特性要求。最终普通隐藏/静音启动、悬停/生产按住检查和 rooftop 全量检查均无 ERROR、VUID 或 panic。普通启动保留既有 fruit-physics hitch warning。
- 真实悬停检查：`added=0 peak_grains=0 contacts=0`，土量/背包不变。
- 真实 LMB cycle：悬停 → 按下 → 松开 → Edit，`added=7168 peak_grains=24 contacts=61`；权威区域体素数等于初始数+成功写入数，真实背包保留。数量来自正常时钟，不作为固定时长性能基准。
- 既有 rooftop 检查再次通过：生产 LMB 的1/10/50/512/1024批量、0/49/50/100/250ms时序、4224个有支撑新增体素，地板、旧材料、发布和背包保持正确。
- 已查看实际游戏截图：`target/soil-bag-native-{hover,pour}.png`；没有用网页截图替代游戏结果。尚需用户接受游戏中的大小、位置、颗粒密度和照明；未做新性能验收或其他 GPU/平台验收。
- `config/gui.toml` 与本次开始前备份完全一致：保留用户 spread13/浮点格式，不提交。没有新增 Debug 控件或保存钩子。

日志：`/tmp/terrain-magic-soil-bag-game-{check,tests,cache-tests,geometry-tests,frame-tests,all-tests,node-tests,release,hover,cycle,rooftop,smoke,tail}.log`。最终普通 startup：`target/re-flora-logs/re-flora-20261007-213511.491-2229480.log`。

## 重现 / 手动试用

```sh
cargo run --release -- --rooftop-poc
# B 选择 Soil；在空屋顶试悬停/按住/松开，3 返回 Edit。
# 不保存此试验场；普通试用不传 --perf。

# 自动检查：只用于空 rooftop，使用真实工具与输入，不保存设置。
RE_FLORA_SOIL_BAG_REVIEW=cycle \
  RUST_LOG='warn,re_flora::app::core::soil_bag_review=info,re_flora::tracer::model_mesh_frame=info,re_flora::run_log_binding=info' \
  cargo run --release -- --hidden --mute --rooftop-poc --auto-exit 5
# hover 模式验证不点击时的零写入/零颗粒。
```

GPU 检查应串行；从同一 worktree 使用 `--latest-log` / `--tail-latest-log` 找到日志并检查 `[SOIL_BAG_REVIEW][CHECK]`。截图 fixture 的相机/时间只在 opt-in review 下设置，不改变普通游戏相机。
