#!/usr/bin/env python3
"""Create and push a release tag that triggers the package CI workflow."""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path

DEFAULT_BRANCH = "main"
DEFAULT_REMOTE = "origin"
WORKFLOW_PATH = ".github/workflows/itch-builds.yml"
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")


class ReleaseTagError(RuntimeError):
    pass


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def cargo_package_info(root: Path) -> tuple[str, str]:
    package_name: str | None = None
    package_version: str | None = None
    in_package = False
    for raw_line in (root / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line == "[package]":
            in_package = True
            continue
        if in_package and line.startswith("["):
            break
        if in_package and line.startswith("name"):
            _key, value = line.split("=", 1)
            package_name = value.strip().strip('"')
        if in_package and line.startswith("version"):
            _key, value = line.split("=", 1)
            package_version = value.strip().strip('"')
    if package_name is None:
        raise ReleaseTagError("package.name not found in Cargo.toml")
    if package_version is None:
        raise ReleaseTagError("package.version not found in Cargo.toml")
    return package_name, package_version


def cargo_version(root: Path) -> str:
    return cargo_package_info(root)[1]


def bump_patch_version(version: str) -> str:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        raise ReleaseTagError(
            f"--bump-patch requires a plain X.Y.Z Cargo.toml version; got {version!r}"
        )
    major, minor, patch = (int(part) for part in match.groups())
    return f"{major}.{minor}.{patch + 1}"


def bump_minor_version(version: str) -> str:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match:
        raise ReleaseTagError(
            f"--bump-minor requires a plain X.Y.Z Cargo.toml version; got {version!r}"
        )
    major, minor, _patch = (int(part) for part in match.groups())
    return f"{major}.{minor + 1}.0"


def update_cargo_toml_version(root: Path, *, old_version: str, new_version: str) -> None:
    path = root / "Cargo.toml"
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    in_package = False
    changed = False
    for index, raw_line in enumerate(lines):
        line = raw_line.strip()
        if line == "[package]":
            in_package = True
            continue
        if in_package and line.startswith("["):
            break
        if in_package and line.startswith("version"):
            newline = "\n" if raw_line.endswith("\n") else ""
            lines[index] = f'version = "{new_version}"{newline}'
            changed = True
            break
    if not changed:
        raise ReleaseTagError("package.version not found in Cargo.toml")
    path.write_text("".join(lines), encoding="utf-8")


def update_cargo_lock_version(
    root: Path, *, package_name: str, old_version: str, new_version: str
) -> None:
    path = root / "Cargo.lock"
    text = path.read_text(encoding="utf-8")
    blocks = text.split("[[package]]")
    changed = False
    for index in range(1, len(blocks)):
        block = blocks[index]
        if re.search(rf'^name = "{re.escape(package_name)}"$', block, re.MULTILINE):
            updated = re.sub(
                rf'^version = "{re.escape(old_version)}"$',
                f'version = "{new_version}"',
                block,
                count=1,
                flags=re.MULTILINE,
            )
            if updated == block:
                raise ReleaseTagError(
                    f"found {package_name!r} in Cargo.lock but version was not {old_version!r}"
                )
            blocks[index] = updated
            changed = True
            break
    if not changed:
        raise ReleaseTagError(f"package {package_name!r} not found in Cargo.lock")
    path.write_text("[[package]]".join(blocks), encoding="utf-8")


def update_cargo_version_files(root: Path, *, new_version: str) -> str:
    package_name, old_version = cargo_package_info(root)
    if old_version == new_version:
        raise ReleaseTagError(f"Cargo.toml is already at version {new_version}")
    update_cargo_toml_version(root, old_version=old_version, new_version=new_version)
    update_cargo_lock_version(
        root,
        package_name=package_name,
        old_version=old_version,
        new_version=new_version,
    )
    return old_version


def run(command: list[str], root: Path, *, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(command, cwd=root, text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        details = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
        raise ReleaseTagError(f"command failed: {shlex.join(command)}\n{details}")
    return result


def git(root: Path, *args: str, check: bool = True) -> str:
    return run(["git", *args], root, check=check).stdout.strip()


def ensure_clean_worktree(root: Path, *, allow_dirty: bool) -> None:
    status = git(root, "status", "--porcelain")
    if status and not allow_dirty:
        raise ReleaseTagError(
            "working tree is not clean; commit or stash changes before tagging\n"
            "use --allow-dirty only if you intentionally want to tag HEAD while local changes exist"
        )


def ensure_branch(root: Path, *, expected_branch: str, allow_any_branch: bool) -> str:
    branch = git(root, "branch", "--show-current")
    if not branch:
        raise ReleaseTagError("HEAD is detached; check out the release branch before tagging")
    if branch != expected_branch and not allow_any_branch:
        raise ReleaseTagError(
            f"current branch is {branch!r}, expected {expected_branch!r}; "
            "use --allow-any-branch to override"
        )
    return branch


def ensure_up_to_date(root: Path, *, remote: str, branch: str, skip: bool) -> None:
    if skip:
        return
    head = git(root, "rev-parse", "HEAD")
    upstream = git(root, "rev-parse", "--verify", f"{remote}/{branch}")
    if head != upstream:
        raise ReleaseTagError(
            f"HEAD does not match {remote}/{branch}; pull/rebase or push your branch before tagging\n"
            "use --skip-up-to-date-check to override"
        )


def local_tag_exists(root: Path, tag: str) -> bool:
    result = run(["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}"], root, check=False)
    return result.returncode == 0


def remote_tag_exists(root: Path, *, remote: str, tag: str) -> bool:
    result = run(
        ["git", "ls-remote", "--exit-code", "--tags", remote, f"refs/tags/{tag}"],
        root,
        check=False,
    )
    if result.returncode == 0:
        return True
    if result.returncode == 2:
        return False
    details = result.stderr.strip() or result.stdout.strip() or f"exit code {result.returncode}"
    raise ReleaseTagError(f"could not check remote tag {tag!r}: {details}")


def canonical_tag(raw_version: str) -> tuple[str, str]:
    version = raw_version[1:] if raw_version.startswith("v") else raw_version
    if not VERSION_RE.fullmatch(version):
        raise ReleaseTagError(
            f"version must look like 1.2.3, 1.2.3-rc.1, or v1.2.3; got {raw_version!r}"
        )
    return f"v{version}", version


def confirm(prompt: str) -> bool:
    answer = input(f"{prompt} [y/N] ").strip().lower()
    return answer in {"y", "yes"}


def print_plan(*, tag: str, version: str, branch: str, remote: str, commit: str, push: bool) -> None:
    print(f"Release tag: {tag}")
    print(f"Package version: {version}")
    print(f"Target commit: {commit}")
    print(f"Source branch: {branch}")
    if push:
        print(f"CI trigger: git push {remote} {tag} -> {WORKFLOW_PATH}")
    else:
        print("CI trigger: disabled by --no-push; push the tag later to start CI")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "version",
        nargs="?",
        help="release version or tag, e.g. 0.2.1 or v0.2.1; defaults to Cargo.toml version",
    )
    parser.add_argument("--remote", default=DEFAULT_REMOTE, help="git remote to push to")
    parser.add_argument("--branch", default=DEFAULT_BRANCH, help="release branch expected for tagging")
    parser.add_argument("--message", help="annotated tag message; defaults to 'Re: Flora <version>'")
    parser.add_argument("--no-push", action="store_true", help="create the tag locally but do not trigger CI")
    parser.add_argument("--dry-run", action="store_true", help="print the actions without creating or pushing a tag")
    parser.add_argument(
        "--bump-patch",
        action="store_true",
        help="increment Cargo.toml/Cargo.lock patch version, commit it, then tag that commit",
    )
    parser.add_argument(
        "--bump-minor",
        action="store_true",
        help="increment Cargo.toml/Cargo.lock minor version, reset patch to zero, commit, then tag",
    )
    parser.add_argument("-y", "--yes", action="store_true", help="skip the confirmation prompt")
    parser.add_argument("--allow-dirty", action="store_true", help="allow tagging with local changes present")
    parser.add_argument(
        "--allow-any-branch",
        action="store_true",
        help="allow tagging from a branch other than --branch",
    )
    parser.add_argument(
        "--allow-version-mismatch",
        action="store_true",
        help="allow the tag version to differ from Cargo.toml package.version",
    )
    parser.add_argument("--skip-fetch", action="store_true", help="do not fetch the release branch and tags first")
    parser.add_argument(
        "--skip-up-to-date-check",
        action="store_true",
        help="do not require HEAD to match <remote>/<branch>",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repo_root()

    try:
        bump_requested = args.bump_patch or args.bump_minor
        if args.bump_patch and args.bump_minor:
            raise ReleaseTagError("pass either --bump-patch or --bump-minor, not both")
        if bump_requested and args.version:
            raise ReleaseTagError("pass a bump option or an explicit version, not both")
        if bump_requested and args.allow_version_mismatch:
            raise ReleaseTagError("bump options cannot be combined with --allow-version-mismatch")

        cargo_ver = cargo_version(root)
        if args.bump_patch:
            version_input = bump_patch_version(cargo_ver)
        elif args.bump_minor:
            version_input = bump_minor_version(cargo_ver)
        else:
            version_input = args.version or cargo_ver
        tag, version = canonical_tag(version_input)

        if version != cargo_ver and not bump_requested and not args.allow_version_mismatch:
            raise ReleaseTagError(
                f"tag version {version!r} does not match Cargo.toml version {cargo_ver!r}; "
                "update Cargo.toml/Cargo.lock first, use a bump option, or use "
                "--allow-version-mismatch"
            )

        ensure_clean_worktree(root, allow_dirty=args.allow_dirty)
        branch = ensure_branch(root, expected_branch=args.branch, allow_any_branch=args.allow_any_branch)

        if not args.skip_fetch:
            git(
                root,
                "fetch",
                "--tags",
                args.remote,
                f"+refs/heads/{branch}:refs/remotes/{args.remote}/{branch}",
            )

        ensure_up_to_date(root, remote=args.remote, branch=branch, skip=args.skip_up_to_date_check)

        if local_tag_exists(root, tag):
            raise ReleaseTagError(f"local tag already exists: {tag}")
        if remote_tag_exists(root, remote=args.remote, tag=tag):
            raise ReleaseTagError(f"remote tag already exists on {args.remote}: {tag}")

        commit = git(root, "rev-parse", "--short=12", "HEAD")
        push = not args.no_push
        print_plan(tag=tag, version=version, branch=branch, remote=args.remote, commit=commit, push=push)

        bump_commit_command = ["git", "commit", "-m", f"bump version to {version}"]
        push_branch_command = ["git", "push", args.remote, branch]
        tag_message = args.message or f"Re: Flora {version}"
        tag_command = ["git", "tag", "-a", tag, "-m", tag_message]
        push_tag_command = ["git", "push", args.remote, tag]

        if args.dry_run:
            if bump_requested:
                print(f"dry run: would update Cargo.toml/Cargo.lock from {cargo_ver} to {version}")
                print("dry run: would run git add Cargo.toml Cargo.lock")
                print(f"dry run: would run {shlex.join(bump_commit_command)}")
                if push:
                    print(f"dry run: would run {shlex.join(push_branch_command)}")
            print(f"dry run: would run {shlex.join(tag_command)}")
            if push:
                print(f"dry run: would run {shlex.join(push_tag_command)}")
            return 0

        action = f"Create annotated tag {tag} at {commit}"
        if bump_requested:
            action = f"Bump version from {cargo_ver} to {version}, commit it, then {action}"
        if push:
            action += f" and push it to {args.remote}"
        if not args.yes and not confirm(action + "?"):
            print("aborted")
            return 1

        if bump_requested:
            update_cargo_version_files(root, new_version=version)
            run(["git", "add", "Cargo.toml", "Cargo.lock"], root)
            run(bump_commit_command, root)
            commit = git(root, "rev-parse", "--short=12", "HEAD")
            print(f"Committed version bump to {version} at {commit}")
            if push:
                run(push_branch_command, root)
                print(f"Pushed {branch} to {args.remote}")

        run(tag_command, root)
        print(f"Created local tag {tag}")

        if push:
            run(push_tag_command, root)
            print("Pushed tag; GitHub Actions should start the release packages workflow.")
            print("Watch with: gh run list --workflow itch-builds.yml --limit 3")
        else:
            print(f"Push later with: git push {args.remote} {tag}")

        return 0
    except ReleaseTagError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
