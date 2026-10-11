#!/usr/bin/env bash
# Explicit native correctness fixture, not an OS-mouse or performance acceptance test.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/pavement"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
binary="$PWD/target/release/re-flora"
export RUST_LOG=info
"$binary" --hidden --mute --auto-exit 0.5 > "$output/default.log" 2>&1
! grep -q '\[PAVEMENT\].*publish' "$output/default.log"
RE_FLORA_PAVEMENT_REVIEW=seed "$binary" --hidden --mute --auto-exit 1 \
    --terrain-save "$output/placed.rflterrain" \
    --screenshot player-default "$output/paths.png" --screenshot-delay 0.2 \
    > "$output/seed.log" 2>&1
grep -q 'seed=verified three_surfaces=true' "$output/seed.log"
grep -q 'kind=Slabs.*retrace=no_extra_layer' "$output/seed.log"
grep -q 'kind=Brick.*retrace=no_extra_layer' "$output/seed.log"
grep -q 'kind=Dirt atlas_unchanged=true flora_removed=1' "$output/seed.log"
for kind in Slabs Brick Dirt; do
    grep -q "kind=$kind sound=dig_shared release=muted cancel=muted idle_cancel=preserved" "$output/seed.log"
done
RE_FLORA_PAVEMENT_REVIEW=view "$binary" --hidden --mute --auto-exit 1 \
    --terrain-load "$output/placed.rflterrain" \
    --screenshot player-default "$output/bends.png" --screenshot-delay 0.2 \
    > "$output/reload.log" 2>&1
grep -q 'restart=verified cells=' "$output/reload.log"
grep -q 'view=bends camera_transient=true' "$output/reload.log"
for kind in Slabs Brick; do
    for sign in 1 -1; do
        grep -q "PAVEMENT_BEND_REVIEW.*kind=$kind sign=$sign courses=6.*complete=true" "$output/seed.log"
        grep -q "PAVEMENT_BEND_REVIEW.*kind=$kind sign=$sign retrace=atlas_identical" "$output/seed.log"
    done
done
for name in default seed reload; do
    grep -q 'Application exited successfully' "$output/$name.log"
    ! grep -Eq ' ERROR |panicked|VUID-|DEVICE_LOST' "$output/$name.log"
done
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Pavement complete left/right bend courses, materials, exact bent retrace, dirt clearing, shared Dig sound lifecycle, restart and unchanged settings passed.\nArtifacts: %s\n' "$output"
