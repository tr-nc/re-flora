# 普通启动约 30 FPS：GPU 映射内存上传诊断

调查日期：2026-10-23；代码基线 `cd936f5a`。

**结论：本机可复现的主要瓶颈是 CPU → GPU 映射内存上传落入 glibc 的 `REP MOVSB` 慢路径，不是新水雾，也不是单纯的 GPU 算力不足或 30 Hz 显示模式。第 1–4 节保留原始诊断；后续正式修复及验证见第 5 节，已不依赖临时环境变量。**

## 1. 影响范围

用户报告刚打开游戏约 30 FPS。本机普通启动、默认花园、没有喷头时，隐藏/静音 Release 两次复现 **25.61 / 24.78 FPS**。实际应用尺寸均为 **5120×2880，内部 1024×576，pixel stride=5**，自动 FIFO。没有更改画质、窗口尺寸、桌面设置或保存的配置。

- Intel i5-12600KF / RTX 3060 Ti / NVIDIA 580.159.04 / glibc 2.42。
- 默认构建保留 Vulkan 验证层。关闭验证层仍只有 **28.86 FPS**；不是主要原因。
- 换成已保存花园仍只有 **26.00 FPS**；不是只发生在新花园初始化。
- 使用旧性能 fixture、零喷头、固定太阳仍只有 **27.81 FPS**。保存的普通模式本来就关闭了自动昼夜循环，不能把 fixture 差异解释为昼夜更新问题。
- 显示服务器报告物理显示器当前 3840×2160 / 60 Hz / scale=1.5；这不等于应用交换链尺寸。上述 5K 尺寸来自实际 `[SCENE_PIXELS]` 日志，未把两者混用。
- 用户原可见会话日志缺少逐帧 scope。这里是同机隐藏原生交换链路径的复现，不冒充对那次可见会话的完整 trace，也不声称可见体验已经修好。

原始用户日志保存在 `target/startup-fps-review/reported.log`。

## 2. 根因与因果证据

### 普通构建的同二进制交错复核

以下四次均使用原默认 Release，SHA256：

```text
6b862bf7b63509d92adb3d08214771fbae1cfafff7f4041ed0b45874f0e74963
```

唯一实验变量是在**单次子进程**设置：

```sh
GLIBC_TUNABLES=glibc.cpu.x86_rep_movsb_threshold=1073741824
```

它提高 libc 选择 `REP MOVSB` 的大小阈值，使本例上传改走其他复制路径。它不关闭验证层、不改变游戏内容、渲染参数或 GPU 算法。**这是机制定位用的临时干预，不是推荐的全局设置或正式解决方案。**

| 运行顺序 / 日志 | 实际 cadence FPS | CPU 命令录制均值 ms | 其中 shadow-prepass 录制 ms | GPU 整帧均值 ms | 暖机后 CPU scope 样本 |
|---|---:|---:|---:|---:|---:|
| 原设置 `default-hidden.log` | 25.61 | 24.883 | 9.601 | 12.661 | 9 |
| 临时绕开 `default-fast-r0.log` | 58.36 | 3.332 | 0.926 | 10.636 | 21 |
| 撤销干预 `default-original-r1.log` | 24.78 | 26.288 | 10.312 | 13.300 | 8 |
| 再次绕开 `default-fast-r1.log` | 58.33 | 3.379 | 0.931 | 10.373 | 21 |

每次 `--auto-exit 16`，分析排除首个 scope 后五秒。普通模式每 30 帧输出一次 scope：表中 CPU/GPU 均值来自这些稀疏样本，FPS 来自首尾帧号差 / 日志时间差，**不是 GPU 时间的倒数**。shadow-prepass 是录制总时间的子区间，不能相加。世界、风和成长正常推进，并非同一瞬时状态回放。此处是较大效应的因果诊断，不是全场景性能验收。

先前无验证层实验也显示相同机制：**28.86 → 58.41 FPS，录制 23.524 → 1.215 ms**。因此，既不能把问题归咎于验证层，也不能把关闭验证层当作修复。

### 调用栈与机器指令

使用 GDB/MI 暂停采样主线程，共 35 次：

