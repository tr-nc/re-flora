#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
    shift
fi

output_dir="${1:-target/ddgi-seam-repro/saved-terrain}"
terrain_path="${TERRAIN_SNAPSHOT_PATH:-saves/terrain_snapshot.rflterrain}"
snapshot_name="snapshot"
screenshot_delay="${SAVED_TERRAIN_SCREENSHOT_DELAY:-10}"
auto_exit="${SAVED_TERRAIN_AUTO_EXIT:-18}"

normal="$output_dir/normal.png"
exact_irradiance="$output_dir/exact-irradiance.png"
dominant_probe="$output_dir/dominant-probe.png"
normal_crop="$output_dir/normal-crop.png"
exact_irradiance_crop="$output_dir/exact-irradiance-crop.png"
dominant_probe_crop="$output_dir/dominant-probe-crop.png"

if ! $dry_run; then
    mkdir -p "$output_dir"
fi

run_capture() {
    local debug_view="$1"
    local output_path="$2"
    shift 2
    local command=(
        cargo run --release --
        --hidden --mute --no-god-rays
        --terrain-load "$terrain_path"
        "$@"
        --screenshot "$snapshot_name" "$output_path"
        --screenshot-delay "$screenshot_delay"
        --auto-exit "$auto_exit"
    )
    if [[ -n "$debug_view" ]]; then
        command+=(--ddgi-debug-view "$debug_view")
    fi
    if $dry_run; then
        printf '[SAVED_TERRAIN_PROBE_REPRO]'
        printf ' %q' "${command[@]}"
        printf '\n'
    else
        "${command[@]}"
        test -s "$output_path"
    fi
}

crop_capture() {
    local source_path="$1"
    local output_path="$2"
    local command=(magick "$source_path" -gravity center -crop 55%x68%+0+0 +repage "$output_path")
    if $dry_run; then
        printf '[SAVED_TERRAIN_PROBE_REPRO]'
        printf ' %q' "${command[@]}"
        printf '\n'
    else
        command magick "$source_path" -gravity center -crop 55%x68%+0+0 +repage "$output_path"
        test -s "$output_path"
    fi
}

if [[ ! -f "$terrain_path" && "$dry_run" == false ]]; then
    printf '[SAVED_TERRAIN_PROBE_REPRO] missing terrain snapshot: %s\n' "$terrain_path" >&2
    exit 2
fi

# The screenshot preset owns and applies the only saved camera snapshot. The loaded terrain is
# authoritative; no procedural test scene or terrain edit is involved. Hand is the default tool,
# so the blue terrain-edit radius is absent from the normal capture.
run_capture "" "$normal"
run_capture exact-irradiance "$exact_irradiance" --environment-probe-visualization
run_capture dominant-probe "$dominant_probe"

crop_capture "$normal" "$normal_crop"
crop_capture "$exact_irradiance" "$exact_irradiance_crop"
crop_capture "$dominant_probe" "$dominant_probe_crop"

printf '[SAVED_TERRAIN_PROBE_REPRO] normal=%s\n' "$normal"
printf '[SAVED_TERRAIN_PROBE_REPRO] exact_irradiance=%s\n' "$exact_irradiance"
printf '[SAVED_TERRAIN_PROBE_REPRO] dominant_probe=%s\n' "$dominant_probe"
printf '[SAVED_TERRAIN_PROBE_REPRO] normal_crop=%s\n' "$normal_crop"
printf '[SAVED_TERRAIN_PROBE_REPRO] exact_irradiance_crop=%s\n' "$exact_irradiance_crop"
printf '[SAVED_TERRAIN_PROBE_REPRO] dominant_probe_crop=%s\n' "$dominant_probe_crop"
