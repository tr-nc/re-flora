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

## 合入主分支 `b2b7d55a` 后的验证

合并保留主分支的 Grow 统一种植目录、异步 GPU 拾取/点击所有权和完整管线注册，同时保留 Soil、无限材料、土袋/颗粒与 Debug 根栏目字母排序。Soil 编辑只使用当前已解析的捕获点击（包括 miss，不替换成别的 hover ray）；不继承 Edit 的80ms间隔，批量/频率仍由保存的 Soil 时钟管理。没有把同步拾取加回普通交互；startup-only rooftop 检查像主分支的其他检查一样显式取得诊断 GPU 答案后运行已解析生产编辑体。

普通悬停/点击现在继承主分支的异步回答延迟（通常1–2帧），并在 terrain publication 后等待新的有效 hover，土袋/发射可短暂隐藏/暂停。缺少回答不制造猜测目标或颗粒、不清除上次成功剂量的视觉许可；已解析 miss/拒绝、松开、失去指针权限和换工具仍停止发射。没有宣称零输入延迟或完成合并后的手感/性能验收。

`cargo fmt --check` / `cargo check` / Release 构建通过；生成文件由正常检查再生成。Node **50/50**；完整 Rust **1460 passed / 4 known baseline failed / 6 ignored**，碰撞另4项通过。剩余4项仍为原有石头/玻璃/保存采样默认值断言；旧 gain-lifecycle 用例随主分支生命周期退役自然更新，不修改无关默认值。新增回归涵盖 Soil 独立时钟、捕获 miss 的所有权和等待 hover 时的颗粒暂停。

隐藏静音普通启动、完整 rooftop 数量/时序检查和真实异步 held-input cycle 通过，日志无 ERROR/VUID/panic。合并后 cycle：`added=6656 peak_grains=9 contacts=16 conserved=true real_inventory_preserved=true`；不同于合并前的同步输入数量/粒子峰值，不能混作匹配性能或相同手感证据。日志在 `/tmp/terrain-magic-main-merge-{check,all-tests,node-tests,release,rooftop,bag-cycle,smoke,tail}.log`。本地 spread13/浮点格式保留、不提交；主分支对 GUI 声明的合法变更保留。

## 世界模型实例与投影（合并后）

土袋从 `Preview` 改为有世界位置/旋转/尺度的 `soil_bag::Instance`。工具拥有其生命周期，渲染器在 acquired-slot fence 完成后**只发布一次** `world_model_instances`，主颜色/深度和太阳投影读取同一份实例快照，不是编辑色覆盖或屏幕 overlay；即使关闭 flora/particles，袋子本身仍足以启动普通场景绘制。

可见表面仍复用既有32²/shared-view缓存、材质和 `meshShade` 的实时太阳/场景阴影、DDGI环境受光。新增的 depth-only 世界 pass 用同一完整发布几何、同一袋口枢轴和相机量化世界变换投进现有太阳深度图，再走普通 depth-copy/terrain/VSM 管线；不是地面圆斑或光向 billboard。创建、姿态/位置、相机、view-bank变化和移除都参与阴影历史失效，避免旧投影残留。

共享 `model_mesh_geometry.slang` 把不受光的三角形访问从 shading 模块分离；`world_model_instance.slang` 共享放置。中间检查先发现 shadow shader 意外要求 DDGI 资源，随后发现旧 cached descriptor helper 为纯几何 pass 提供了多余资源，均通过这两个模块边界解决；未给深度 pass 塞入光照字段或扩大设备特性。新 pipeline 已注册完整 graphics/extent/transient-frame 生命周期。太阳 depth texture 的 TRANSFER_SRC 仅允许 opt-in 检查真实数据；普通交互不下载它。

此物体是 **Raster Consumer + direct-sun shadow caster**，没有添加土袋为 DDGI Occluder、局部灯二级光线遮挡或光线反射几何，也没有新增刚体、掉落/拾取、库存袋子或存档。固定 hover/pour 两态、成功写入门控颗粒、土量/支撑/库存/立即批次时钟均保持；网页素材未改，生成文件无变化。

最终验证：

- fmt/check/Release 构建通过；完整 Rust **1462 passed / 4 known baseline failed / 6 ignored**，碰撞另4项通过，Node **50/50**。2项新增回归覆盖世界实例阴影历史及独立场景绘制，既有袋口、共享采样/Vulkan入口、颗粒、沉积和工具回归通过。未修改4项既有石头/玻璃/保存默认值断言。
- `RE_FLORA_SOIL_BAG_REVIEW=world` 的空 rooftop：实际太阳 depth **316 个非空像素 → 换 Edit 移除后0**，不是仅检查 draw 日志；`added=0 peak_grains=0`，地形/真实库存不变。depth readback 是显式、阻塞、恢复 layout 的诊断，不是性能数据。
- 生产 cycle：hover/pour 两种实例都执行 `published_triangles` 太阳 pass；可见面仍为 `cached_surface_cells`。`added=6656 peak_grains=9 contacts=16 conserved=true real_inventory_preserved=true`。完整 rooftop 1/10/50/512/1024批量、0/49/50/100/250ms与4224有支撑写入检查保持通过。
- 普通隐藏静音启动、上述检查及体素地板截图日志无 ERROR/VUID/panic。`target/soil-bag-world-voxel-floor.png` 真实呈现袋子在普通世界体素地面上的投影。
- 旧固定模型 rooftop 地板是独立的简化 authored-sun consumer（`static_scene.frag.slang`），**本来不接收世界阴影**，此次没有擅自改它或切换保存的场景默认值。普通花园/已有体素屋顶模式可见投影；用 `RE_FLORA_ROOFTOP_VOXELS=1` 只对诊断启动临时启用后者。`world-before/after.png` 的相机不同，仅作局部查看，不是匹配 A/B 或性能证据。没有新增性能预算/其他 GPU验收或用户最终美术认可。
- `config/gui.toml` 与开始前备份逐字节一致，spread13等本地改动不提交；没有新增 Debug 控件。未自动开启可见游戏。

日志：`/tmp/terrain-magic-soil-bag-world-{check,tests,instance-tests,frame-tests,all-tests,node-tests,release,depth,cycle,hover,voxel-floor,rooftop,smoke,tail}.log`。中间描述符错误保留在早期工作树 run log 和 `world-unlit-descriptor-error.log` 中，不作最终成功证据。

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
# world 模式验证真实太阳 depth 的写入/移除（空 rooftop、零土量）。
# 观察实际体素地板投影；不改变保存的默认模式：
RE_FLORA_ROOFTOP_VOXELS=1 cargo run --release -- --rooftop-poc
```

GPU 检查应串行；从同一 worktree 使用 `--latest-log` / `--tail-latest-log` 找到日志并检查 `[SOIL_BAG_REVIEW][CHECK]`。截图 fixture 的相机/时间只在 opt-in review 下设置，不改变普通游戏相机。
