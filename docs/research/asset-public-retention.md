# 资产公开保留与用途核查（2026-10-23）

基线：`20e1f3de72d95cba9685f211407798d0ad2008a9`。本次只补用途、来源、许可和台账，
不搬文件、不重生成声音/图片、不修改运行时。法律事实与工程建议分开；不是律师出具的授权证书。
可随包保存的是权利人的许可证原文、署名和可验证来源，不能自行替第三方生成授权证明。

## 结论

| 资产 | 公开保留判断 | 本次依据/动作 |
| --- | --- | --- |
| Pixelify Sans | 保留 | 本地字体内声明 OFL 1.1，之前已补版权/许可证 |
| 自有模型、v5 制作源、模型配方/贴纸 | 保留 | 项目制作链可追溯；所有者已确认蝴蝶实验归属；非运行输入不构成拆仓理由 |
| EA FAST 128 PNG | 保留，附上游 BSD 通知 | 已有工具/生成命令来源，补固定审计版本、作者、完整 BSD-3-Clause 和哈希 |
| ARI NH172 两份 HRTF | 保留，数据遵循 CC BY-SA 3.0 | 精确找回原始 SOFA；其 License 属性明确；重建与当前数据逐字节相同；两目录补署名、许可和转换记录 |
| 两篇 JCGT PDF | 保留未修改 PDF | 本地末页明确 CC BY-ND 3.0；补完整许可；不与 CESCG PDF 混为一谈 |
| NVIDIA STBN 256 PNG | 来源已确认，但公开原件/商业使用边界尚待澄清 | 与官方包逐字节相同；保存完整两部分条款；不武断标为必须私有或自由商用 |
| 脚步、7 个备用农具图、太阳 | 仍需替代/分仓方案 | 已知原件公开分发限制；不能靠补作者署名消除 |
| Sound Armoury、蝉鸣、8 主图标、风声参考、宣传图 | 待用户来源/购买信息 | 不因为第三方身份自动拆；也不凭文件名补出不存在的授权 |
| CESCG PDF | 转载许可待确认 | 作者/来源已知，但未取得同等 CC 授权；可考虑只留官方链接 |
| 项目截图、研究图表和 OBJ 样本 | 保留文档/制作资料 | 不当作游戏运行资产；包含外部内容的具体权利仍按原来源 |

完整逐组用途见 [台账](../assets/inventory.md)，每一组新增 `purpose`，与加载阶段 `usage` 分开。

## HRTF：从猜数据库许可改为读取实际文件

官方文件：
https://sofacoustics.org/data/database/ari%20%28artificial%29/hrtf%20b_nh172.sofa

官方目录：
https://sofacoustics.org/data/database/ari%20%28artificial%29/

本次下载 SHA-256 `22c79a11c384247f59e1c3b76e5f05a5eaf6b4a6632a325b7796163820dbe416`，
与 `git show 7610c5a2^:assets/hrtf/hrtf_b_nh172.sofa` 完全一致。
用 h5py 直接读取属性，不靠搜索摘要套用其它 ARI 版本：

- `License = Creative Commons Attribution-ShareAlike 3.0 Unported License`
- `Organization = Acoustics Research Institute, Austrian Academy of Sciences`
- `DatabaseName = ARI (ARTIFICIAL)`；`ListenerShortName = nh172`
- `ListenerDescription = Neumann KU 100`
- IR 为 1550×2×256，48 kHz，所有 Delay 为零。

按 float32 IR、SOFA 球面方向到 listener-local 方向、native little-endian 格式独立重建，
**整个** native 表与两份本地文件逐字节相同。
转换不是新增录音或重新授权：保留机构署名、官方源链接、修改说明、CC BY-SA 3.0。
ShareAlike 针对该数据的适用义务不能被代码 MIT / 项目 CC BY-NC-SA 覆盖。

