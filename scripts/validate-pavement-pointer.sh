#!/usr/bin/env bash
# Actual Winit routing + native frames, NOT OS-mouse/listening/performance acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/pavement-pointer"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
RE_FLORA_PAVEMENT_REVIEW=pointer RUST_LOG=info target/release/re-flora \
    --hidden --mute --auto-exit 6 \
    --screenshot player-default "$output/drag.png" --screenshot-delay 0.35 \
    > "$output/native.log" 2>&1
for kind in Slabs Brick; do
    grep -q "kind=$kind drag=verified window_events=true preview_retained=true along_ground=true" "$output/native.log"
done
grep -q 'complete=true cases=2 settings_restored=true' "$output/native.log"
grep -q 'Application exited successfully' "$output/native.log"
! grep -Eq ' ERROR |panicked|VUID-|DEVICE_LOST' "$output/native.log"
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Both paving materials retain their drag preview, extend along ground and mute on release; GUI unchanged.\nArtifacts: %s\n' "$output"
