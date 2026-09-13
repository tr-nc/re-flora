# 夏日蝉鸣合入蝴蝶 feature

2026-09-13，目标仅 `agent/butterfly-block-flight`，不更新 main、不 push。

## 提交与语义合并

- `64e17578`：经用户授权提交 GUI 调整及相机快照清空。
- `1eb119fd`：普通 merge，完整合入夏日蝉鸣 Worker 固定 HEAD `a31364beef94c89ea7f322ab21259d316ee34d1b`。
- `30120bc8`：独立验证夹具修复，使 opt-in 生态场景自带近景观察点，不再依赖用户的 `bright-pix` 快照；玩家正常启动相机不变。

按 Dispatch 与 resolving-merge-conflicts 流程核对两边意图：共享生态调度器接管草、特殊植物、树冠的出生机会／位置；现有 ButterflyEmitter 接收 `spawn_at`，继续拥有个体和飞行。删除旧来源列表与出生时钟，没有并行生成器。共享调度接入当前已保存的 ButterflyFlightVariant/Tuning，而不是退回旧版默认。

保留当前自然配色权重、动画 2.1 倍透明面积补偿、运行时外观 checkbox、风响应与共享节奏、地表相对高度、树枝裁剪 A/B、叶片角度光学／局部抖动以及当前存档菜单。旧出生时钟测试改为共享机会／全局及局部容量测试；外观切换、运动独立性、面积补偿和颜色测试继续通过。

冲突涉及生态接口、emitters、植被旧物种清理、leaf flutter shader 字段及生成 GPU structs。最终 `cargo check` 从源码再生成：GPU structs 相对目标 HEAD 无变化；GUI 生成文件只有出生率标签变化。ParticleInstanceGpu 仍为 52 字节，leaf_optics offset 36 的回归通过。

## 验证

- `cargo fmt --check`、`cargo check`：通过，保留已有编译 warnings。
- `cargo test`：961 主测试通过、2 ignored；另 4 库测试通过。音频测试有设备探测 ALSA 提示，但没有测试失败。
- `cargo test -p re-flora-physics`：44 测试通过。
- 以下 release app 命令均以 `flock --close /tmp/re-flora-summer-gpu.lock` 串行执行，hidden/mute，无自动可见启动；日志检查无 ERROR/panic/update failed，shutdown failures=0。

```sh
RE_FLORA_ECOLOGY_SCENE=canopy RE_FLORA_ECOLOGY_SMOKE=1 RE_FLORA_BUTTERFLY_REVIEW=switch flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 110
RE_FLORA_VEGETATION_RESPONSE_VALIDATE=1 flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 8
RE_FLORA_ECOLOGY_SCENE=dense flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 30
```

实际日志均在本 worktree 的 `target/re-flora-logs/`：

| 日志 | 证据 |
| --- | --- |
| `re-flora-20260913-183605.220-40327.log` | 树冠来源有蝴蝶和蝉鸣；两次世界替换，第二次 old_butterflies=2、retained=0；删除鸣叫宿主通过；清空后 20 秒无新出生 passed；frame 2106/2226 方块／动画切换 |
| `re-flora-20260913-183757.246-42691.log` | GPU 植被快速反向、held pose、lifetime remap 通过；叶片 peak angle 0.35478、quiet angle 0.00000004、held bounds passed |
| `re-flora-20260913-183807.207-42773.log` | 75,398 株草、8 个附近区域；蝴蝶和蝉鸣均从真实草位置出生 |

首轮未修复夹具时默认远景相机不在生态范围内（groups=0），虽然正常退出但未触发生命周期，明确不计为验收；修复后的上述 passed 标记才是有效证据。听感、最终主观外观和超大世界性能未验收；静音运行不是耳机听感证据。

## Worker 移除与恢复

验证完成后再次确认 summer-cicadas / wQ idle、工作区干净、HEAD 未前进且已被当前目标包含；执行 `herdr worktree remove --workspace wQ`（非 force）及 `git branch -d agent/summer-cicadas`，成功移除。其他 Worker 保持原样。

删除前归档全部非 debug/release 构建目录的 target 证据（包括存档、日志、脚本、交付报告）：

`/home/terence/code/worker-archives/summer-cicadas-a31364be-20260913-evidence.tar.gz`

SHA-256：`d83a201e0238e26f8a420b6079423491345bbebc83273bbbd2a031752d81fecd`。约 32 MiB，已验证归档可列举。源代码可从 merge 第二父提交恢复；构建缓存未归档、可重建。测试运行没有改变已提交的用户 GUI 和相机配置。
