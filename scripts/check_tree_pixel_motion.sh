#!/usr/bin/env bash
# Native camera-motion/material regression, not a performance benchmark.
# Replay buffers are isolated; this does not edit saved settings or launch a visible game.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
env -u WAYLAND_DISPLAY RE_FLORA_TREE_PIXEL_MOTION_VALIDATE=1 \
    flock --close "${RE_FLORA_GPU_LOCK:-/tmp/re-flora-summer-gpu.lock}" \
    cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 30
log="$(cargo run --release -- --latest-log 2>/dev/null)"
grep -F '[TREE][MOTION_VALIDATE] PASS cases=36' "$log"
grep -F '[SHUTDOWN] phase=complete failures=0' "$log"
if grep -En ' ERROR |panicked at|VUID-' "$log"; then
    printf 'Tree pixel motion regression found errors: %s\n' "$log" >&2
    exit 1
fi
printf 'Tree pixel motion regression passed: %s\n' "$log"
