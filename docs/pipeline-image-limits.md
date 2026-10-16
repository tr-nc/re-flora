# Pipeline image descriptor 约束

## 影响范围

Apple M4 Pro / macOS 的 Glass 模式（包括 `--rooftop-poc`）在创建
`glass_resolve.comp` layout 时请求 10 个 storage image descriptor，设备每 stage
只支持 8 个。普通启动不创建这条 pipeline，之前的 pointer 重构不是引入原因。
其他设备取决于其实际限额，不代表所有 GPU 都失败。

## 根因

`readonly RWTexture2D` 只是 storage image 的只读访问声明，不会变成 sampled
image。四个不可变 screen fact（opaque depth/provenance、glass front depth/data）
和其余六个 storage image 合计 10 个；旧 layout owner 未在调用 Vulkan 前检查
设备约束。SPIR-V 反射测试先复现 10 > 8；加入设备 guard、保持旧 shader 再运行
原生 rooftop 后，错误明确定位为 `glass_resolve.comp`、COMPUTE、required=10、limit=8。

计数依据 Vulkan 的 `VkPipelineLayoutCreateInfo` 合同：普通非 update-after-bind
布局的 VUID 03020 / 03034。Storage texel buffer 也计入 storage-image 预算；
sampled / combined image 与 uniform texel buffer 使用另一组 sampled-image 预算。

## 引入时间

四个只读 screen 输入以 storage image 声明可追溯至 `491a8ea1`（2026-08-28，
`add hybrid glass voxel resolve`）。已独立构建确认原始基线 `190c9249`
（2026-10-13）同样报错。尚未重建全部早期 shader，因此首次生成超限二进制的
确切提交未以历史运行确认；不将发现时间误作引入时间。

## 干净修复

- Glass pass 的四个只读输入改为 `Texture2D<float/uint>`，继续用整数坐标
  texel load；不加 sampler，不改变滤波、坐标、格式或计算。图像原本就有 SAMPLED
  usage，resource tracker 根据反射绑定继续管理 layout/barrier。Storage 数量变为 6。
- Device 持有实际 physical-device limits；`PipelineLayout::new` 在创建普通及
  合并 graphics layout 前统一检查 storage / sampled image 的 per-stage 和
  跨所有 sets 总预算。数组按元素计数，多 stage 绑定在总量中只计一次。
  不支持的布局在进入 Vulkan 前失败，并报告实际需求和限额；不禁用 validation，
  也不静默关闭 Glass。此检查不宣称覆盖 Vulkan 的所有其他设备限制。
- 纯逻辑 seam 覆盖限额边界、只读 storage、texel/array、独立及共享 stage、
  跨 set 同 binding 编号、总量与大整数计数；真实预编译的 reflection/optimized
  SPIR-V 都检查 Glass 的 descriptor 合同。已有资源绑定 interface 不变。

### 验证（2026-10-16）

- `cargo fmt --check`、`cargo check`、`cargo test`（1603 app + 4 collision，7 ignored）、
  `cargo test -p re-flora-vkn --lib`（65 passed）、`cargo build --release` 通过。
- 普通 hidden/mute 0.5 s 和原始 rooftop 复现均无致命日志；检查了真实 run log。
- Glass 固定场景覆盖 0 / 25 / 50%：实际 0 / 23.474 / 45.467%，GPU scene-query、
  transport、raster screen hit、前景保留断言通过；exhaustion 和 nonfinite 都为 0。
- 原始 rooftop Soil stability：38 holding / 0 missing / 4096 added，守恒与库存
  断言通过，**现在无 Vulkan validation error**。
- 没有配置或生成文件变化。这里只验证正确性，不作性能提升或跨平台视觉等价声明。

证据：`target/priority-optimization/layout-{baseline,guard-red,test-red,green}.log`、
`glass-coverage-{0,25,50}.log`、`layout-soil-stability.log`、`layout-run-log.log`。
