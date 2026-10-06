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
cp config/gui.toml "$output/gui-original.toml"
trap 'cp "$output/gui-original.toml" config/gui.toml' EXIT

run_native() {
    local name=$1 mode=$2 status=0
    local before
    before=$(sha256sum config/gui.toml)
    shift 2
    local launch=(cargo run --release --)
    if [[ "$name" == gui-reload ]]; then
        # Keep identical compiled fallback defaults; otherwise persistence tests
        # could pass because Cargo compiled the saved values into a new binary.
        launch=("$PWD/target/release/re-flora")
    fi
    if [[ -n "$mode" ]]; then
        flock --close /tmp/re-flora-summer-gpu.lock env RE_FLORA_STONE_REVIEW="$mode" \
            "${launch[@]}" --hidden --mute "$@" > "$output/$name.stdout.log" 2>&1 || status=$?
    else
        flock --close /tmp/re-flora-summer-gpu.lock env -u RE_FLORA_STONE_REVIEW \
            "${launch[@]}" --hidden --mute "$@" > "$output/$name.stdout.log" 2>&1 || status=$?
    fi
    local latest
    latest=$(target/release/re-flora --latest-log)
    cp "$latest" "$output/$name.log"
    target/release/re-flora --tail-latest-log 20 > "$output/$name.tail.log" 2>&1
    if [[ "$status" != 0 ]] || grep -Eq ' ERROR |\[Validation\]|panicked|DEVICE_LOST' "$output/$name.log" "$output/$name.stdout.log"; then
        printf 'Native stone validation failed: %s\n' "$output/$name.log" >&2
        return 1
    fi
    grep -q 'Application exited successfully' "$output/$name.log"
    if [[ -n "$mode" || "$name" == gui-reload ]]; then
        grep -q '\[STONE_PREVIEW\] enabled=true' "$output/$name.log"
    else
        if grep -q '\[STONE_PREVIEW\] enabled=true' "$output/$name.log"; then
            echo 'Default mode unexpectedly enabled stone preview' >&2
            return 1
        fi
    fi
    if [[ "$name" != gui-save ]]; then
        [[ "$before" == "$(sha256sum config/gui.toml)" ]]
    fi
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
# Aggregate integration: one shared view bank controls direct stones too, and
# global dither covers both adapters after composition. Same finite seed/pivot.
for style in 32 32-repeat 128 256 global combined; do
    mode=$style
    [[ "$style" == 32-repeat ]] && mode=32
    RE_FLORA_STONE_STYLE_REVIEW="$mode" run_native "style-$style" direct-rock --windowed \
        --screenshot player-default "$output/style-$style.png" --screenshot-delay 2 --auto-exit 4
done
for style in 32 32-repeat 128 256; do
    [[ "$(magick identify -format '%wx%h' "$output/style-$style.png")" == 1600x900 ]]
    # Upper rock silhouette against darker terrain; exclude animated flowers
    # below its base. Local coverage, not whole-frame RGB or an art score.
    magick "$output/style-$style.png" -crop 440x280+580+270 +repage \
        -fx 'r>0.58&&g>0.58&&b>0.58?1:0' -depth 8 gray:- > "$output/style-$style.mask"
done
cmp -s "$output/style-32.mask" "$output/style-32-repeat.mask"
! cmp -s "$output/style-32.mask" "$output/style-128.mask"
! cmp -s "$output/style-32.mask" "$output/style-256.mask"
! cmp -s "$output/style-128.mask" "$output/style-256.mask"
RE_FLORA_STONE_STYLE_REVIEW=cycle run_native style-cycle cycle --windowed --auto-exit 10
for count in 32 128 256; do
    grep -q "STONE_STYLE_REVIEW.*count=$count " "$output/style-cycle.log"
done
grep -q 'global=true bank_binding=19' "$output/style-cycle.log"
binary_before=$(sha256sum target/release/re-flora)
RE_FLORA_STONE_GUI_SAVE_REVIEW=1 RE_FLORA_DEBUG_PANEL_REVIEW=1 \
RE_FLORA_DEBUG_SEARCH_REVIEW='stone rendering' RUST_LOG="$RUST_LOG,re_flora::app::core=info" \
run_native gui-save direct-rock --windowed --screenshot player-default "$output/gui-save.png" \
    --screenshot-delay 2 --auto-exit 4
grep -q 'Config saved successfully' "$output/gui-save.log"
grep -q '\[STONE_GUI_REVIEW\].* input=release ' "$output/gui-save.log"
[[ "$config_before" != "$(sha256sum config/gui.toml)" ]]
cp config/gui.toml "$output/gui-native-saved.toml"
RE_FLORA_DEBUG_PANEL_REVIEW=1 RE_FLORA_DEBUG_SEARCH_REVIEW='stone rendering' \
run_native gui-reload '' --windowed --screenshot player-default "$output/gui-reload.png" \
    --screenshot-delay 2 --auto-exit 4
grep -q 'path=direct-triangle type=Rock seed=42' "$output/gui-reload.log"
[[ "$binary_before" == "$(sha256sum target/release/re-flora)" ]]
cp "$output/gui-original.toml" config/gui.toml
[[ "$config_before" == "$(sha256sum config/gui.toml)" ]]
printf 'Native dual paths, same-source hashes, disable/re-enable, extent cycles and GUI Search/Save/restart passed.\nArtifacts: %s\n' "$output"
