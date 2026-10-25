# Sprinkler → vegetation：局部冲击、叶面积湿与低成本运动

日期：2026-10-25。源码基线：`966a03bde4d3fe6fbc3e566173630b3864f1b2b2`。

状态：**研究与实施前讨论报告，不是已实现功能或性能验收**。
交互版：[HTML report](sprinkler-vegetation-response.html)。HTML 中的运动是独立的二维示意模型，不是游戏画面，也不验证 Vulkan、喷流覆盖或 terrain 遮挡。

## 1. 最新需求与推荐结论

用户已明确：

- 需要可见效果，也需要控制性能成本。
- 草、树叶、苹果之间**不需要互相挡水**；不做树冠截留导致下游喷流衰减。
- **terrain 必须挡水**。
- 对一条水流采用一根代表 ray 的精度即可，不做密集多射线接触积分。
- 草的浸湿量按株保存，不按草模型的每个 voxel 保存。

推荐 V1：**单向、局部水响应**。共享喷流提供地形裁剪后的入水强度与方向；每株草拥有一个叶面湿润标量与紧凑局部运动状态；苹果拥有个体悬挂响应；树叶尽量复用现有响应，湿度按小叶簇/锚点共享。喷中偏转，离开回弹；湿草轻微下垂，局部喷击恢复更沉。已有风系统、土壤水分和掉果规则不借机重写。

不做：逐滴刚体/流体、逐草 voxel 积水、新的树干弹性系统、叶子截留后再重新灌溉、全场每像素扫描喷头、每帧 GPU→CPU 植物读回。

**仍值得看报告确认的体验项**：原来的 “drippy” 是指湿重下垂（droopy），还是也要看见叶尖滴落的水珠？推荐先下垂与回弹；可见滴水作为同一视觉迭代的可选后续，不默认扩大为二次灌溉。技术方案不要求用户选择 buffer、ID 或求解器。

### 单 ray 的准确含义

本方案把“一根 ray”解释为**一条代表性水束的中心线地形检测**，不是整个有宽度的扇形喷幅只测一根中心线。后者遇到只遮住半边喷幅的墙必然漏判，不能同时承诺 terrain 是硬遮挡。

当前 Sprinkler 已有弹道短段 sweep 和有限支持的地形裁剪。新增植物反馈优先**消费这份结果，不另给每株植物打一组 ray，也不为本功能削减现有喷流/土壤遮挡**。有限水束还必须复用同侧/局部 terrain 裁剪；不能仅因束中心没撞墙，就把宽核直接加到墙后植物。

若目标进一步变成“整个喷头总共只有一次直线 ray query”，那是另一个喷流模型变更：弧线、扇形和坡面覆盖都要一起重新定义，不能只改植物反馈造成画面、水分和受力三套不同的命中结果。本次不这样扩大范围。

## 2. 一手证据：支持低阶模型，但不支持任意物理夸张

### S1 — 真实叶片受水滴冲击可以拆成少量模态

