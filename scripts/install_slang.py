#!/usr/bin/env python3
"""Install the pinned Slang compiler release used by shader validation."""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

SLANG_VERSION = "2025.23"
RELEASE_BASE_URL = (
    f"https://github.com/shader-slang/slang/releases/download/v{SLANG_VERSION}"
)


@dataclass(frozen=True)
class ReleaseAsset:
    archive: str
    sha256: str


ASSETS = {
    ("linux", "x86_64"): ReleaseAsset(
        "slang-2025.23-linux-x86_64-glibc-2.27.tar.gz",
        "d51403562ef12a72b40f57c22838bee43666b703b0807ccd8595e59766a8dd97",
    ),
    ("linux", "aarch64"): ReleaseAsset(
        "slang-2025.23-linux-aarch64.tar.gz",
        "d79841aaab94e5a19c7cdcc95c18ccd9466c0afc0d9ffa4d132ff4f5da1a71b7",
    ),
    ("macos", "x86_64"): ReleaseAsset(
        "slang-2025.23-macos-x86_64.tar.gz",
        "b0fc2c57b793f18e0d2c416a677eee2a82fc33eccd09243a375db09f881883da",
    ),
    ("macos", "aarch64"): ReleaseAsset(
        "slang-2025.23-macos-aarch64.tar.gz",
        "fb009a68c861850cbea28344b026de8a381e10ad20b4a287f4b5446f1cbb2a15",
    ),
    ("windows", "x86_64"): ReleaseAsset(
        "slang-2025.23-windows-x86_64.zip",
        "c81e04cb5609c30f296bc149952581744572f56adf8cd9297086e9a7f120fc3a",
    ),
}


def host_key() -> tuple[str, str]:
    systems = {"linux": "linux", "darwin": "macos", "windows": "windows"}
    machines = {
        "x86_64": "x86_64",
        "amd64": "x86_64",
        "aarch64": "aarch64",
        "arm64": "aarch64",
    }
    system = systems.get(platform.system().lower(), platform.system().lower())
    machine = machines.get(platform.machine().lower(), platform.machine().lower())
    return system, machine


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def installed_paths(destination: Path, system: str) -> tuple[Path, Path]:
    executable = destination / "bin" / ("slangc.exe" if system == "windows" else "slangc")
    library_names = {
        "linux": ("lib/libslang.so",),
        "macos": ("lib/libslang.dylib",),
        "windows": ("bin/slang.dll", "lib/slang.dll"),
    }
    libraries = tuple(destination / name for name in library_names[system])
    library = next((candidate for candidate in libraries if candidate.is_file()), libraries[0])
    return executable, library


def download_and_extract(asset: ReleaseAsset, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as temporary_directory:
        temporary = Path(temporary_directory)
        archive = temporary / asset.archive
        url = f"{RELEASE_BASE_URL}/{asset.archive}"
        print(f"Downloading {url}")
        urllib.request.urlretrieve(url, archive)
        actual_digest = sha256(archive)
        if actual_digest != asset.sha256:
            raise RuntimeError(
                f"SHA-256 mismatch for {asset.archive}: "
                f"expected {asset.sha256}, got {actual_digest}"
            )

        extracted = temporary / "extracted"
        extracted.mkdir()
        shutil.unpack_archive(archive, extracted)
        if destination.exists():
            shutil.rmtree(destination)
        extracted.replace(destination)


def append_github_environment(environment: Path, executable: Path, library: Path) -> None:
    with environment.open("a", encoding="utf-8") as file:
        file.write(f"SLANGC={executable}\n")
        file.write(f"SLANG_LIB={library}\n")
        file.write(f"SLANG_VERSION={SLANG_VERSION}\n")

    github_path = os.environ.get("GITHUB_PATH")
    if github_path:
        with Path(github_path).open("a", encoding="utf-8") as file:
            file.write(f"{executable.parent}\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--destination",
        type=Path,
        default=Path(".tools") / f"slang-{SLANG_VERSION}",
        help="installation directory",
    )
    parser.add_argument(
        "--github-env",
        type=Path,
        help="append SLANGC, SLANG_LIB, and SLANG_VERSION to this GitHub Actions env file",
    )
    args = parser.parse_args()

    system, machine = host_key()
    asset = ASSETS.get((system, machine))
    if asset is None:
        supported = ", ".join(f"{key[0]}/{key[1]}" for key in sorted(ASSETS))
        parser.error(f"unsupported host {system}/{machine}; supported hosts: {supported}")

    destination = args.destination.resolve()
    executable, library = installed_paths(destination, system)
    if not executable.is_file() or not library.is_file():
        download_and_extract(asset, destination)
        executable, library = installed_paths(destination, system)
    if not executable.is_file():
        raise RuntimeError(f"Slang executable was not installed at {executable}")
    if not library.is_file():
        raise RuntimeError(f"Slang shared library was not installed at {library}")

    result = subprocess.run(
        [executable, "-version"],
        check=True,
        capture_output=True,
        text=True,
    )
    version_output = (result.stdout + result.stderr).strip()
    if SLANG_VERSION not in version_output:
        raise RuntimeError(
            f"expected Slang {SLANG_VERSION}, got: {version_output or '<no version output>'}"
        )

    if args.github_env:
        append_github_environment(args.github_env, executable, library)

    print(f"Slang {SLANG_VERSION}: {executable}")
    print(f"Slang library: {library}")
    print(version_output)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
