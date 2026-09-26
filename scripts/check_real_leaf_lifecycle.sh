#!/usr/bin/env bash
# Bounded GPU/app contract replay; not part of cargo test and not a perf benchmark.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
env -u WAYLAND_DISPLAY RE_FLORA_REAL_LEAVES_VALIDATE=1 \
    flock --close "${RE_FLORA_GPU_LOCK:-/tmp/re-flora-summer-gpu.lock}" \
    cargo run --release -- --hidden --mute --auto-exit 8
log="$(cargo run --release -- --latest-log 2>/dev/null)"
grep -F '[LEAF_LIFECYCLE][VALIDATE] PASS' "$log"
grep -F '[SHUTDOWN] phase=complete failures=0' "$log"
if grep -En ' ERROR |panicked at|VUID-' "$log"; then
    printf 'Leaf lifecycle validation found errors: %s\n' "$log" >&2
    exit 1
fi
printf 'Leaf lifecycle validation passed: %s\n' "$log"
