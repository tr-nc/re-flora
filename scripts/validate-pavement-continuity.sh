#!/usr/bin/env bash
# Real native input/picking/publication; not OS-mouse or perf acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/pavement-continuity"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
RE_FLORA_PAVEMENT_REVIEW=turn RUST_LOG=info target/release/re-flora \
    --hidden --mute --auto-exit 6 \
    --screenshot player-default "$output/blocked.png" --screenshot-delay 1.15 \
    > "$output/native.log" 2>&1
for kind in Slabs Brick; do
    grep -q "kind=$kind red_preview=true contiguous_prefix=true terrain_after_block=unchanged held_no_restart=true duplicate_down=ignored release=muted" "$output/native.log"
done
grep -q 'complete=true cases=2 single_path_per_drag=true' "$output/native.log"
grep -q 'Application exited successfully' "$output/native.log"
python3 scripts/runtime_log_diagnostics.py "$output/native.log"
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Two tight-turn paving drags remain one contiguous prefix, show red, retain exact atlas bytes while held, ignore duplicate down and mute on release. GUI unchanged.\nArtifacts: %s\n' "$output"