- [自包含署名与转换配方](../../assets/hrtf/README.md)
- [验证记录](../../assets/hrtf/provenance.json)
- [官方完整条款](https://creativecommons.org/licenses/by-sa/3.0/legalcode.txt)

ARI 主站页面本次返回 403；不据此猜许可。SONICOM 的一般 ARI/人工头目录条目
也不代替实际旧文件的 License 属性。

## FAST：保留，不把“数值纹理”与“必须私有”画等号

EA/SEED 上游代码和随库资料使用 BSD-3-Clause；固定审计版本
`2cf53e4bb510d07511fe63a312556d2a2e108c70`：

- [README（含生成命令、随机种子参数、随库纹理和作者）](https://github.com/electronicarts/fastnoise/blob/2cf53e4bb510d07511fe63a312556d2a2e108c70/README.md)
- [LICENSE.txt](https://raw.githubusercontent.com/electronicarts/fastnoise/2cf53e4bb510d07511fe63a312556d2a2e108c70/LICENSE.txt)

项目原 README 已说明这些文件由该工具生成，并记录两条命令。补齐 BSD 通知后保留。
没有发现单独的输出限制；不把工具 BSD 许可泛化为“任何输出都自动 BSD”。历史实际
生成器版本/seed 未记录，不能声称已重现原始生成过程。没有为此重生成或改变纹理。

## STBN：不要只读许可证的一半

固定上游：`NVIDIA-RTX/STBN@48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b`，
旧 `NVIDIAGameWorks/SpatiotemporalBlueNoiseSDK` 链接重定向至此。

- [官方预生成包](https://github.com/NVIDIA-RTX/STBN/blob/48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b/Assets/STBN.zip)
- [完整许可](https://github.com/NVIDIA-RTX/STBN/blob/48b2839e4d8b7f0202ac72c6b0ae720d235a5b8b/License.txt)
- [加入商业/非商业两部分许可的提交说明](https://github.com/NVIDIA-RTX/STBN/commit/ba4da3b54298889d187c0d621fd96b34e7567d18)

本地 256 PNG 均与 ZIP 中同名条目字节相同；ZIP 没有额外的纹理专属许可文件。
现有许可第一部分的 §3.3 限制研究/评估用途；第二部分明确标为 Commercial Use License，
§1(c) 涉及作为应用 object code 一部分的分发，§2 有附加条件，§4(b) 限制 standalone 分发。
因此：

- “整个 STBN 只有非商用许可”不完整，忽略了商业部分和提交说明。
- “噪声是数学数据，所以下载的 PNG 自然是 CC0”没有一手许可支持。
- “移到私有仓库就可以商用”也不成立；存储方式不替代使用授权。

需向上游确认这些 PNG/生成输出是否有独立授权，以及公开原件与商业游戏能否同时分发。
上游条款给出的邮箱：`nvidia-rtx-license-questions@nvidia.com`。本次没有发邮件或代表用户接受协议。

建议问题：

> May the pregenerated PNG textures in Assets/STBN.zip be redistributed as raw
> files in a public GPL-3.0 game source repository and bundled with a commercial
> game? Are generated texture outputs covered by separate terms from the SDK
> source, and which license/attribution must accompany them?

## 用途核查的重要细分

源码 `src/tracer/resources.rs` 会加载全部六组 64 切片纹理；并不说明全部都实际采样。

| 数据组 | 当前实际用途证据 |
| --- | --- |
| STBN scalar | `tracer_shadowing.slang` 阴影 PCF/PCSS 采样旋转；`god_ray.slang` 步进抖动 |
| STBN unitvec2 | `tracer_shadowing.slang` 阴影 UV jitter |
| FAST weighted_cosine | `tracer.slang` 漫反射反弹方向与太阳盘采样 |
| STBN unitvec3、STBN cosine、FAST unit_vec3 | 仍加载/声明，当前源审阅未找到实际采样调用 |

这揭示了后续可能精简的方向，但删除这些 bank 需要同步资源创建/绑定与验证，
不是本次许可文档更新顺带删除的内容。

## JCGT：已有许可，不能继续笼统标为三篇都未知

本地 2019 PDF 第 30 页、2021 PDF 第 29 页均明确列出作者版权和 CC BY-ND 3.0。
两份原样 PDF 可以按该许可署名再分发；保留 [原作者、正式来源和哈希](../references/ddgi/README.md)，
并补 [完整条款](https://creativecommons.org/licenses/by-nd/3.0/legalcode.txt)。
独立阅读笔记不是全文转换。CESCG 2022 PDF 没有查到同等授权，台账单独列组。

## 验证范围

核对二进制哈希、SOFA 元数据及完整 native 重建、STBN 256 个文件匹配，且补充用途字段的
台账生成器保留旧 catalog 无 purpose 字段的兼容行为。本次没有改运行数据或渲染代码，
不把文档核查当成性能测试或版权机关出具的证书。许可证快照只规范化换行和行末空白，
正文不变，文件来源与校验和写在对应目录中。

提交前验证：7 个台账 Python 测试、Ruff、Pyright 通过；586 个既有非说明类
运行/媒体/制作数据 SHA-256 全部未变，FAST 128 与 STBN 256 PNG 校验清单通过。
`cargo build --release` 成功（仅既有警告）。未改 Rust/shader 或资源字节，本次未重新运行 GPU smoke。
