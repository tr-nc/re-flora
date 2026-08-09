#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

screenshot="${1:-target/ddgi-seam-repro/patt-seam.png}"
mkdir -p "$(dirname "$screenshot")"

cargo run --release -- \
    --hidden --mute --windowed \
    --environment-lighting-test-scene patt-seam \
    --screenshot patt "$screenshot" \
    --screenshot-delay 2 \
    --auto-exit 8

exec "$repo_root/scripts/analyze_patt_ddgi_seam.py" "$screenshot"
