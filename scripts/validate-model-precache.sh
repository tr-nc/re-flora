#!/usr/bin/env bash
# Real native correctness/visual evidence; not artistic or performance acceptance.
set -euo pipefail
if [[ "${1:-}" == --help ]]; then
    echo 'Usage: scripts/validate-model-precache.sh'
    echo 'Build Release, capture the three independent cache modes, then exercise live switches/resolutions and resizes. Never saves GUI settings.'
    exit 0
fi
if [[ $# != 0 ]]; then echo 'Unexpected argument; use --help.' >&2; exit 2; fi
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=2
export RUST_LOG='warn,re_flora::app::core=info,re_flora::tracer::model_mesh_frame=info,re_flora::tracer::model_surface_cache=info'
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
export VK_LAYER_PATH="${VK_LAYER_PATH:-/home/terence/vulkan-sdk/x86_64/share/vulkan/explicit_layer.d}"
out=target/model-precache
mkdir -p "$out"
before=$(sha256sum config/gui.toml config/camera_snapshots.toml)
cargo build --release
run() {
    local label=$1; shift
    env -u WAYLAND_DISPLAY RE_FLORA_MODEL_VIEW_REVIEW=128 RE_FLORA_MODEL_SURFACE_REVIEW="$label" \
        flock --close /tmp/re-flora-summer-gpu.lock target/release/re-flora --hidden --mute --windowed "$@" >"$out/$label.stdout.log" 2>&1
    cp "$(target/release/re-flora --latest-log)" "$out/$label.run.log"
    if grep -En 'ERROR|panicked|VUID-|Validation Error' "$out/$label.run.log"; then exit 1; fi
    grep -q 'SHUTDOWN.*phase=complete failures=0' "$out/$label.run.log"
    for object in flower_heads attached_apples mesh_butterflies; do
        grep -q "MODEL_VIEW_DRAW.*object=$object " "$out/$label.run.log"
    done
}
for mode in off on apple butterfly flower; do
    run "$mode" --screenshot player-default "$out/$mode.png" --screenshot-delay 3 --auto-exit 5
    test -s "$out/$mode.png"
done
for object in flower_heads attached_apples mesh_butterflies; do
    grep -q "MODEL_VIEW_DRAW.*object=$object .*shader=cached_surface_cells" "$out/on.run.log"
    grep -q "MODEL_VIEW_DRAW.*object=$object .*shader=native_triangle" "$out/off.run.log"
done
run sweep --screenshot player-default "$out/sweep" --screenshot-delay 0 --screenshot-sequence 24 0.5 --auto-exit 16
for stage in 0 1 2 3 4 5 6 7 8; do grep -q "MODEL_SURFACE_REVIEW.*stage=$stage " "$out/sweep.run.log"; done
grep -q 'MODEL_VIEW_DRAW.*object=dynamic_apples .*shader=cached_surface_cells' "$out/sweep.run.log"
for extent in 1023x767 1280x720; do grep -q "RESIZE.*published generation=.*extent=$extent " "$out/sweep.run.log"; done
test "$before" = "$(sha256sum config/gui.toml config/camera_snapshots.toml)"
sha256sum "$out"/*.png > "$out/screenshots.sha256"
echo "Native model pre-cache evidence ready: $out (art/motion/performance not accepted by this check)"