Bhosale, Esmaili, Bhar & Jung (2020), *Bending, twisting and flapping leaf upon raindrop impact*, Bioinspiration & Biomimetics 15, 036007。DOI [10.1088/1748-3190/ab68a8](https://doi.org/10.1088/1748-3190/ab68a8)。实际读取了[作者提供的完整论文 PDF](https://bhosale2.github.io/assets/paps/leaf.pdf)。

- 摘要与结论（PDF pp. 1, 10）：桂树叶片的弯曲、扭转可用单自由度阻尼振子近似；冲击位置决定能量进入哪种模态。
- 实验方法（p. 5）：实验叶片疏水，试次后没有残留水；**不能拿这篇论文证明长期积湿质量或干燥速率**。
- 限制（p. 10）：小变形条件；大变形、强耦合、反复雨滴影响不在已验证范围。论文也没有给游戏草、苹果或连续旋转喷头标定参数。

工程推论：局部弯曲/摆动加少量状态是有物理根据的近似；不需要先引入 Cosserat beam、软体碰撞或每滴水的动力学。把根/叶柄固定，把喷击作为外力，不用随机位移替代受力。

### S2 — 水滴与柔性体的阻尼不是一个普适的“越湿越大”常数

Fowler, Marshall, Son & Jung (2024), *Resonance and Damping in Drop-Cantilever Interactions*。
作者[论文目录](https://sites.google.com/cornell.edu/sunnyjsh/publications)列出发表于 Physical Review Fluids 9, 123605，DOI [10.1103/PhysRevFluids.9.123605](https://doi.org/10.1103/PhysRevFluids.9.123605)。实际全文读取的是[作者 arXiv v1](https://arxiv.org/html/2406.18830v1)，不是声称已核对期刊最终版的所有细节。

- 实验对象是九种长度的聚碳酸酯悬臂与液滴，不是园艺草。
- 摘要及结果讨论：液滴与悬臂接近共振时，相位关系可使阻尼增强；长度改变会改变双方的相位与耦合。
- 因而 `wetness → damping` 可以是可调的风格化映射，不能宣称对所有草叶成立的材料定律。本方案不模拟液滴自身振动共振，也不照搬该实验频率。

### S3 — 质量、冲击与摆动必须分开

OpenStax 原版教材：[15.5 Damped Oscillations](https://openstax.org/books/university-physics-volume-1/pages/15-5-damped-oscillations)、[15.4 Pendulums](https://openstax.org/books/university-physics-volume-1/pages/15-4-pendulums)，已读取正文。

- 固定刚度的弹性振子：`ω₀ = sqrt(k/m)`。质量增加通常降低固有频率；它本身不等价于持续输入振动能量。
- 正阻尼、恒定外力的系统最终趋向新的平衡点；来回摆动来自瞬态、阀门脉冲、旋转扫过或明确的未解析扰动。
- 小角度理想单摆：`T = 2π sqrt(L/g)`，周期不依赖质量。苹果不能直接套“沾水变重，所以必然摆得更慢”的弹簧结论。

工程选择：草的湿重优先表现为下垂与喷击恢复变慢；苹果重点是水流推力和回弹，不夸大苹果表面水的质量。悬挂苹果不新增完整刚体；只有脱落后进入已有物理路径。

### S4 — 实例级合力与细节运动分层是已有工程实践

Tiago Sousa / Crytek，[GPU Gems 3, Chapter 16: Vegetation Procedural Animation and Shading in Crysis](https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-16-vegetation-procedural-animation-and-shading-crysis)，已读取作者章节。

§16.1 把整体弯曲与叶片细节分开，按实例聚合局部风源，使用高度权重和形变限制，并对实例/叶片做相位区分。该章支持“每实例聚合输入、顶点只消费姿态”的结构，**不提供本项目 Vulkan 实现的耗时，也不证明逐植物有状态模拟免费**。

### S5 — 湿润标量应独立于瞬时降水强度

Sébastien Lagarde，[Water drop 2b — Dynamic rain and its effects](https://seblagarde.wordpress.com/2013/01/03/water-drop-2b-dynamic-rain-and-its-effects/)，已读取原作者文章。

Wet level 与 rain intensity 是不同变量；停雨后仍可保留湿润并逐步干燥；动态物体可拥有自身 wet level。文章的材料常数、道路水层与反射不是草叶标定，不在本次复制。现有项目也不是一个只需调 roughness 就能完成的通用 PBR 草材质。

### 访问限制与未用作证据的搜索命中

- PubMed 页面未提取到有效正文；改读 S1 作者 PDF。
- Oxford 的 2022 综述、ScienceDirect 的 2020 叶倾角/储水文章返回 403；不把搜索摘要当作已读全文，不据此给湿重或容量定量值。
- 2015 Gart 等人的 APS 全文端点返回 401；只核对到作者目录，不引用其未读实验数据。
- S2 的 `/html/...v2` 返回 404，PDF 下载中止；随后成功读取实际存在的 v1 HTML。
- 一个搜索回答把引用 Gart 论文的 MIT `comtet-2015.pdf` 当成 Gart 原文；未采用该错误归属。

## 3. 源码审计：已有基础与真正需要新增的部分

以下是基线代码事实，不是已有喷水反馈的声明。路径相对仓库根目录，行号以本报告基线为准。

| 证据 | 目前的行为 | 实施影响 |
| --- | --- | --- |
| [grass_moisture_material.slang](../../shader/slang/grass_moisture_material.slang)；[现有交付说明](../reports/grass-moisture-response.md) | 支撑 dirt/sand 的 2-bit 湿度驱动草颜色，最多暗化 25%；没有每草 voxel 的叶面水状态 | 用户要的每株 wetness 是新增叶面状态，不是删除一套已有的逐草 voxel 水模拟；土壤水分保留 |
| [flora_types.slang](../../shader/slang/flora_types.slang):83；[active_surface_to_flora_instances.slang](../../shader/slang/active_surface_to_flora_instances.slang):35 | 草 Instance 是两个 uint，共 8 bytes；实例输出使用原子 append | 不能把 wetness 历史绑定 draw index；必须跟随稳定根位置/生命周期重映射。不要把原实例步长改成 4 bytes |
| [vegetation_response.rs](../../src/tracer/vegetation_response.rs):170–210；[vegetation_response_sample.slang](../../shader/slang/vegetation_response_sample.slang) | 高草、短草、苹果共用三个 XZ 响应场；网格间距 16 voxel；输入 Y=0 | 水有高度，不可直接往二维场塞喷水力；同 XZ 不同高度的苹果不应被一起推 |
| [vegetation_response_types.slang](../../shader/slang/vegetation_response_types.slang)；[solver](../../shader/slang/vegetation_response_solver.slang) | 每份现有响应状态 112 bytes，包含运动、4 份离散姿态、局部运动和时钟；`force` 参数实际作为目标平衡位移 | 复用解析阻尼积分方法，而非把 Newton 数值直接送进名为 force 的参数；也不把 112-byte 状态机械地复制给所有草 |
| [leaves.rs](../../src/tracer/vegetation_response/leaves.rs)；[tree.rs](../../src/tree_gen/tree.rs):95；[上传](../../src/tracer/mod.rs):7722–7789 | 叶片响应按现有渲染 voxel/元素拥有稳定 key；有共享小枝锚点信息 | “现有独立叶状态”不等于“每片生物叶一个状态”。湿度可按已有小叶簇锚点共享；不新增完整逐 voxel 水模拟，不顺便重做风系统 |
| [flora_vertex.slang](../../shader/slang/flora_vertex.slang):235–360；[flora_motion.slang](../../shader/slang/flora_motion.slang):155 | 草根部有高度权重，苹果有有界摆长几何，叶和果实再变换到枝条姿态 | 接触点要使用实际挂点/上一已发布姿态；受力转换到相应坐标系；渲染与阴影要一致 |
| [sprinkler_mist_prepare.comp.slang](../../shader/slang/sprinkler_mist_prepare.comp.slang):70–147 | 源速度、拖曳、有限支持和 terrain sweep 已在 GPU 求出；发布光学 lobe 与土壤 contact，但未发布完整植物受力输入 | 在同一 transport owner 发布归一化入水/动量代理，不能从“到地面的 contact”推出叶面已经喷中，也不以雾亮度冒充力 |
| [sprinkler_mist_geometry.slang](../../shader/slang/sprinkler_mist_geometry.slang):13 | 注释及实现明确 terrain-only，排除 animated trees；未知地形/预算耗尽保守阻挡 | 与用户“不互挡”一致。复用现有硬遮挡与有限支持裁剪；不要重建树冠水遮挡 BVH |
| [sprinkler_air_math.slang](../../shader/slang/sprinkler_air_math.slang)；[sprinkler_air.rs](../../src/tracer/sprinkler_air.rs):7 | optical-only 当前帧密度；受方向雾开关和密度值影响；亚格核会为光学目的扩宽 | 不能直接把它当物理水场；关闭雾/粒子不关闭植物反应，不复用扩宽核偷偷扩大受力范围 |
| [tracer/mod.rs](../../src/tracer/mod.rs):3778、4170 | 当前植物响应先录制，喷流准备更晚；喷流准备里还采样 DDGI 与阴影 | 同帧接入需拆分早期 transport 与晚期光学修饰，或定义有 generation 的已发布输入。不能只挪一个 descriptor 读取未写的同帧槽 |
| [fruit_release.rs](../../src/tracer/fruit_release.rs)；[fruit_handoff.rs](../../src/tracer/vegetation_response/fruit_handoff.rs) | 主路径是 frame-owned 稀疏异步掉果交接，另有旧/诊断整场读回入口；都依赖原二维场 | 新个体摆动必须进入最终发布姿态与速度，保持稀疏交接，不新增每帧 CPU 求解或阻塞读回 |
| [flora_lighting_cache.comp.slang](../../shader/slang/flora_lighting_cache.comp.slang)；[grass_band.vert.slang](../../shader/slang/grass_band.vert.slang) | cached / native 分别准备草颜色和姿态；缓存按 layer/instance 计算 | 状态每株只更新一次，不能在每个 layer/vertex 内重复积分；两个消费者都读同一结果 |

## 4. 推荐的模块与行为契约

### 输入所有者：Sprinkler transport

小接口概念：`sampleSpray(receiverPose, extent, worldInterval) → { exposure, push }`。

- 这是共享喷流的消费者，不是第二套 emitter registry、独立湿圆盘或新的喷水时钟。
- `exposure` 是非负的标量入水代理；`push` 是带方向的合力/冲量代理。当前源没有实测 kg/s 或 Newton 标定，报告不捏造 SI 参数。
- 两股相向水流的 `push` 可以抵消，但 `exposure` 相加。细碎冲击若需要，由非负活动量驱动、稳定种子去同步；不能凭空在停水后持续加随机能量。
- 空间筛选优先按水束/有限支持建立局部候选，而不是每株扫描全场全部喷头。先复用现有 bounds；若发布水束局部索引，溢出必须显式处理，不能静默截断水源。
- terrain 检测属于水束输入所有者。有限支持不得穿墙；不要把“不要逐滴碰撞”偷换成“完全不管遮挡”。仅在中心线 hit 位置截断但允许横向核穿墙不满足硬约束。
- 植物只接收、不改变共享水源。叶面 wetness 是饱和的外观/力学代理，不宣称守恒水库；土壤输入仍按当前 contact，一份视觉滴水不得再重复灌溉。

### 状态所有者：植被接收者

| 对象 | 推荐粒度与职责 |
| --- | --- |
| 草 | 一株/一个根实例一个 `surface_wetness`；另有紧凑局部偏转和速度。保留 8-byte 绘制实例，用 sidecar 绑定稳定身份 |
| 苹果 | 每颗挂果两个摆动自由度及角速度；用实际高度和挂点受力。不通过二维场扩散；喷水默认不触发掉果 |
| 树叶 | 湿度按已有叶簇/小枝锚点聚合，局部喷击复用既有叶响应。大簇不能一个中心命中就整簇同幅抖动；接收点/空间权重仍需限定局部支持 |

生命周期：LOD、相机剔除、列表重排不重置；移除/重种是新身份，不继承旧水；有效重建保留存活者历史；世界载入/替换清理旧 transient 状态。第一版不修改世界存档格式，叶面水和局部振动作为 transient；土壤存档原样保留。关闭 A/B 恢复原渲染与行为，不把水状态写进原土壤湿度。

### 两个时间尺度

以下是设计模型，不是实验拟合值：

```text
湿润：dw/dt = a·Q·(1−w) − b·w,    0 ≤ w ≤ 1
运动：m(w)·q'' + c(w)·q' + k·(q−q_rest(w)) = F_spray
```

恒定 Q 的湿度可用解析指数更新，避免帧率依赖。运动继续使用已有的解析阻尼思路，分清目标平衡位移、速度和真实冲量。所有时间使用世界时间；暂停不补喷，调参不重置相位，长帧不靠无上限补步。

V1 的“湿重”明确限于**静态下垂和局部喷击恢复**。已有共享风运动不被悄悄重建。若后续要求每株湿度同时改变全部风振动频率，需要个体连续相位或合成受力求解，不能直接用 `sin(time * frequency(w))`，也不能把已经积分后的风位移再次当外力。这个扩展必须单独计状态/性能，不把 V1 描述成完整湿草空气动力学。

颜色建议保留现有土壤暗化兼容，但让叶面湿润贡献有界；例如用两种暗化强度的最大值而不是连续乘两次，防止湿土+湿叶无意叠加变黑。这是材料表现选择，不让土壤湿度驱动喷水冲击或叶面质量。

## 5. 成本：可估算状态大小，不能凭代码承诺帧耗时

### 量级算例，不是当前场景计数/最终 ABI

假设 100,000 株草：

- 只有一个 f32 wetness：400,000 bytes ≈ **0.38 MiB**，但不能独立表达回弹。
- wetness + 二维偏转/速度：逻辑最少 5 floats；若为对齐/元数据按 **32 bytes/株** 预算，则一份约 **3.05 MiB**。
- 若照搬当前“三帧槽 × 双输出”分配方式，就是约 **18.31 MiB**，尚未含输入、重映射和容量余量；按 131,072 的二次幂容量则是 **24 MiB**。这不是承诺一定要六份副本，而是防止只报“一个 float 很便宜”。
- 把现有 112-byte 完整响应状态复制给每株：一份约 **10.68 MiB**，六份约 **64.09 MiB**，还没算容量取整。不是推荐路线。
- 为既有每喷头 256 个积分样本额外发布一个 float4：`16 × 256 × sprinkler_count` bytes/份；256 喷头为 **1 MiB/份**。这是附加通道的算例，不含已有支持几何、候选索引和 frame slots，也不是最终布局。

### 每帧工作

目标是 `O(共享水束准备 + 有效接收者 × 局部候选数)`，而不是 `O(全场植物 × 全场喷头 × 256 × 模型voxel)`。局部候选在重叠喷头极端场景仍会增长，不能说严格常数时间。

- 无喷头且所有局部状态已静止/干燥时跳过新增更新；停喷后仍需完成干燥和回弹，不能直接清零。
- 湿度可降频更新/按时间戳精确衰减；窄喷流命中与冲量不能因降频漏掉，需要积分世界时间内的输入。先正确，再按 Release 测量选择频率。
- “只更新屏幕可见植物”会让离屏草不变湿，重新出现时状态不一致；物理受影响集合与渲染可见集合必须分开。
- 不在片元 shader 做受力，不为每层草更新一次湿度，不对每个接收者重复读取同一喷头的所有地形轨迹。
- 土壤迁移/存档仍需要 voxel 水分；每株 wetness 不会把这部分成本消掉。

本报告没有新增功能的 CPU/GPU ms、FPS 或性能通过结论。旧 Sprinkler 的历史 benchmark 不能代替本次反馈路径的增量测量。

## 6. 实施顺序与验收

1. **共享输入与 terrain 契约**：把 transport 的植物输入发布提前，与光学修饰分清职责；验证零 duty、暂停、mist=0、particles off 和 terrain 墙仍一致。首先不改变原浇水量。
2. **个体状态与生命周期**：实现草 sidecar、苹果个体状态、叶簇湿度归属。重排/LOD/移除重种/载入测试先过，不做缓存序号假身份。
3. **运动与材质接入**：草根固定，局部叶响应，苹果固定摆长；cached/native、pixelized、阴影、掉果交接消费同一最终姿态。
4. **可见候选**：Debug 中提供可保存 A/B checkbox，未勾选为原模式。按 concern 组织 Moisture / Spray Response 控件，单一保存归属，不给每类对象散落多份开关。不自动打开可见游戏。
5. **先视觉确认，再性能接受**：先交效果与诚实测量；小幅性能回退不单独阻塞视觉评审。崩溃、非法资源、terrain 漏水和状态错绑不能豁免。

### 行为验收矩阵

| 场景 | 必须看到/读到 |
| --- | --- |
| 关风，水束扫过草 | 喷中先向入射方向偏转；离开有衰减回弹；根部不移动 |
| 关风，停水但仍湿 | 残余摆动逐渐停止；保持下垂并缓慢恢复，不永久抖动 |
| 两颗相同 XZ、不同高度苹果 | 只有实际喷中的苹果受力；摆长固定 |
| 相向双喷头 | 入水相加，净定向力抵消；不因相消而变干 |
| terrain 全墙/半墙/薄墙/台阶 | 墙后接收者无输入；有限支持不跨墙；墙前正常。半墙不能只测整个喷幅中心 |
| 植物前后排列 | 不互相截断喷流；后方未被 terrain 挡住的接收者仍可响应 |
| mist=0 / particles off / 切换光学模式 | 物理输入不变，不绑定视觉密度 |
| 列表重排/LOD/隐藏恢复/编辑/重种 | 存活者历史跟身份；新生命无旧湿度/速度；离屏不冻结物理时间 |
| 掉果 | 最后可见位置和速度连续交接，不新增每帧 readback |
| 30/60/120 Hz、阀门脉冲、长帧、暂停 | 累积入水与冲量在定义容差内一致，无非有限值/爆炸/补喷 |
| A/B 关闭 | 原路径仍可用，无新增持续计算；保留用户保存值 |

Release 配对测量应覆盖无喷头、单喷头、重叠多个喷头，稀疏/密集草与多树；固定相机、配置、时间、世界内容，报告 CPU frame 与 GPU transport/response/cache 时间及 p50/p95、峰值显存、有效接收者和候选计数。候选列表构建/重映射尖峰单列，不能只报稳态平均。无预先杜撰硬性能预算。

## 7. 报告本身的验证与范围

- 本次只新增本 Markdown、HTML 和报告示意模型的测试；不改 Rust、Slang、生成文件或 GUI 默认值。
- HTML 自包含，不加载远程脚本/字体/图片。场景图是报告代码绘制的原创 SVG 示意，不导入论文图片或第三方资产。各视觉/时间常数是解释性选择，不是实测拟合。
- `node --test docs/research/sprinkler-vegetation-response.test.mjs`：**8 项通过**，覆盖有界/有限状态、推开/回弹/静止、相向水流、湿重切换、30/60/120 Hz 输入积分、解析更新组合、苹果摆长及本地引用完整性。只验证示意模型与展示契约，不能充当游戏端实现验收。
- 用独立 agent-browser 会话检查 `file://` 直接打开、四种情景、湿重开关、播放/暂停/重置和时间轴；无页面错误、无外部资源请求。1440×1000 与 390×844 下无页面级横向溢出，窄屏图表使用可键盘访问的局部滚动。已查看开头、运动区、窄屏截图，位于 `target/sprinkler-response-report/report-*.png`。
- 桌面与窄屏 axe 检查均为 **0 violations**；SVG 文字/裁剪内容等仍有自动对比度 `incomplete`，不宣称完整 WCAG 合规。初次窄屏检查发现表格滚动区不能键盘聚焦，已添加具名 focusable region 后复验通过。
- 已在本工作树执行 `cargo build --release`，通过；日志 `target/sprinkler-response-report/release-build.log`。这是未修改游戏行为的基线构建，不是新功能运行证据。
- 用户原有 `config/gui.toml` 修改必须保持不变。检查基准 SHA-256：`af8f139df37e67e8f4543f8ce065a33babf08432589cc8d224c24743a580b8e2`。
