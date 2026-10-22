# 普通启动约 30 FPS：GPU 映射内存上传诊断

调查日期：2026-10-23；代码基线 `cd936f5a`。

**结论：本机可复现的主要瓶颈是 CPU → GPU 映射内存上传落入 glibc 的 `REP MOVSB` 慢路径，不是新水雾，也不是单纯的 GPU 算力不足或 30 Hz 显示模式。本文交付的是诊断，不是已完成的修复。**

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

这是“通用系统内存复制策略不适合本机这类 GPU 映射地址”的问题。具体 Vulkan 分配的实际 memory flags / CPU 缓存属性尚未逐一读出；写合并/非缓存映射与该指令的相互作用是下一步实现时需要核实的细节，不能据此宣布驱动、glibc 本身有已确定的缺陷。

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

**本次未修改 Rust/Shader 或上传实现，未把临时环境变量写入启动器、配置或系统。原问题在未干预的普通构建里仍存在。** 没有声称完成修复或普遍达到 60 FPS。

## 复现与证据

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
