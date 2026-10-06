#!/usr/bin/env bash
# Native visual/correctness evidence, not a performance or artistic acceptance test.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=2
export RUST_LOG="warn,re_flora::run_summary=info,re_flora::run_log_binding=info,re_flora::app::core=info,re_flora::tracer::model_mesh_frame=info"
out="target/view-quantization/native"
mkdir -p "$out"
config_before="$(sha256sum config/gui.toml)"
cargo build --release

run() {
    local mode="$1"; shift
    local label="${REVIEW_LOG_LABEL:-$mode}"
    RE_FLORA_MODEL_VIEW_REVIEW="$mode" RE_FLORA_MODEL_SURFACE_REVIEW=off env -u WAYLAND_DISPLAY \
        flock --close /tmp/re-flora-summer-gpu.lock \
        cargo run --release -- --hidden --mute --windowed "$@" >"$out/$label.stdout.log" 2>&1
    local latest
    latest="$(cargo run --release -- --latest-log)"
    cp "$latest" "$out/$label.run.log"
    printf '%s\n' "$latest" >"$out/$label.log-path.txt"
    if grep -En 'ERROR|panicked|VUID-|Validation Error' "$out/$label.run.log"; then
        echo "Native validation failed: $label" >&2; exit 1
    fi
    grep -q 'SHUTDOWN.*phase=complete failures=0' "$out/$label.run.log"
    for object in flower_heads attached_apples mesh_butterflies; do
        grep -q "MODEL_VIEW_DRAW.*object=$object .*bank_binding=19" "$out/$label.run.log"
    done
}

for mode in 32 128 256; do
    run "$mode" --screenshot player-default "$out/$mode.png" --screenshot-delay 2 --auto-exit 4
    test -s "$out/$mode.png"
done
for count in 32 128 256; do
    grep -q "MODEL_VIEW_QUANTIZATION.*count=$count " "$out/$count.run.log"
done

REVIEW_LOG_LABEL=32-repeat run 32 --screenshot player-default "$out/32-repeat.png" --screenshot-delay 2 --auto-exit 4

# A frozen production-mesh butterfly against sky, away from tree/HUD/wind.
# Compare binary foreground COVERAGE (not RGB/AA quality or whole-scene noise).
# An ignored count makes these masks identical and fails.
read -r width height < <(magick identify -format '%w %h\n' "$out/32.png")
x=$((width * 31 / 100)); y=$((height * 7 / 100))
w=$((width * 11 / 100)); h=$((height * 10 / 100))
for mode in 32 32-repeat 128 256; do
    magick "$out/$mode.png" -crop "${w}x${h}+${x}+${y}" +repage \
        -fx 'r>b-0.12?1:0' -depth 8 gray:- >"$out/$mode.butterfly-mask.raw"
done
cmp -s "$out/32.butterfly-mask.raw" "$out/32-repeat.butterfly-mask.raw"
! cmp -s "$out/32.butterfly-mask.raw" "$out/128.butterfly-mask.raw"
! cmp -s "$out/32.butterfly-mask.raw" "$out/256.butterfly-mask.raw"
! cmp -s "$out/128.butterfly-mask.raw" "$out/256.butterfly-mask.raw"
sha256sum "$out"/*.butterfly-mask.raw >"$out/foreground-masks.sha256"

# One live process changes 32/128/256/8/512, rotated poses, camera orbit and two
# post-submission native resizes. Existing screenshot sequence retains swapchain
# frames. It does not freeze environmental lighting or claim pixel-perfect RGB.
run sweep --screenshot player-default "$out/sweep" --screenshot-delay 0 \
    --screenshot-sequence 36 0.25 --auto-exit 15
grep -q 'MODEL_VIEW_REVIEW.*complete=true' "$out/sweep.run.log"
grep -q 'MODEL_VIEW_DRAW.*object=dynamic_apples .*bank_binding=19' "$out/sweep.run.log"
for count in 8 32 128 256 512; do
    grep -q "MODEL_VIEW_QUANTIZATION.*count=$count " "$out/sweep.run.log"
done
test "$(grep -c 'MODEL_VIEW_REVIEW_RESIZE' "$out/sweep.run.log")" = 2
for extent in 1023x767 1280x720; do
    grep -q "RESIZE.*published generation=.*extent=$extent tracer_generation=" "$out/sweep.run.log"
done
test "$config_before" = "$(sha256sum config/gui.toml)"
sha256sum "$out"/*.png >"$out/screenshots.sha256"
echo "Native model-view evidence ready: $out (art/motion still need user review)"
