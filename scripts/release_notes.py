#!/usr/bin/env python3
"""Validate committed, player-facing notes before tagging or packaging a build."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

VERSION_RE = re.compile(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?")
SECTIONS = ("What's new", "Known limitations", "How to play")


class ReleaseNotesError(ValueError):
    pass


def notes_path(version: str) -> str:
    if not VERSION_RE.fullmatch(version):
        raise ReleaseNotesError(f"invalid package version: {version!r}; use e.g. 0.4.1 or 0.4.1-preview.1")
    return f"docs/releases/{version}.md"


def validate_notes(text: str, version: str) -> str:
    """Check the contract, not the truth or quality of the agent's prose."""
    notes_path(version)
    if not text.startswith(f"# Re: Flora {version}\n"):
        raise ReleaseNotesError(f"notes must start with '# Re: Flora {version}'")
    if re.search(r"\b(?:TODO|TBD|PLACEHOLDER)\b|Automated package build", text, re.I):
        raise ReleaseNotesError("replace TODO/TBD/placeholder or automated boilerplate with actual player notes")
    for heading in SECTIONS:
        match = re.search(rf"^## {re.escape(heading)}\s*\n(.*?)(?=^## |\Z)", text, re.M | re.S)
        if not match or len(match.group(1).strip()) < 15:
            raise ReleaseNotesError(f"write a substantive '## {heading}' section (not an empty heading)")
    return text.rstrip() + "\n"


def committed_notes(root: Path, version: str) -> str:
    path = notes_path(version)
    result = subprocess.run(["git", "show", f"HEAD:{path}"], cwd=root, text=True, encoding="utf-8", capture_output=True)
    if result.returncode:
        raise ReleaseNotesError(
            f"missing committed player notes: {path}\n"
            f"Read docs/agents/releasing.md, write and commit {path}, then retry.\n"
            f"Check with: python3 scripts/release_notes.py --version {version}\n"
            "Uncommitted files are not included in the release target."
        )
    return validate_notes(result.stdout, version)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="exact package version, e.g. 0.4.1 or 0.4.1-preview.1")
    parser.add_argument("--output", type=Path, help="write validated HEAD notes here (for packaging / GitHub --notes-file)")
    args = parser.parse_args()
    try:
        text = committed_notes(Path(__file__).resolve().parents[1], args.version)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(text, encoding="utf-8")
        print(f"Player notes ready: {notes_path(args.version)}")
        return 0
    except (ReleaseNotesError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
