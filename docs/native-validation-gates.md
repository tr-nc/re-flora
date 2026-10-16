# 原生验证脚本：日志必须能使验收失败

## 影响范围

六个铺路／石块验证命令的错误日志守卫可能假绿：
`validate-pavement{,-prototype-native,-pointer,-continuity,-recovery}.sh` 和
`validate-stone-placement.sh`。即使应用退出码为 0、行为标记齐全，只要日志含
ERROR、panic、VUID 或 DEVICE_LOST，命令也必须失败。普通启动意外触发铺路或
石块预览的两个负向守卫同样受影响。正向标记缺失和非零进程退出原本即可被拒绝。

## 根因

Bash 的 `errexit` 不对被 `!` 反转状态的命令生效。原来的 `set -e` 加
`! grep ...` 不是断言：命中错误时该行返回 1，但脚本继续执行到成功输出，最终
返回 0。真实六个命令的隔离测试注入错误及意外启动工作后，修复前有 26 个负例
错误通过；并非根据源码形状推测。

## 引入时间

`validate-pavement.sh` 的该行可追溯至 `b12e36fe`（2026-10-11）。其余脚本的
首次引入时间未逐个追溯；本次修复前六个入口都已复现。此前已保存的铺路原生日志
另行独立扫描未发现上述错误，这不改变脚本本身存在假绿缺口的结论。

## 干净修复

- 六个命令直接调用已有 `runtime_log_diagnostics.py`，复用唯一的致命诊断分类
  module，不另建一套正则。其非零退出触发原有 shell 失败路径；缺失、不可读或
  非 UTF-8 日志也会失败关闭。
- 分类器保留原有精确 Wayland cosmetic timeout 例外，并识别 `DEVICE_LOST`
  下划线写法，保持旧守卫的覆盖面。
- 意外启动工作使用显式条件与 `exit 1`。行为标记、成功退出标记、进程退出码和
  配置校验继续是独立必要条件。
- `scripts/tests/test_native_validation_commands.py` 通过实际六个 shell 入口测试，
  仅用确定性 app/cargo adapter 替代昂贵的 GPU 与构建。覆盖正常成功、错误日志
  加成功标记、缺失日志、不完整运行、进程失败和意外启动工作。

验证命令：

```sh
python3 -m unittest scripts.tests.test_native_validation_commands scripts.tests.test_runtime_log_diagnostics
```

这些是验收机制测试，不是玩法或 GPU 正确性证据。真实原生脚本仍需运行并检查
各自的日志；禁止以关闭 validation 或扩大忽略规则来换取绿色结果。
