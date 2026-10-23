# 蝴蝶制作源与当前预览入口

**唯一预览工具：[模型像素化预览台 · 蝴蝶](../model-preview/?model=butterfly)**。
本目录首页只做跳转；游戏和网页共用正式资源
[`assets/models/butterfly.glb`](../../assets/models/butterfly.glb)。

## 保留的内容

- `blender-v5/`：当前批准的双翼模型制作源、导出/验证脚本与离线验证图集。
  `run.py` 使用同目录 `palette.py`，不再依赖已退役的 v2。
- `vendor/three-0.183.0/`：当前模型预览及土袋发布脚本仍在使用的 Three.js，
  其 MIT 许可和来源说明继续随目录保留；它不是可删除的旧实验。
- `index.html`、`validate-debug-preview.cjs`：当前工具的兼容入口。

```sh
node scripts/serve-model-preview.mjs
# http://127.0.0.1:8765/model-preview/?model=butterfly
python3 experiments/butterfly-method-comparison/blender-v5/run.py --help
```

正式 GLB 的几何和节点动画由游戏和网页直接加载，不使用离线方向图集。
制作变更先在 v5 审阅，再有意发布到正式模型目录；详见
[共享模型约定](../../assets/models/README.md)。

## 已删除的实验

2026-10-23 按项目所有者要求删除 Blender v1–v4、失败的 AI 候选、
旧分辨率研究、旧制作说明网页及相关输出/脚本。
所有者确认这些由助手发起的蝴蝶实验属于项目自有内容；
此次删除是因为已无维护价值，**不是以版权疑虑为理由**。
第三方 Three.js 仍遵循自身许可。

历史资料可以从 Git 提交 `1067833b` 查看；没有重写 Git 历史。
具体范围与验证见 [清理记录](../../docs/assets/cleanup-unused.md)。
其它研究文档中的历史路径和验收结论不代表当前仍保留旧实验。
