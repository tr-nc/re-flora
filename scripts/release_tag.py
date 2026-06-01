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
    raise ReleaseTagError("package.version not found in Cargo.toml")


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
        cargo_ver = cargo_version(root)
        tag, version = canonical_tag(args.version or cargo_ver)

        if version != cargo_ver and not args.allow_version_mismatch:
            raise ReleaseTagError(
                f"tag version {version!r} does not match Cargo.toml version {cargo_ver!r}; "
                "update Cargo.toml/Cargo.lock first or use --allow-version-mismatch"
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

        tag_message = args.message or f"Re: Flora {version}"
        tag_command = ["git", "tag", "-a", tag, "-m", tag_message]
        push_command = ["git", "push", args.remote, tag]

        if args.dry_run:
            print(f"dry run: would run {shlex.join(tag_command)}")
            if push:
                print(f"dry run: would run {shlex.join(push_command)}")
            return 0

        action = f"Create annotated tag {tag} at {commit}"
        if push:
            action += f" and push it to {args.remote}"
        if not args.yes and not confirm(action + "?"):
            print("aborted")
            return 1

        run(tag_command, root)
        print(f"Created local tag {tag}")

        if push:
            run(push_command, root)
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
