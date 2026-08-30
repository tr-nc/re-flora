#!/usr/bin/env python3
"""Reject fatal or Vulkan validation diagnostics in the latest app run log."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from runtime_log_diagnostics import fatal_diagnostic_excerpts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log-root", type=Path, default=Path("target/re-flora-logs"))
    parser.add_argument("--tail", type=int, default=20)
    args = parser.parse_args()

    pointer = args.log_root / "latest-run-log.txt"
    log = Path(pointer.read_text(encoding="utf-8").strip())
    text = log.read_text(encoding="utf-8", errors="replace")
    matches = fatal_diagnostic_excerpts(text)
    if matches:
        print("latest run contains fatal or validation diagnostics:", file=sys.stderr)
        print("\n".join(matches), file=sys.stderr)
        return 1

    lines = text.splitlines()
    print("\n".join(lines[-args.tail :]))
    print(f"validated run log: {log}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
