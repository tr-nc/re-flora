#!/usr/bin/env python3
"""Print GitHub release asset download counts for this repository."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

DEFAULT_REPO = "tr-nc/re-flora"


def github_api(path: str, token: str | None) -> object:
    request = urllib.request.Request(f"https://api.github.com{path}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("X-GitHub-Api-Version", "2022-11-28")
    if token:
        request.add_header("Authorization", f"Bearer {token}")

    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise SystemExit(f"GitHub API request failed: HTTP {exc.code}\n{body}") from exc


def iter_releases(repo: str, token: str | None, tag: str | None) -> list[dict[str, object]]:
    if tag:
        release = github_api(f"/repos/{repo}/releases/tags/{tag}", token)
        if not isinstance(release, dict):
            raise SystemExit("Unexpected GitHub API response for release tag")
        return [release]

    releases = github_api(f"/repos/{repo}/releases?per_page=100", token)
    if not isinstance(releases, list):
        raise SystemExit("Unexpected GitHub API response for release list")
    return releases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=DEFAULT_REPO, help=f"owner/repo, default: {DEFAULT_REPO}")
    parser.add_argument("--tag", help="only show one release tag, e.g. v0.2.2")
    parser.add_argument(
        "--token",
        default=os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN"),
        help="GitHub token; defaults to GITHUB_TOKEN or GH_TOKEN",
    )
    args = parser.parse_args()

    releases = iter_releases(args.repo, args.token, args.tag)
    if not releases:
        print("No GitHub releases found.")
        return 0

    total = 0
    for release in releases:
        tag_name = str(release.get("tag_name", "<unknown>"))
        assets = release.get("assets", [])
        if not isinstance(assets, list):
            continue

        print(tag_name)
        if not assets:
            print("  no assets")
            continue

        for asset in assets:
            if not isinstance(asset, dict):
                continue
            name = str(asset.get("name", "<unknown>"))
            count = int(asset.get("download_count", 0))
            total += count
            print(f"  {count:6d}  {name}")

    print(f"total: {total}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
