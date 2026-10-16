#!/usr/bin/env bash
# A controlled native garden tests replaceable endpoint candidates without retreat.
# Ordinary pointer scripts keep natural stepped soil; no OS-mouse/perf claim.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/pavement-recovery"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
RE_FLORA_PAVEMENT_REVIEW=recover RUST_LOG=info target/release/re-flora \
    --hidden --mute --auto-exit 20 \
    --screenshot player-default "$output/intent.png" --screenshot-delay 1.15 \
    > "$output/native.log" 2>&1
for kind in Slabs Brick; do
    grep -q "kind=$kind intent_origin=retained invalid_tail=red paused_atlas=exact retry=valid_target no_retreat=true same_gesture=true contiguous_courses=true baked_prefix=exact" "$output/native.log"
done
grep -q 'dirt_candidate=published surface_sampling=retained atlas=unchanged' "$output/native.log"
grep -q 'protected_targets=discarded live_atlas_guard=retained rejected_atlas=exact clear_target=published fixed_history=valid_only no_retreat=true same_gesture=true' "$output/native.log"
grep -q 'garden=flat fixture_only=true ordinary_pointer_garden=unchanged' "$output/native.log"
grep -q 'complete=true cases=2 settings_restored=true' "$output/native.log"
grep -q 'Application exited successfully' "$output/native.log"
python3 scripts/runtime_log_diagnostics.py "$output/native.log"
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Two held paving paths discard rejected endpoints, retry a direct target without retreat, continue through a quarter bend, preserve earlier voxels/frozen width, and mute on release. GUI unchanged.\nArtifacts: %s\n' "$output"
