# Debug 设置：新增控件默认可保存

## 接入规则

普通 float/int/uint/bool/choice/string/color 设置优先声明在 `config/gui.toml` 的 section/param 中。
现有构建生成器会生成类型化访问字段，统一 GUI 和统一保存遍历同一份声明。不要另加 App 临时副本，也不要为某个参数新增 save 分支。

需要自定义布局或强类型设置时：

1. 在 `src/app/gui_config_model.rs` 的 `SavedCustomSettings` 内声明字段或设置结构，提供 serde 和兼容旧文件的默认值。
2. 在统一 DebugSettings 绘制入口接入自定义编辑器，编辑器只接收 `SavedControls`，不接收裸 `egui::Ui` 或 App 可变字段。
3. 通过非捕获 selector 绑定存储字段，例如：

```rust
ui.slider(
    |s| &mut s.butterfly_flight.tuning.height_above_ground,
    0.03..=0.24,
    "Flight height above ground",
    0.005,
    false,
);
```

到此即可，不需要新增保存／加载复制逻辑。Save 序列化的就是自定义控件正在修改的对象。
checkbox 用 `toggle`，支持 bool 或枚举两态。其他基础类型优先用上述通用声明路径；需要扩展自定义编辑类型时，在 `SavedControls` 内增加同样绑定存储 selector 的方法，不暴露任意 `&mut T` 或裸 UI。

真正临时的实验只能显式选择不保存，并说明理由：

```rust
settings.draw(ui, |section, temporary| {
    if section == "Wind" {
        temporary.not_saved("Wind prototype experiment", |ui| prototype.controls(ui));
    }
});
```

理由会显示给玩家，空理由会失败。不要把用户希望保留的选项放在这个逃生入口。持久化字段禁止为了绕开约束而添加 `serde(skip)`；例如树的运行时 `growth_age` 是既有明确排除项，不是新增可保存滑杆的先例。

## 为什么不会再漏一个 save 接口

- 树／蝴蝶原本各有 live 副本，Save 时手工复制；这些副本和复制代码已删除。
- `SavedCustomSettings` 在 TOML 中 flatten，保留原 `[tree]`、`[butterfly_flight]` 文件结构；`Deref` 保持现有运行代码读写路径，但实际指向同一份对象。
- `SavedControls` 的 selector 是 `for<'a> fn(&'a mut SavedCustomSettings) -> &'a mut T`。捕获 App 临时变量的闭包不能转换成它；控件也拿不到裸 UI。
- 原任意 `extra_controls(&mut egui::Ui)` 入口改成显式 `TemporaryControls`；底层 raw renderer 改为私有。
- 通用参数保留由声明驱动的统一同步，不是为新增字段手写映射；没有增加第二套生成器。

## 自动检查

`cargo test` 自动运行：

- `every_declared_generic_setting_saves_its_live_value`：遍历全部通用参数，修改 live 值后实际保存、加载并比较。
- `every_serialized_custom_leaf_survives_live_edit_save_reload`：遍历自定义设置的全部序列化叶子，逐项修改 live 对象并 round-trip；新增叶子自动加入。新增枚举等无法自动选合法替代值的类型会明确失败，要求补测试取值策略，而非默默跳过。
- `newly_declared_slider_needs_no_save_hook`：测试构建额外声明一个新字段，用实际 egui 鼠标事件调整新滑杆，无保存接线仍能保存并重载。
- 临时标签、旧文件兼容、原子保存、失败提示、树与蝴蝶 UI 回归仍保留。

覆盖范围是已声明／序列化的设置，不声称能检测有人故意在其它任意 UI 中绕过统一入口。旧树编辑器仍是模块内部可信适配器，直接编辑已保存的树对象；Environment Probes 是既有独立临时实验，现已明确显示不保存。本次没有把整个应用所有 UI 都改造成受限界面。

## 2026-09-13 验证

实现提交 `0409f89e`，worktree `re-flora-agent-butterfly-block-flight`。

- `cargo fmt --check`、`cargo check` 通过。
- `cargo test`：**957 + 4 通过，2 ignored**；完整测试日志 `target/gui-persistence-all-tests.log`。
- 曾故意接入捕获 App 值的 selector，`cargo check` 按预期产生 E0308：expected fn pointer, found closure。探针已移除，证据 `target/gui-persistence-forbidden-binding.log`；这项负向编译探针为本次手动验证，非持续运行的独立 compile-fail 测试。
- 持锁 `env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5` 通过。日志 `target/re-flora-logs/re-flora-20260913-021902.508-594729.log`，正常退出、`failures=0`，无 ERROR/panic/VUID；原有 6 项 release 编译 warning 未改。
- 用户 `config/gui.toml` 零差异；B 默认与 5.5 Hz、0.35 自主速度、上下强度 2、离地高度 0.08 均保留。生成文件、shader、ABI、飞行模型无变化。
- 本次验证是持久化／UI 输入正确性和真实启动 smoke，不是新的动态美观或性能验收。未自动启动可见游戏、未 merge/push 或管理其他 Worker。
