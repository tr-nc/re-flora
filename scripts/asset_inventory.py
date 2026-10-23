#!/usr/bin/env python3
"""Generate a tracked-file asset ledger; qualitative judgments stay in catalog.json."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import re
import subprocess
from collections import Counter
from fnmatch import fnmatchcase
from pathlib import Path
from typing import TypedDict
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
CATALOG = Path("docs/assets/catalog.json")
MEDIA_EXTENSIONS = frozenset(
    [
        ".png",
        ".jpg",
        ".jpeg",
        ".webp",
        ".gif",
        ".bmp",
        ".svg",
        ".ico",
        ".tga",
        ".exr",
        ".hdr",
        ".ktx",
        ".dds",
        ".wav",
        ".ogg",
        ".mp3",
        ".flac",
        ".aiff",
        ".opus",
        ".mp4",
        ".webm",
        ".mov",
        ".glb",
        ".gltf",
        ".obj",
        ".fbx",
        ".blend",
        ".aseprite",
        ".ase",
        ".psd",
        ".ttf",
        ".otf",
        ".woff",
        ".woff2",
        ".petalhrtf",
        ".sofa",
        ".pdf",
    ]
)
EMBEDDED_MEDIA = re.compile(
    rb"data:(?:image/|font/|audio/|video/|application/(?:font|x-font))", re.IGNORECASE
)
FIELDS = [
    "path",
    "kind",
    "bytes",
    "sha256",
    "group",
    "usage",
    "decision",
    "source",
    "gap",
    "evidence",
    "packaged_loose",
]


class Group(TypedDict):
    id: str
    name: str
    patterns: list[str]
    exclude: list[str]
    usage: str
    decision: str
    source: str
    gap: str
    evidence: list[str]


class Catalog(TypedDict):
    reviewed_revision: str
    reviewed_on: str
    groups: list[Group]


def tracked_paths(root: Path) -> list[str]:
    output = subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
    return sorted(set(output.decode("utf-8").strip("\0").split("\0")) - {""})


def asset_kind(root: Path, path: str) -> str | None:
    suffix = Path(path).suffix.lower()
    if suffix in MEDIA_EXTENSIONS:
        return suffix[1:]
    if path.startswith("assets/"):
        if suffix in {".md", ".txt"} or Path(path).name in {
            "SHA256SUMS",
            "provenance.json",
        }:
            return "bundle-note"
        return "asset-source/data"
    if suffix == ".html" and EMBEDDED_MEDIA.search((root / path).read_bytes()):
        return "embedded-media-container"
    return None


def matching_group(path: str, groups: list[Group]) -> Group | None:
    matches = [
        group
        for group in groups
        if any(fnmatchcase(path, pattern) for pattern in group["patterns"])
        and not any(fnmatchcase(path, pattern) for pattern in group["exclude"])
    ]
    if len(matches) > 1:
        raise ValueError(
            f"Overlapping asset groups for {path}: {[g['id'] for g in matches]}"
        )
    return matches[0] if matches else None


def inventory(
    root: Path, paths: list[str], groups: list[Group]
) -> list[dict[str, str]]:
    rows = []
    for path in sorted(set(paths)):
        kind = asset_kind(root, path)
        if kind is None:
            continue
        group = matching_group(path, groups)
        data = (root / path).read_bytes()
        usage = group["usage"] if group else "待审计"
        if kind == "bundle-note":
            usage = "随包来源/许可说明，不加载"
        elif Path(path).suffix == ".mjs" and path.startswith("assets/models/"):
            usage = "构建校验 / 模型制作源"
        rows.append(
            {
                "path": path,
                "kind": kind,
                "bytes": str(len(data)),
                "sha256": hashlib.sha256(data).hexdigest(),
                "group": group["id"] if group else "UNREVIEWED",
                "usage": usage,
                "decision": group["decision"] if group else "待审计，勿自动删除",
                "source": group["source"] if group else "未记录",
                "gap": group["gap"] if group else "补分类、引用证据和来源/许可",
                "evidence": "; ".join(group["evidence"]) if group else "",
                # Reviewed packaging policy, NOT a dependency/use inference.
                "packaged_loose": "yes"
                if path.startswith("assets/") or path == "demo/img/splash.png"
                else "no",
            }
        )
    return rows


def csv_text(rows: list[dict[str, str]]) -> str:
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=FIELDS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return output.getvalue()


def cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def report(rows: list[dict[str, str]], catalog: Catalog) -> str:
    total = sum(int(row["bytes"]) for row in rows)
    asset_rows = [row for row in rows if row["path"].startswith("assets/")]
    lines = [
        "# 资产包总表（生成文件）",
        "",
        "维护方式与统计口径见 [README](README.md)。逐文件明细见 [inventory.csv](inventory.csv)。",
        f"引用/许可人工审阅基线：`{catalog['reviewed_revision']}`；审阅日期：{catalog['reviewed_on']}。",
        "文件数量、字节数与哈希来自当前 Git 跟踪文件的工作树内容，不代表自动重新审核过代码或许可。",
        "",
        f"共 **{len(rows)}** 个登记文件/容器，**{total / 1024**2:.3f} MiB**；",
        f"其中 `assets/` **{len(asset_rows)}** 个，外部 **{len(rows) - len(asset_rows)}** 个。",
        "随包说明也计文件；HTML 按容器计一个，不展开内嵌媒体；未按哈希去重。",
        "",
        "## 使用状态（逐文件口径）",
        "",
        "| 状态 | 文件数 |",
        "| --- | ---: |",
    ]
    for usage, count in sorted(Counter(row["usage"] for row in rows).items()):
        lines.append(f"| {cell(usage)} | {count} |")
    lines += [
        "",
        "## 分组与处理建议",
        "",
        "表中数量包含该组随包说明；真正的引用状态以 CSV 的逐文件 usage 为准。",
        "“先核许可”不是“已确认侵权”；“归档”是建议，不是删除授权。",
        "",
        "| 资产组 / ID | 文件数 | MiB | 使用状态 | 分仓建议 | 来源/许可证据 | 尚缺信息 |",
        "| --- | ---: | ---: | --- | --- | --- | --- |",
    ]
    for group in catalog["groups"]:
        members = [row for row in rows if row["group"] == group["id"]]
        size = sum(int(row["bytes"]) for row in members) / 1024**2
        values = [
            f"{group['name']} (`{group['id']}`)",
            str(len(members)),
            f"{size:.3f}",
            group["usage"],
            group["decision"],
            group["source"],
            group["gap"],
        ]
        lines.append("| " + " | ".join(cell(value) for value in values) + " |")
    unknown = [row for row in rows if row["group"] == "UNREVIEWED"]
    if unknown:
        lines += ["", "## 未审阅文件", ""] + [f"- `{row['path']}`" for row in unknown]
    lines += ["", "## 引用与来源证据索引", ""]
    for group in catalog["groups"]:
        refs = [
            f"[{path}](../../{quote(path, safe='/')})" for path in group["evidence"]
        ]
        lines.append(f"- **{group['id']}**：" + "；".join(refs))
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="check committed tables without writing; fail on stale or unreviewed entries",
    )
    args = parser.parse_args()
    catalog: Catalog = json.loads((ROOT / CATALOG).read_text(encoding="utf-8"))
    groups = catalog["groups"]
    if len({group["id"] for group in groups}) != len(groups):
        raise ValueError("Duplicate group IDs")
    for group in groups:
        for path in group["evidence"]:
            if not (ROOT / path).exists():
                raise ValueError(f"Missing evidence path: {path}")
    rows = inventory(ROOT, tracked_paths(ROOT), groups)
    outputs = {"inventory.csv": csv_text(rows), "inventory.md": report(rows, catalog)}
    stale = []
    for name, content in outputs.items():
        path = ROOT / "docs/assets" / name
        if args.check:
            if not path.exists() or path.read_text(encoding="utf-8") != content:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_text(content, encoding="utf-8")
    unknown = sum(row["group"] == "UNREVIEWED" for row in rows)
    print(f"assets={len(rows)} groups={len(groups)} unreviewed={unknown}")
    if stale:
        print("Stale tables: " + ", ".join(stale))
        print("Run python3 scripts/asset_inventory.py after reviewing catalog.json.")
    return int(bool(stale or unknown))


if __name__ == "__main__":
    raise SystemExit(main())