- 13 次落在 `EguiRenderer::prepare_command_buffer → Mesh::update → Buffer::fill → __memmove_avx_unaligned_erms`。
- 10 次落在 `record_shadow_prepass → VegetationResponse::record → Buffer::map_buffer_mem_and_write → __memmove_avx_unaligned_erms`。
- 另有树姿态上传、帧获取等待、成长/碰撞等样本。

这些是低频、会扰动调度的定位样本，**不是精确时间占比或 GDB 下的性能结果**。高频命中的复制指令地址为 `0x00007ffff7cb0147`；对同一 libc 的反汇编确认它是：

```text
<__memmove_avx_unaligned_erms+775>: rep movsb (%rsi),(%rdi)
```

本机默认 `glibc.cpu.x86_rep_movsb_threshold=0x840`（2112 字节）。共享上传实现把 GPU 映射地址交给 `ash::util::Align::copy_from_slice`，由通用 libc 复制策略选择路径：

- `crates/re-flora-vkn/src/memory/buffer/buffer_impl.rs`：`map_buffer_mem_and_write`、`fill`。
- `src/egui_renderer/mesh.rs`：每帧顶点/索引上传。
- `src/tracer/vegetation_response.rs`：响应输入 staging 上传。

**证据链：相同二进制可复现低帧率 → 主线程集中停在映射上传的同一复制指令 → 只改变 libc 复制策略就移除约 22–23 ms 录制开销 → 撤销即回退 → 再施加又恢复。**

这是“通用系统内存复制策略不适合本机这类 GPU 映射地址”的问题。原始诊断时尚未逐一读出实际 memory flags；后续原生分配/上传回归确认了 `HOST_CACHED` 与非缓存分配两条路径（第 5 节）。这仍不能据此宣布驱动、glibc 本身有已确定的缺陷。

## 3. 何时引入

**尚未确定首次出现低帧率的提交或系统变更。**

`git blame` 显示通用上传里的 `Align::copy_from_slice` 已存在于 `87ea4dab8`（2025-03-07），明显早于本轮水雾实现。但旧代码存在并不能证明旧版本在当时机器、libc、驱动和分配布局上就同样慢。没有做历史二分，也没有证据指认最近一次系统升级、驱动缓存或某个水雾提交。

前一轮水雾报告的 58.31 FPS 是另一个场景/尺寸和无验证层计时构建，不能直接当作当前普通启动的基线；本次已对**原默认二进制**独立复核。

## 4. 干净修复的边界

正式修复应归属 **`re-flora-vkn` 共享 Buffer 上传层**，而不是关 UI、禁用植被、降低分辨率、关闭验证层，或给每个调用方打补丁。

候选设计需先核实分配属性，再选择适合映射内存的写入策略，覆盖 typed/raw/ranged 上传，同时保留：

- 字节内容、偏移、长度、边界与可能的未初始化 padding 语义；不能为改成 SIMD 而引入 Rust 未定义行为。
- host-write 可见性、必要的 CPU store fence / Vulkan 同步，以及既有 frame-slot 所有权。
- 普通缓存内存和非 x86 平台的正确后备路径。
- 真实 UI、植被、树姿态调用和不同大小/对齐的 Release 应用回归；不能只跑系统 RAM 的 memcpy 微基准。

**原始诊断阶段未修改 Rust/Shader 或上传实现，未把临时环境变量写入启动器、配置或系统；当时未干预的普通构建仍有问题。** 后续修复如下，不将本机结果推广为普遍 60 FPS 保证。

## 5. 正式修复与验证（2026-10-23）

### 单一上传入口与写入契约

修复位于 `crates/re-flora-vkn/src/memory/buffer/`：

- `buffer_impl.rs` 的 typed/raw/ranged/element/uniform 写入共享 `write_mapped`，在写入前保留原有范围检查；公共 API 和 `fill<T: Copy>` 约束不变。
- `mapped_write.rs` 根据**实际分配属性**选择策略：仅 x86-64 的 `HOST_VISIBLE && !HOST_CACHED` 映射使用 SSE2 写入；缓存映射及其他架构仍用标准 `copy_nonoverlapping`。不要求 `DEVICE_LOCAL`，因此也覆盖 allocator 回退到非缓存 host heap 的情况。
- 专用路径先处理不对齐前缀，再用 16 字节对齐的 `MOVNTDQ` 流式存储复制完整向量，最后处理尾部；只读 source、只写 destination，不访问范围之外。`SFENCE` 和 asm 默认 memory effects 保证这些存储在返回、提交前被正确排序。既有 frame-slot 所有权、Vulkan 屏障、分配属性和提交次序未改变。
- 使用受限的原始汇编复制对象表示，**不**把 `Copy` 类型可能未初始化的 padding 构造成 `&[u8]` 或已初始化的 Rust SIMD 值；缓存/其他架构的 raw-pointer fallback 同样保留这个契约。
- 不修改 libc 全局策略、不强制新分配位置、不关闭验证层、不降低画质。未修改 Shader 或生成文件。

