# 游戏与网页统一参数化花头

## 用户决策

采用已经可用的单轮径向模型作为两端共同基准，接受其无法准确表达所有花的事实；本轮不增加多轮、融合花冠或兰花器官系统。删除旧的独立造型分支，让现有花通过数据预设收口，尽快进入视觉验收。

## 唯一来源

- 将 `parametric-flower.mjs`、`palette-mask.mjs` 从网页模型目录移到 `assets/models/`，没有留下第二份实现或 fallback。
- `assets/models/flower-source.mjs` 现在只有六种花的数据预设和统一生成器调用。删除旧 `petals`、`tulip`、tube/blade/lathe 及整株茎叶配方；不再保留按物种走不同几何代码的路径。
- 网页 `flower-catalog.mjs` 直接引用这六项正式预设，仅额外定义白花老鹳草、Gillenia、四瓣、五瓣尖星与自定义花。没有重复维护两份正式形态参数。
- 保留 `flower-head.mjs` 的通用 attachment-local 归一接口；它不是独立花型。
- publisher 与 Cargo 指纹同时覆盖预设、生成器、权重工具、头接口和发布脚本，任一变化必须重发 `flowers.json`。

| 游戏身份 | 共用参数基准 |
| --- | --- |
| 野老鹳草 | 五片圆宽瓣，浅聚拢 |
| 勿忘草 | 五片短宽瓣、小花心；不再借波斯菊八瓣模板 |
| 滨菊 | 16 条窄瓣、平黄色花盘 |
| 波斯菊 | 八片宽瓣、瓣尖缺口 |
| 松果菊 | 12 条下垂瓣、锥状凸起花心 |
| 郁金香 | 六片聚拢瓣的杯形近似 |

六个 stable ID、顺序、英文显示名、茎层数（41/42/42/42/43/37）保持，已有花园不重新映射物种。新增网页研究花不擅自加入游戏注册表。游戏继续负责原有 voxel 茎／可选茎实验、风、增长、种植与整体／花头尺寸控制；本轮没有改 GUI 默认值或增加仅 App 保存的滑杆。

## 共用颜色，而不只共用形状

以前游戏缓存每个命中只存固定器官材质角色，不能表达统一贴纸渐变。本轮发布相同的顶点 UV，以及 `palette-mask.mjs` 对预设权重图／palette 解码出的 **sRGB8 颜色图集**，与网页 DataTexture 的字节一致。

- 四槽权重仍在线性光中混色，由共用 JS 发布，不在 Rust 另写一套模板或渐变配方。
- Native CPU 解码这些 sRGB8 texel 为线性值，放入既有 `model_cache_palette` 缓冲的 atlas 区域；固定四槽前缀的 w 分量记录偏移／宽／高，不增加描述符。
- 花头 bake 插值 UV，然后把两个 UNORM12 坐标编码为一个可精确表示的 24-bit float integer。原 `ModelBakedSurface` 仍为 32 字节，`normalMaterial.w` 在花模型中从器官角色变为 atlas UV；其他模型的材质约定未改。
- 新 `flower_palette.slang` 做 clamp-to-edge 双线性采样：texel 先解码至线性，再过滤，与网页 sRGB DataTexture、LinearFilter、flipY=false 一致。之后沿用游戏环境光照和深度显示。
- UV 最大误差为每轴 `0.5/4095`，128² 图集约 `0.016` texel；不声称像素最终颜色和网页灯光逐字节相同。
- cache format version 从 6 升到 7。共用**生成器**不等于共用**同一几何缓存**；不同形态独立 bake，未来形状与 UV 真正一致的预设仍可共享。

网页绘制／修改不是实时游戏编辑器，仍需有意写回预设／权重源并运行 publisher。此步骤统一的是已发布默认形态与颜色来源，不自动同步页面内存。

## 发现并修复的真实问题

初次 native 测试拒绝退化三角形：旧网页生成器在 `sin(PI)` 的极小残余上取小于 1 的幂，导致未开缺口的瓣尖出现假宽度和细碎面。这些面在 f32 加到茎顶后塌缩。

根因修复放在共用生成器：未开缺口的瓣尖宽度精确为零。保留 native 非退化检查，不靠跳过坏三角形或放宽 normal 断言掩盖。网页和游戏同时得到修复；有缺口的瓣尖仍保留其实际宽度。

既有 `validate-flower-models.mjs` 仍硬编码历史 8 物种，改为读实际共用目录数量 6，移除一株的阶段按目录数量减一；未放宽阶段数、深度、Vulkan 错误和生命周期检查。

## 验证

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：1260 passed，4 ignored。含发布数据／UV／图集拒绝校验、完整花头 attachment 与独立尺寸／高度／边界、所有变换不改变 UV、atlas 线性上传值、不同形态不错误共享 bake。
- `python3 scripts/run_slang_tests.py`：27 passed。新增 12-bit UV 编解码全轴值、夹取与量化误差测试。
- `node --test experiments/model-preview/tests/*.test.mjs`：34 passed。新增 native JSON 与统一生成器所有 indices／positions／UV 的再生成一致性，以及与网页颜色解码的每个 atlas 字节一致性；六项 ID／茎层数锁定。
- 四套浏览器回归通过：既有叶／蝶／苹果；11 花型 242 姿态、原 RGBA、GPU 深度最大误差 `0.00002488470022710132` 与 8 遮挡夹具；贴纸实际绘制／撤销／PNG／资源释放；3 动物 75 姿态。
- `env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5`：通过，日志检查无 ERROR/panic/VUID，shutdown failures=0。
- `env -u WAYLAND_DISPLAY node scripts/validate-flower-models.mjs --seconds 45`：六种花、9 阶段、8/32/64px、view／growth／lifetime／resize，通过并产出 native 截图；GUI 配置 hash 不变。20 秒初跑只到第 7 阶段，记录为不完整，延长后完整通过，不删除第 8 阶段检查。
- `RE_FLORA_MODEL_CACHE_REVIEW=1 RE_FLORA_FLOWER_MODEL_REVIEW=a` 隐藏静音 Release 8 秒：format 7 runtime 通过；kind 3 检查 `3,145,728` 缓存记录，mismatches=0；其余三种模型也为 0，shutdown failures=0。
- 已查看实际 native 截图与网页总览，确认新花头接在既有茎上；这不是用户美术接受，也未自动启动可见游戏。

日志：`target/unified-native-{node,fmt,check,tests,slang,smoke,review,cache-review}.log`。
浏览器产物：`target/unified-flower-review/`；native 图片／summary：`target/flower-native-review/`。
生成资产仅 `assets/models/flowers.json`，通过 publisher 更新；未手改 shader-derived Rust structs。

## 已接受限制与性能边界

单轮、相同花瓣模板、简单花心的范围在生成器注释中保留；郁金香非双轮交错／闭合苞，重瓣玫瑰、融合钟形花冠、兰花结构仍不准确支持。极宽或强聚拢仍可能穿插。没有借回旧物种独立 mesh 伪装成支持，也没有扩大本轮几何 feature。

现在六种形态都独立，whole/head cache source 从 10 到 12；32px / 256 views 时花缓存从 80 MiB 增至 **96 MiB**，加上六张 128² linear float4 atlas **1.5 MiB**。生成三角形也比旧低模更密，原始数据与 bake 成本变大。这是已知资源取舍，不是性能优化；本轮没有同场景旧版／新版 Release 性能 A/B，因此不声称性能接受通过。先让用户验收可见效果，再独立决定简化采样密度或性能优化，不能以缓存共享为由把五瓣花偷偷替换成八瓣花。
