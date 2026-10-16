#!/usr/bin/env bash
# Production candidate/atlas/save validation, not OS-pointer or perf acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/path-loops"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
binary="$PWD/target/release/re-flora"
export RUST_LOG=info
RE_FLORA_LOW_WALL_REVIEW=loops "$binary" --hidden --mute --auto-exit 1 \
    --terrain-save "$output/rings.rflterrain" \
    --screenshot player-default "$output/rings.png" --screenshot-delay 0.3 \
    > "$output/seed.log" 2>&1
for kind in Earth Stone Brick; do
    grep -q "PATH_LOOP_REVIEW.*kind=$kind closed=true exact_seam=true even_bond=true preview_atlas=unchanged interior=unchanged" "$output/seed.log"
done
RE_FLORA_LOW_WALL_REVIEW=loops-verify "$binary" --hidden --mute --auto-exit 0.5 \
    --terrain-load "$output/rings.rflterrain" > "$output/reload.log" 2>&1
grep -q 'PATH_LOOP_REVIEW.*restart=verified cells=.* interiors=unchanged' "$output/reload.log"
for name in seed reload; do
    grep -q 'Application exited successfully' "$output/$name.log"
    python3 scripts/runtime_log_diagnostics.py "$output/$name.log"
done
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'PASS: three wall materials, circle/corner-start loops, exact seams, closed bonds, preview-only snapping, untouched interior, saved/reloaded voxels and clean logs.\nArtifacts: %s\n' "$output"
