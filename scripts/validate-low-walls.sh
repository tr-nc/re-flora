#!/usr/bin/env bash
# Native correctness and candidate capture, NOT OS-mouse/listening/perf acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/low-walls"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
binary="$PWD/target/release/re-flora"
export RUST_LOG=info
RE_FLORA_LOW_WALL_REVIEW=seed "$binary" --hidden --mute --auto-exit 1 \
    --terrain-save "$output/placed.rflterrain" > "$output/seed.log" 2>&1
grep -q 'seed=verified three_materials=true two_heights=true saved_recipe_route=true' "$output/seed.log"
grep -q 'deep_bedrock=byte_identical wall_guard=full_mesh_plus_shell' "$output/seed.log"
for kind in Earth Stone Brick; do
    grep -q "kind=$kind upper_obstacle=protected rejected_atlas=exact clear_target=recovered same_gesture=true" "$output/seed.log"
    for height in 0.08 0.16; do
        grep -q "kind=$kind height=$height.*complete=true frozen_height=true same_gesture_retrace=atlas_identical inside_fill=false" "$output/seed.log"
        grep -q "kind=$kind height=$height sound=dig_shared release=muted cancel=muted idle_cancel=preserved" "$output/seed.log"
    done
done
for kind in Stone Brick; do
    for height in 0.08 0.16; do
        grep -q "kind=$kind height=$height fresh_retrace=atlas_identical" "$output/seed.log"
        grep -q "kind=$kind height=$height internal_half_brick_seams=none checked=" "$output/seed.log"
    done
done
RE_FLORA_LOW_WALL_REVIEW=view "$binary" --hidden --mute --auto-exit 1 \
    --terrain-load "$output/placed.rflterrain" \
    --screenshot player-default "$output/candidates.png" --screenshot-delay 0.2 \
    > "$output/reload.log" 2>&1
grep -q 'restart=verified cells=' "$output/reload.log"
grep -q 'view=candidates camera_transient=true' "$output/reload.log"
snapshot_before=$(sha256sum "$output/placed.rflterrain")
RE_FLORA_LOW_WALL_REVIEW=dig "$binary" --hidden --mute --auto-exit 0.5 \
    --terrain-load "$output/placed.rflterrain" > "$output/dig.log" 2>&1
grep -q 'dig=verified three_materials=true snapshot_file=unchanged' "$output/dig.log"
[[ "$snapshot_before" == "$(sha256sum "$output/placed.rflterrain")" ]]
for name in seed reload dig; do
    grep -q 'Application exited successfully' "$output/$name.log"
    python3 scripts/runtime_log_diagnostics.py "$output/$name.log"
done
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Three real wall materials, two heights, complete layers, frozen recipes, upper-obstacle protection/recovery, retrace, shared Dig audio lifecycle, restart, Dig and unchanged GUI passed.\nArtifacts: %s\n' "$output"
