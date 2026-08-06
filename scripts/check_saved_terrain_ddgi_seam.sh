#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
    shift
fi

if [[ "${1:-}" == -* ]]; then
    printf 'usage: %s [--dry-run] [output-directory]\n' "$0" >&2
    exit 2
fi

output_dir="${1:-target/ddgi-seam-repro/single-exact}"
terrain_path="${TERRAIN_SNAPSHOT_PATH:-saves/terrain_snapshot.rflterrain}"
screenshot_delay="${SAVED_TERRAIN_SCREENSHOT_DELAY:-10}"
auto_exit="${SAVED_TERRAIN_AUTO_EXIT:-18}"
debug_view="${SAVED_TERRAIN_DDGI_DEBUG_VIEW:-exact-irradiance}"

case "$debug_view" in
    exact-irradiance|spatial-weight-current|spatial-weight-nominal|\
    spatial-weight-wrap|spatial-weight-nominal-wrap)
        ;;
    *)
        printf '[SAVED_DDGI_SEAM_CHECK] verdict=INVALID_VIEW view=%s\n' \
            "$debug_view" >&2
        exit 2
        ;;
esac

screenshot="$output_dir/$debug_view.png"
crop="$output_dir/$debug_view-crop.png"
app_stdout="$output_dir/app.stdout.log"
log_path=""

run_command=(
    cargo run --release --
    --hidden --mute --no-god-rays
    --terrain-load "$terrain_path"
    --ddgi-debug-view "$debug_view"
    --environment-probe-visualization
    --screenshot snapshot "$screenshot"
    --screenshot-delay "$screenshot_delay"
    --auto-exit "$auto_exit"
)
crop_command=(
    magick "$screenshot"
    -gravity center -crop 55%x68%+0+0 +repage "$crop"
)

print_command() {
    local prefix="$1"
    shift
    printf '%s' "$prefix"
    printf ' %q' "$@"
    printf '\n'
}

print_paths() {
    printf '[SAVED_DDGI_SEAM_CHECK] capture=%s crop=%s app_stdout=%s log=%s\n' \
        "$screenshot" "$crop" "$app_stdout" "${log_path:-unavailable}"
}

if $dry_run; then
    print_command '[SAVED_DDGI_SEAM_CHECK] run' "${run_command[@]}"
    print_command '[SAVED_DDGI_SEAM_CHECK] crop' "${crop_command[@]}"
    printf '[SAVED_DDGI_SEAM_CHECK] analyze %q %q\n' \
        "$repo_root/scripts/analyze_saved_ddgi_seam.py" "$crop"
    exit 0
fi

if [[ ! -f "$terrain_path" ]]; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=INPUT_MISSING terrain=%s\n' "$terrain_path" >&2
    exit 2
fi

if ! command -v magick >/dev/null 2>&1; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=SCREENSHOT_FAILED reason=missing-magick\n' >&2
    exit 4
fi

mkdir -p "$output_dir"
print_paths
print_command '[SAVED_DDGI_SEAM_CHECK] run' "${run_command[@]}"

if ! "${run_command[@]}" >"$app_stdout" 2>&1; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=APP_FAILED\n' >&2
    print_paths >&2
    tail -n 40 "$app_stdout" >&2 || true
    exit 3
fi

log_path="$(sed -n 's/.*Run log saved to //p' "$app_stdout" | tail -n 1)"
print_paths

if [[ -z "$log_path" || ! -f "$log_path" ]]; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=APP_FAILED reason=missing-run-log\n' >&2
    exit 3
fi

if rg -n 'ERROR|panic' "$app_stdout" "$log_path" >/dev/null 2>&1; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=APP_FAILED reason=error-or-panic\n' >&2
    rg -n 'ERROR|panic' "$app_stdout" "$log_path" >&2 || true
    exit 3
fi

if [[ ! -s "$screenshot" ]]; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=SCREENSHOT_FAILED reason=missing-capture\n' >&2
    exit 4
fi

if ! "${crop_command[@]}"; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=SCREENSHOT_FAILED reason=crop-failed\n' >&2
    exit 4
fi

if [[ ! -s "$crop" ]]; then
    printf '[SAVED_DDGI_SEAM_CHECK] verdict=SCREENSHOT_FAILED reason=missing-crop\n' >&2
    exit 4
fi

set +e
analysis_output="$(python3 "$repo_root/scripts/analyze_saved_ddgi_seam.py" "$crop" 2>&1)"
analysis_status=$?
set -e
printf '%s\n' "$analysis_output"

case "$analysis_status" in
    0)
        printf '[SAVED_DDGI_SEAM_CHECK] verdict=GREEN\n'
        exit 0
        ;;
    1)
        printf '[SAVED_DDGI_SEAM_CHECK] verdict=RED\n'
        exit 1
        ;;
    *)
        printf '[SAVED_DDGI_SEAM_CHECK] verdict=ANALYSIS_FAILED status=%s\n' "$analysis_status" >&2
        exit 5
        ;;
esac
