#!/usr/bin/env bash
# Opt-in real app correctness review, not a unit test or performance acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/pavement-prototype-native"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
export RUST_LOG=info
RE_FLORA_PAVEMENT_REVIEW=prototype target/release/re-flora \
    --flat-terrain --hidden --mute --auto-exit 0.5 \
    --terrain-save "$output/placed.rflterrain" > "$output/seed.log" 2>&1
for kind in Slabs Brick; do
    grep -q "kind=$kind published_retracted=true rejected_tail=true stationary=true branch=true empty_click_suppressed=true soil_and_completed_path=atlas_identical flora_count_and_identity=restored" "$output/seed.log"
done
grep -q 'backdrag_then_cancel=atlas_identical redraw_not_required=true sound=muted' "$output/seed.log"
grep -q 'smoothing_range=2 sharp_corner=published terrain_preflight=true' "$output/seed.log"
RE_FLORA_PAVEMENT_REVIEW=prototype-verify target/release/re-flora \
    --terrain-load "$output/placed.rflterrain" --hidden --mute --auto-exit 0.5 \
    > "$output/reload.log" 2>&1
grep -q 'restart=verified atlas_crc=.*flora_count=.*authored_identity_and_seed=true' "$output/reload.log"
for name in seed reload; do
    grep -q 'Application exited successfully' "$output/$name.log"
    ! grep -Eq ' ERROR |panicked|VUID-|DEVICE_LOST' "$output/$name.log"
done
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'Native prototype migration: real voxel/flora retraction, branch, stationary pointer, cancel, sharp corner and terrain/flora restart passed.\nArtifacts: %s\n' "$output"