新增独立原生回归 `crates/re-flora-vkn/examples/validate_mapped_uploads.rs`，只创建隐藏窗口、不创建音频；通过真实 Buffer API 上传，复制到 GPU-only buffer，再复制至 readback，等待 GPU job 后校验。实际属性：

```text
CpuToGpu: DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT
GpuToCpu: HOST_VISIBLE | HOST_COHERENT | HOST_CACHED
```

### 默认 Release 的三对交错复测

基线二进制是诊断时保存的原默认 Release；修复后也是普通 `cargo build --release`，**两者都保留验证层，不设置 `GLIBC_TUNABLES`**。这是用户默认路径的 CPU/cadence 验证，不是无验证层 GPU 性能归因。

同一花园、相机、保存的画质、无喷头，自动 FIFO；接受的六次运行均为 **4096×2560 / 820×512 / stride=5**。每次 16 秒，首个 CPU scope 后丢弃五秒；旧版每轮 8 个暖机后样本，新版每轮 21 个。表中为每轮均值；汇总对三轮各自统计等权平均。帧率来自实际帧号/时间差。

| 轮次 | 旧版 FPS | 修复后 FPS | 旧版 CPU 录制 ms | 修复后 CPU 录制 ms |
|---|---:|---:|---:|---:|
| r1（after→before） | 25.26 | 58.38 | 23.391 | 3.282 |
| r2（before→after） | 25.40 | 58.40 | 23.210 | 3.211 |
| r3（before→after） | 25.06 | 58.45 | 23.512 | 3.201 |
| **三轮平均** | **25.24** | **58.41** | **23.371** | **3.232** |

CPU 录制减少 **20.139 ms / 86.2%**。世界、风、成长正常推进，不是相同瞬时帧重放。没有将 GPU 整帧波动归因给上传，也没有根据 GPU timestamp 倒算 FPS。

**保留的排除记录**：首对 r0 自动选中的显示器从 DP-2（5120×2880 / 1024×576）变成 HDMI-1（4096×2560 / 820×512），跨运行尺寸断言失败。因此整对 r0 的两次运行均不进入匹配汇总，保留全部原始日志并补做 r3；没有人为改桌面/画质，也没有混合尺寸计算提速。更早一次修复后的 5K 普通启动得到 58.42 FPS / 3.339 ms，仅作单次复现解除证据，不与 4K 三对汇总混算。

### 验证结果与限制

- `cargo fmt --check`、`cargo check`、普通 `cargo build --release` 通过。
- 完整主项目测试：1678 app + 4 library + 1 integration 通过，既有 ignored 保持原样。
- vkn：71 个单元测试通过（5 个新增）；Release 定向 6 个通过，包含旧范围检查。
- 新测试包括 73,728 个源/目标独立对齐、长度、缓存策略组合；810 个 Linux guard-page 组合检测越界读写；带 padding 的 `Copy`、零长度、缓存策略与原范围校验。
- 同步验证层开启的原生回归：修复前后均通过 **232 个真实 GPU 往返用例**，覆盖两类实际分配、0–1 MiB 上传、对齐/非对齐、边界拒绝不修改数据、uniform/element/typed 写入及 padded Copy。该例子不混入普通 `cargo test`，也不作为性能证据。
- 默认花园 resize：5 个请求被合并成 3 次一致的资源发布，最终 1280×720 / generation=4，正常关闭。
- 16 喷头原模式→air→原模式、暂停/恢复、零密度/零占空比及颗粒/全局光开关：同步验证通过；暂停 contact/droplet 数据保持相同；已打开检查两张原/air 截图，无明显上传损坏。不是新的艺术效果验收。
- 六次匹配性能运行及最终原生回归/resize/air-switch 均无应用 WARN/ERROR/VUID/SYNC-HAZARD，正常关闭；不据此宣布历史音频 underrun 或 terrain-removal 问题已修复。
- 新增 `scripts/check-startup-cadence.mjs` 和四个 Node 测试，使用实际 cadence 与 CPU 录制预算检查完整日志；保留 warnings，拒绝短日志、错误、尺寸变化或非递增帧。对保存的旧版日志能 RED，对修复后六次中的三个 after 日志能 GREEN。
- 当前只在 Linux x86-64 / 本机 GPU 验证。非 x86 后备路径在逻辑上保持标准复制，缓存分支已有测试；没有声称跑过 Windows/macOS/ARM 或所有硬件。

