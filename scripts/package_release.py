#!/usr/bin/env python3
"""Create a self-contained Re: Flora binary package for itch.io."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import zipfile
from pathlib import Path

APP_NAME = "re-flora"
PACKAGE_DIRS = ["assets", "config"]
PACKAGE_FILES = [
    "README.md",
    "LICENSE",
    "LICENSE-ASSETS",
    "docs/playing.md",
    "demo/img/splash.png",
]


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


def host_platform() -> str:
    system = platform.system().lower()
    if system == "darwin":
        return "macos"
    if system == "windows":
        return "windows"
    if system == "linux":
        return "linux"
    return system


def default_channel() -> str:
    system = host_platform()
    machine = platform.machine().lower().replace("amd64", "x86_64")
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
            dst = stage_root / filename
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)


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


def run_allow_duplicate_rpath(command: list[str]) -> None:
    result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode == 0:
        return
    combined = f"{result.stdout}\n{result.stderr}"
    if "would duplicate path" in combined or "already exists" in combined:
        return
    raise RuntimeError(combined.strip() or f"command failed: {' '.join(command)}")


def fix_unix_runtime_paths(binary_path: Path, current_platform: str, has_lib_dir: bool) -> None:
    if not has_lib_dir:
        return

    if current_platform == "macos":
        run_allow_duplicate_rpath(
            ["install_name_tool", "-add_rpath", "@executable_path/lib", str(binary_path)]
        )
    elif current_platform == "linux":
        patchelf = shutil.which("patchelf")
        if patchelf is None:
            raise FileNotFoundError(
                "patchelf is required to make the Linux package find bundled shared libraries"
            )
        subprocess.run(
            [patchelf, "--set-rpath", "$ORIGIN/lib", str(binary_path)], check=True
        )


def brew_prefix(package: str) -> Path | None:
    brew = shutil.which("brew")
    if brew is None:
        return None
    result = subprocess.run(
        [brew, "--prefix", package], text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        return None
    path = Path(result.stdout.strip())
    return path if path.exists() else None


def first_existing(paths: list[Path]) -> Path | None:
    for path in paths:
        if path.exists():
            return path
    return None


def copy_macos_vulkan_runtime(stage_root: Path) -> list[Path]:
    if host_platform() != "macos":
        return []

    vulkan_loader_prefix = brew_prefix("vulkan-loader")
    molten_vk_prefix = brew_prefix("molten-vk")
    vulkan_sdk = Path(os.environ["VULKAN_SDK"]) if "VULKAN_SDK" in os.environ else None

    loader_candidates = []
    molten_candidates = []
    if vulkan_loader_prefix:
        loader_candidates.extend(
            [
                vulkan_loader_prefix / "lib" / "libvulkan.1.dylib",
                vulkan_loader_prefix / "lib" / "libvulkan.dylib",
            ]
        )
    if molten_vk_prefix:
        molten_candidates.extend(
            [
                molten_vk_prefix / "lib" / "libMoltenVK.dylib",
                molten_vk_prefix / "lib" / "MoltenVK.xcframework" / "macos-arm64_x86_64" / "libMoltenVK.dylib",
            ]
        )
    if vulkan_sdk:
        loader_candidates.extend(
            [vulkan_sdk / "lib" / "libvulkan.1.dylib", vulkan_sdk / "lib" / "libvulkan.dylib"]
        )
        molten_candidates.extend(
            [vulkan_sdk / "lib" / "libMoltenVK.dylib", vulkan_sdk / "MoltenVK" / "dylib" / "macOS" / "libMoltenVK.dylib"]
        )

    # Common Homebrew locations for local packaging without invoking brew.
    for prefix in (Path("/opt/homebrew"), Path("/usr/local")):
        loader_candidates.extend(
            [
                prefix / "opt" / "vulkan-loader" / "lib" / "libvulkan.1.dylib",
                prefix / "opt" / "vulkan-loader" / "lib" / "libvulkan.dylib",
            ]
        )
        molten_candidates.extend(
            [
                prefix / "opt" / "molten-vk" / "lib" / "libMoltenVK.dylib",
                prefix / "opt" / "molten-vk" / "libexec" / "MoltenVK.xcframework" / "macos-arm64_x86_64" / "libMoltenVK.dylib",
            ]
        )

    loader = first_existing(loader_candidates)
    molten = first_existing(molten_candidates)
    if loader is None or molten is None:
        missing = []
        if loader is None:
            missing.append("Vulkan loader libvulkan.1.dylib")
        if molten is None:
            missing.append("MoltenVK libMoltenVK.dylib")
        raise FileNotFoundError(
            "macOS package requires bundled Vulkan runtime pieces; missing " + ", ".join(missing)
        )

    lib_dir = stage_root / "lib"
    lib_dir.mkdir(parents=True, exist_ok=True)
    copied = []
    for src, name in ((loader, "libvulkan.1.dylib"), (molten, "libMoltenVK.dylib")):
        dst = lib_dir / name
        shutil.copy2(src, dst)
        copied.append(dst)
        print(f"Bundled macOS Vulkan runtime: {dst.relative_to(stage_root)}", file=sys.stderr)

    icd_dir = stage_root / "vulkan" / "icd.d"
    icd_dir.mkdir(parents=True, exist_ok=True)
    icd = {
        "file_format_version": "1.0.0",
        "ICD": {
            "library_path": "../../lib/libMoltenVK.dylib",
            "api_version": "1.3.0",
        },
    }
    icd_path = icd_dir / "MoltenVK_icd.json"
    icd_path.write_text(json.dumps(icd, indent=2) + "\n", encoding="utf-8")
    copied.append(icd_path)
    print(f"Bundled macOS Vulkan ICD: {icd_path.relative_to(stage_root)}", file=sys.stderr)
    return copied


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
    current_platform = host_platform()

    if stage_root.exists():
        shutil.rmtree(stage_root)
    stage_root.mkdir(parents=True)
    dist_dir.mkdir(parents=True, exist_ok=True)

    binary_path = copy_binary(root, stage_root, target_dir)
    copy_runtime_tree(root, stage_root)
    copied_vulkan = copy_macos_vulkan_runtime(stage_root)
    fix_unix_runtime_paths(
        binary_path,
        current_platform,
        has_lib_dir=(stage_root / "lib").exists() and bool(copied_vulkan),
    )
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
