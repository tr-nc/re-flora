#!/usr/bin/env python3
"""Create a self-contained Re: Flora binary package for itch.io."""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import stat
import sys
import zipfile
from pathlib import Path

APP_NAME = "re-flora"
PACKAGE_DIRS = ["assets", "config", "shader"]
PACKAGE_FILES = ["README.md", "LICENSE", "LICENSE-ASSETS"]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def cargo_version(root: Path) -> str:
    in_package = False
    for raw_line in (root / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line == "[package]":
            in_package = True
            continue
        if in_package and line.startswith("["):
            break
        if in_package and line.startswith("version"):
            _key, value = line.split("=", 1)
            return value.strip().strip('"')
    raise ValueError("package.version not found in Cargo.toml")


def default_channel() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower().replace("amd64", "x86_64")
    if system == "darwin":
        system = "macos"
    elif system == "linux":
        system = "linux"
    return f"{system}-{machine}"


def safe_component(value: str) -> str:
    value = value.strip().replace(os.sep, "-")
    if os.altsep:
        value = value.replace(os.altsep, "-")
    return re.sub(r"[^A-Za-z0-9._+-]+", "-", value).strip("-")


def binary_name() -> str:
    return f"{APP_NAME}.exe" if platform.system() == "Windows" else APP_NAME


def ignore_packaged_junk(_dir: str, names: list[str]) -> set[str]:
    ignored = {".DS_Store", "Thumbs.db"}
    return {name for name in names if name in ignored}


def copy_runtime_tree(root: Path, stage_root: Path) -> None:
    for dirname in PACKAGE_DIRS:
        src = root / dirname
        if not src.exists():
            raise FileNotFoundError(f"required package directory is missing: {src}")
        shutil.copytree(src, stage_root / dirname, ignore=ignore_packaged_junk)

    for filename in PACKAGE_FILES:
        src = root / filename
        if src.exists():
            shutil.copy2(src, stage_root / filename)


def copy_binary(root: Path, stage_root: Path, target_dir: Path) -> Path:
    src = target_dir / "release" / binary_name()
    if not src.exists():
        raise FileNotFoundError(
            f"release binary not found: {src}\n"
            "Run `cargo build --release --locked` before packaging."
        )

    dst = stage_root / binary_name()
    shutil.copy2(src, dst)

    if platform.system() != "Windows":
        mode = dst.stat().st_mode
        dst.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    return dst


def write_build_info(stage_root: Path, *, version: str, channel: str) -> None:
    fields = {
        "app": APP_NAME,
        "version": version,
        "channel": channel,
        "git_sha": os.environ.get("GITHUB_SHA", "unknown"),
        "git_ref": os.environ.get("GITHUB_REF_NAME", "unknown"),
        "built_on": platform.platform(),
    }
    content = "".join(f"{key}={value}\n" for key, value in fields.items())
    (stage_root / "BUILD_INFO.txt").write_text(content, encoding="utf-8")


def zip_dir(src_dir: Path, archive_path: Path) -> None:
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(src_dir.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(src_dir.parent))


def package(args: argparse.Namespace) -> Path:
    root = repo_root()
    version = args.version or cargo_version(root)
    channel = args.channel or default_channel()
    package_name = f"{APP_NAME}-{safe_component(version)}-{safe_component(channel)}"

    dist_dir = (root / args.dist_dir).resolve()
    stage_parent = dist_dir / "stage"
    stage_root = stage_parent / package_name
    target_dir = (root / args.target_dir).resolve()

    if stage_root.exists():
        shutil.rmtree(stage_root)
    stage_root.mkdir(parents=True)
    dist_dir.mkdir(parents=True, exist_ok=True)

    copy_binary(root, stage_root, target_dir)
    copy_runtime_tree(root, stage_root)
    write_build_info(stage_root, version=version, channel=channel)

    archive_path = dist_dir / f"{package_name}.zip"
    if archive_path.exists():
        archive_path.unlink()
    zip_dir(stage_root, archive_path)

    return archive_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", help="package/user version; defaults to Cargo.toml version")
    parser.add_argument("--channel", help="itch channel name, e.g. windows, macos, fedora")
    parser.add_argument("--target-dir", default="target", help="Cargo target dir, default: target")
    parser.add_argument("--dist-dir", default="dist", help="package output dir, default: dist")
    parser.add_argument(
        "--print-version",
        action="store_true",
        help="print the Cargo.toml package version and exit",
    )
    args = parser.parse_args()

    root = repo_root()
    if args.print_version:
        print(cargo_version(root))
        return 0

    archive_path = package(args)
    print(archive_path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