复核命令（重型命令同样使用既有 flock；不要嵌套锁）：

```sh
cargo test --manifest-path crates/re-flora-vkn/Cargo.toml --target-dir target
cargo test --release --manifest-path crates/re-flora-vkn/Cargo.toml --target-dir target mapped_write
cargo run --release --manifest-path crates/re-flora-vkn/Cargo.toml --target-dir target \
  --example validate_mapped_uploads
node --test scripts/tests/check-startup-cadence.test.mjs
RUST_LOG=info target/release/re-flora --hidden --mute --perf --auto-exit 16 > target/startup-cadence.log 2>&1
node scripts/check-startup-cadence.mjs target/startup-cadence.log --min-fps 50 --max-record-ms 8
```

50 FPS / 8 ms 是这次同机回归的判别阈值，不是其他硬件的通用预算。普通 Release 已重新构建；配置、相机库、已有世界保持哈希不变。没有启动可见窗口、播放声音或推送。

修复证据在 `target/mapped-upload-fix/`：`inputs-before.sha256`、保留的 `before` 二进制、所有编译/测试日志、`native-{before-final,after,final}.log`、`matched/` 的八个完整日志及六个接受结果的 `summary.json`、`resize.log`、`air-switch.log`/截图、`upload-disassembly.txt`、`audit.json` 与最终 Release 哈希。早期原生回归例子缺一个结尾括号的编译失败保留为 `native-before.log`，修正后才进行了上述验证。

## 原始诊断复现与证据

原始与干预命令（均在 `/tmp/re-flora-garden-delivery-heavy.lock` 下串行执行；存在 `DISPLAY` 时仅在子进程 unset Wayland 变量）：

```sh
RUST_LOG=info target/release/re-flora --hidden --mute --perf --auto-exit 16

# 仅定位；不要保存为永久环境设置
RUST_LOG=info GLIBC_TUNABLES=glibc.cpu.x86_rep_movsb_threshold=1073741824 \
  target/release/re-flora --hidden --mute --perf --auto-exit 16
```

证据根目录：`target/startup-fps-review/`。包含原日志、每次完整运行与 `*-summary.json`、临时 `analyze.mjs` / `sample-main-stack.mjs`、GDB 原始记录、`memmove-disassembly.log`、显示状态、输入哈希与 `audit.json`。

- `cadence-red.log`：原默认 25.61 FPS 触发 `<50` 诊断断言；50 仅用于区分本例的 30/60 档，不是新增通用预算。
- 四次默认构建复核均正常关闭、无应用 WARN/ERROR/VUID/SYNC-HAZARD。
- `loaded-no-validation.log` 有一次音频控制队列满警告，保留，不据此认定音频已经修复。
- `same-binary-validation-disabled.log` 是失败的 loader 过滤尝试：应用显式请求的层被过滤后 `ERROR_LAYER_NOT_PRESENT`，exit 101；不计入性能结果，随后改用既有 feature 做验证层对照。
- PATH 上 `nsys` 只是未安装 Nsight Systems 2025.6.3 的包装器，不能用于采样；转用现有 GDB。
- 首次 GDB/MI 未启用 async，采样数 0；保留 `gdb-raw.log`，只有 `gdb-async-stacks.log` 的 35 个栈用于定位。
- 已恢复并重新构建普通 `cargo build --release`，原二进制、GUI、相机库 SHA256 均匹配调查前；没有可见窗口、可听播放、存档覆盖、推送或遗留子进程。
