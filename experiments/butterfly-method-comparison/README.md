# 蝴蝶 Blender → Sprite 实验

## 当前进展

已把仓库外的两版实验收进当前 worktree，作为后续迭代基线；没有修改游戏代码或正式素材。

原始来源：`/home/terence/Documents/Codex/2026-09-06/realtime-voice-chat-3/outputs/butterfly-method-comparison/`，以及其同级 `butterfly-art-workflow.html`。原目录保留备份。除 Python 缓存和 Blender `.blend1` 自动备份外，保留原始交付；HTML 暂留作可用的对照入口，不是后续必须维护的交付规范。后续进展以本 Markdown 为入口。

### 已有成果与限制

- `blender/`：v1 可编辑 `.blend`、同源动画 GLB、导出脚本、五方向×五帧图集、转台视频及原始渲染。
- `blender-v2/`：连续双叶翼形、身体升沉/俯仰、腹段跟随，直接渲染 16px/32px；保留诊断对照与独立重放结果。
- 原手绘参考、AI 生图两轮失败证据及相关记录一并保留，避免把已试过的路线误当作未探索。
- 迁入后已在独立副本完成全量复跑：75 帧重放一致，源 `.blend` 和四张 16/32px 索引图集与归档逐字节相同。详见 [接手复验与低分辨率诊断](resolution-study/README.md)。
- v2 仍未通过最终美术验收：最薄姿态仅 3px 高，腹段跟随在多数 16px 帧中不可辨，5fps 节奏离散。未批准替换原画。

优先阅读：

1. [v2 艺术检查与限制](blender-v2/QUALITY_REVIEW.md)
2. [v2 运行方法](blender-v2/README.md)
3. [关键姿态](blender-v2/KEY_POSES.md)
4. [已有一手制作案例研究](blender-v2/research-3d-to-pixel.md)
5. [历史浏览器验收](browser-validation-v2.md)

## 查看与复现

从仓库根目录启动静态服务器：

```sh
python3 -m http.server 8788 --bind 127.0.0.1 --directory experiments/butterfly-method-comparison
```

访问 `http://127.0.0.1:8788/comparison-v2.html` 查看原手绘/v1/v2；`index.html` 是上一轮 AI/Blender 对照。页面使用随附的 model-viewer，许可证在 `blender/MODEL-VIEWER-LICENSE.txt`。

保留手动编辑的模型并重新导出：

```sh
BLENDER=/path/to/blender /usr/bin/python3 experiments/butterfly-method-comparison/blender-v2/run.py --export-existing
```

依赖 Blender（原验证版本 4.5.13 LTS）、Python + Pillow、ffmpeg。**不带 `--export-existing` 会按脚本重建并覆盖 `.blend`**；`--draft` 只适合快速检查，不代表完整验证。脚本会更新本实验目录的派生输出；提交前检查 diff。

`analyze-reference-v2.py` 已改为读取随附的原手绘快照，不再依赖主 worktree 的绝对路径。历史文档、日志和 JSON 中的旧路径、服务器地址与验证结论保留其历史含义，不能当作当前操作说明。其他原始文件没有改写。

## 接手任务进度

用户目标：游戏中当前蝴蝶虽然像素化，仍感觉分辨率偏高；希望接手三维动画转多方向序列帧流程，降低后续修改动作和输出分辨率的成本，并了解其他像素游戏的手绘、三维预渲染及混合流程。

1. 已检查游戏代码：16px、5fps、取第 1/3 行；不同运动模式的显示尺寸路径不同。屏幕实际显示大小仍待游戏内验收。
2. 已完整复跑保存的 v2 模型，未重建或覆盖归档源文件。
3. 已完成[一手案例复核](../../docs/research/butterfly_blender_pipeline_followup.md)：Dead Cells 的三维预渲染、Celeste 作者的手工像素设计，以及三维参考后重画的混合流程。不能把约 50px 角色的成功直接外推到 8–16px 蝴蝶，也尚未证明本项目的制作成本一定更低。
4. 已新增隔离诊断脚本，实际渲染 16/12/8px 并生成同显示尺寸的逐帧图和 GIF。8px 出现触边且轮廓损失明显；12px 值得进一步专门设计，不能仅靠降低分辨率解决 v2 姿态问题。
5. 用户确认视觉方向后再决定游戏接入、批量导出接口及性能验证。若做游戏内 A/B，必须使用 Debug 面板运行时复选框保留原模式。

讨论留待技术复验和案例调研汇总后，带实际候选一起进行；不以提问阻塞可自主完成的检查。
