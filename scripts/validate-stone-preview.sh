#!/usr/bin/env bash
# Native correctness/evidence, not a performance or art acceptance benchmark.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=2
export RUST_LOG='warn,re_flora::run_summary=info,re_flora::run_log_binding=info,re_flora::tracer::stone_preview=info,re_flora::app::core::stone_preview=info,re_flora::app::core::scene_supersampling=info'
output="$PWD/target/stone-native"
mkdir -p "$output"
cargo build --release > "$output/release-build.log" 2>&1
config_before=$(sha256sum config/gui.toml)

run_native() {
    local name=$1 mode=$2 status=0
    shift 2
    if [[ -n "$mode" ]]; then
        flock --close /tmp/re-flora-summer-gpu.lock env RE_FLORA_STONE_REVIEW="$mode" \
            cargo run --release -- --hidden --mute "$@" > "$output/$name.stdout.log" 2>&1 || status=$?
    else
        flock --close /tmp/re-flora-summer-gpu.lock env -u RE_FLORA_STONE_REVIEW \
            cargo run --release -- --hidden --mute "$@" > "$output/$name.stdout.log" 2>&1 || status=$?
    fi
    local latest
    latest=$(cargo run --release -- --latest-log 2>/dev/null)
    cp "$latest" "$output/$name.log"
    cargo run --release -- --tail-latest-log 20 > "$output/$name.tail.log" 2>&1
    if [[ "$status" != 0 ]] || grep -Eq ' ERROR |\[Validation\]|panicked|DEVICE_LOST' "$output/$name.log" "$output/$name.stdout.log"; then
        printf 'Native stone validation failed: %s\n' "$output/$name.log" >&2
        return 1
    fi
    grep -q 'Application exited successfully' "$output/$name.log"
    if [[ -n "$mode" ]]; then
        grep -q '\[STONE_PREVIEW\] enabled=true' "$output/$name.log"
    else
        if grep -q '\[STONE_PREVIEW\] enabled=true' "$output/$name.log"; then
            echo 'Default mode unexpectedly enabled stone preview' >&2
            return 1
        fi
    fi
    [[ "$config_before" == "$(sha256sum config/gui.toml)" ]]
}

run_native smoke '' --auto-exit 0.5
for mode in voxel-rock direct-rock voxel-slab direct-slab; do
    run_native "$mode" "$mode" --windowed \
        --screenshot player-default "$output/$mode.png" --screenshot-delay 2 --auto-exit 4
    test -s "$output/$mode.png"
    case "$mode" in
        voxel-*) grep -q 'kernel=model_voxelize surface=make_surface visibility=Contree' "$output/$mode.log" ;;
        direct-*) grep -q 'path=direct-triangle' "$output/$mode.log" ;;
    esac
done
for kind in rock slab; do
    voxel=$(grep '\[STONE_PREVIEW\] enabled=true' "$output/voxel-$kind.log" | head -1 | grep -o 'source=[0-9a-f]*')
    direct=$(grep '\[STONE_PREVIEW\] enabled=true' "$output/direct-$kind.log" | head -1 | grep -o 'source=[0-9a-f]*')
    [[ -n "$voxel" && "$voxel" == "$direct" ]]
done
run_native cycle cycle --windowed --auto-exit 10
for phase in {0..9}; do
    grep -q "\[STONE_REVIEW\].* phase=$phase " "$output/cycle.log"
done
grep -q '\[STONE_PREVIEW\] enabled=false' "$output/cycle.log"
grep -q 'ratio=16:1' "$output/cycle.log"
grep -q 'ratio=64:1' "$output/cycle.log"
printf 'Native dual paths, same-source hashes, disable/re-enable and extent cycles passed.\nArtifacts: %s\n' "$output"
