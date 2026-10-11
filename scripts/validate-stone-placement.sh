#!/usr/bin/env bash
# Explicit, short native correctness checks; no visual or performance acceptance.
set -euo pipefail
cd "$(dirname "$0")/.."
output="$PWD/target/stone-placement"
mkdir -p "$output"
cargo build --release > "$output/build.log" 2>&1
before=$(sha256sum config/gui.toml)
export RUST_LOG=info
binary="$PWD/target/release/re-flora"
"$binary" --hidden --mute --auto-exit 0.5 > "$output/default.log" 2>&1
! grep -q '\[STONE_PREVIEW\]' "$output/default.log"
RE_FLORA_STONE_PLACEMENT_REVIEW=seed "$binary" --hidden --mute --auto-exit 0.5 \
    --terrain-save "$output/placed.rflterrain" > "$output/seed.log" 2>&1
grep -q '\[STONE_PLACEMENT\].*seed=42 next_seed=43' "$output/seed.log"
grep -q '\[STONE_PLACEMENT\].*seed=43 next_seed=44' "$output/seed.log"
RE_FLORA_STONE_PLACEMENT_REVIEW=verify "$binary" --hidden --mute --auto-exit 0.5 \
    --terrain-load "$output/placed.rflterrain" > "$output/reload.log" 2>&1
grep -q 'restart=verified terrain_bytes=exact rock_and_slab=true dig=verified' "$output/reload.log"
for file in default seed reload; do
    grep -q 'Application exited successfully' "$output/$file.log"
    ! grep -Eq ' ERROR |panicked|VUID-|DEVICE_LOST' "$output/$file.log"
done
[[ "$before" == "$(sha256sum config/gui.toml)" ]]
printf 'No startup preview; seed advancement, terrain restart, Dig and unchanged config passed.\nArtifacts: %s\n' "$output"
