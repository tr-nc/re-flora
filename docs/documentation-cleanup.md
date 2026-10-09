# 文档清理记录与后续盘点

## 范围与判断原则

本轮基于 `adde77ac`（2026-10-06）的仓库状态，在本地 `docs` 分支整理。
目标是为下一篇 Reddit 近半年开发回顾减少噪声，不是重写所有技术文档。

已盘点 `docs/` 文件及文档标题／状态声明，并对下面的删除项逐份阅读、核对提交历史及仓库引用。
这是一轮保守清理，不代表所有保留文档都已逐段验证。

- 删除已结束的一次性合并、分支选择和工作区操作流水账；历史仍在 Git 中。
- 保留玩家／开发／发布指南、产品方向、架构契约、兼容格式和仍有价值的研究／失败证据。
- 不把“旧”“没有引用”“文件名含 plan/progress”单独当作删除依据。
- 不删除研究附件、截图、PDF、音频及 HTML 试验台；需要先确认消费者和复现价值。

## 本轮删除表

以下路径相对 `docs/`。可用 `git show adde77ac:docs/<文件名>` 恢复原文。

| 已删除文档 | 判断依据 | 保留的知识入口 |
| --- | --- | --- |
| `curated-dev-selection.md` | 已完成的 dev 分支选择记录；其 GLSL 回退路径已由 `003e535d` 删除，不再是当前构建方式 | `slang-migration-roadmap.md`、`slang-validation-plan.md`、Git 历史 |
| `premerge_cleanup.md` | 已完成的 DDGI 工作区／分支／标签操作记录，含旧机器路径和当时的分支库存，不是当前操作指南 | `digging_regression.md`、DDGI 研究；两处证据归档引用改指固定历史版本 |
| `butterfly_main_merge_validation.md` | 2026-09-12 的 main → 蝴蝶 Worker 合并验收；当时的设置与计数不能代表当前构建 | 蝴蝶功能／研究文档、`fallen_leaf_flight_validation.md`、`fruit_ground_jitter.md` |
| `butterfly_cicada_merge_validation.md` | 2026-09-13 的蝉鸣 Worker 合并与移除流水账，操作已完成 | `ecology_spawning.md`、蝉鸣与音频专项文档 |
| `butterfly_main_integration.md` | 一次 main → Butterfly Blend 的同步记录；随后 Blend 已合入 main | Git 历史、蝴蝶专项研究与验证 |
| `butterfly_blend_main_integration.md` | 2026-09-21 的固定版本合并已由 main 提交 `fe1b3ccd` 完成 | Git 历史、蝴蝶专项文档、`god_ray_full_resolution.md` |
| `terrain_material_integration.md` | 2026-09-20 的合并已由 main 提交 `49a61c61` 完成，仅记冲突解决与当次验收 | `terrain_materials.md`、`terrain_material_per_voxel.md` |

## 修正而非删除

| 文档 | 本轮处理 |
| --- | --- |
| `ddgi_history_candidate.md`、`ddgi_history_reuse_implementation.md` | 保留实验限制和归档来源；将已删除清理文档的引用改为 `adde77ac` 固定版本链接 |
| `slang-migration-roadmap.md`、`slang-validation-plan.md` | 保留当前原生 Slang 构建契约；移除过时的 76-entry／133-file 固定总数，改以 manifest 和检查器输出为准 |

## 暂留／下一轮需核实

| 文档或类别 | 暂留理由／下一步 |
| --- | --- |
| `agents/`、`playing.md`、`development.md`、`packaging.md` | 当前操作入口；不因旧日期删除 |
| `game_direction.md`、`first_garden_moment.md`、`steam_direction.md` | 区分目标与已实现功能；发帖不能把愿景写成完成项 |
| `terrain_persistence_v1.md` | 明确标为历史地形格式，但旧存档兼容仍有价值；当前入口是 `garden_snapshots.md` |
| `gui_settings_architecture_plan.md` | 大型实施计划可能与 `agents/gui-settings.md` 重叠；逐项核对未落地约束后再提炼／删除 |
| `non_gameplay_refactor_progress.md` | 2026-06-01 的文件大小与 in-progress 状态已不适合作为当前状态；仍含未关闭事项，需对照代码和任务记录，不在本轮盲删 |
| `procedural_tree_rustle_progress.md`、`terrain_smoothing_gpu_progress.md`、`whole_tree_rasterization_progress.md` | 需要分别确认当前契约、人工验收和测量边界，再将流水账提炼掉 |
| DDGI／光照／风响应候选与旧 A/B 验证 | 部分模式已退役，但失败证据和回归限制仍有价值；后续拆分“当前行为”和“历史证据” |
| `performance/`、`evidence/`、`research/`、`references/` | 包含历史性能、来源、复现材料与工具附件；先追踪引用及自动化消费者，不整目录清空 |
| `discussions/`、屋顶与攀援实验 | 设计讨论或 opt-in 实验不等于现有普通玩法；保留并在宣传素材中明确身份 |

## 本轮验证

- `git diff HEAD --check` 通过；6 份新增／修改文档中的 6 个本地 Markdown 链接目标均存在。
- `cargo build --release` 通过，保留既有 `collision_bench_rapier` unused import warning。
- 额外运行 `python3 scripts/check_shader_manifest.py` **未通过**：报告 `shader/slang/apple_pixel.frag.slang` 不是共享 module。本轮未修改脚本或 shader，这是当前基线的检查器／manifest 问题，不借文档清理改动渲染代码；不声称 shader 验收全部通过。
- 仅改文档，没有运行 GPU smoke 或可见游戏；生成文件与用户配置无变化。

## 发帖素材入口

见 [Reddit 近半年开发回顾素材](reddit-half-year-update.md)。素材按玩家能看到的变化组织，
未核实发布包范围、未测量的性能提升和未完成的玩法闭环不得写成已交付承诺。
