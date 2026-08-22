#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output_dir="${1:-target/bd2-blue-voxel}"
screenshot_delay="${BD2_SCREENSHOT_DELAY:-12}"
auto_exit="${BD2_AUTO_EXIT:-14}"
screenshot="$output_dir/final.png"
app_stdout="$output_dir/app.stdout.log"

mkdir -p "$output_dir"
run_command=(
    cargo run --release --
    --hidden --mute --house-scene
    --screenshot bd2 "$screenshot"
    --screenshot-delay "$screenshot_delay"
    --auto-exit "$auto_exit"
)

printf '[BD2_BLUE_VOXEL_CHECK] run'
printf ' %q' "${run_command[@]}"
printf '\n'
if ! "${run_command[@]}" >"$app_stdout" 2>&1; then
    printf '[BD2_BLUE_VOXEL_CHECK] verdict=APP_FAILED\n' >&2
    tail -n 40 "$app_stdout" >&2 || true
    exit 3
fi

log_path="$(sed -n 's/.*Run log saved to //p' "$app_stdout" | tail -n 1)"
if [[ -z "$log_path" || ! -f "$log_path" ]]; then
    printf '[BD2_BLUE_VOXEL_CHECK] verdict=APP_FAILED reason=missing-run-log\n' >&2
    exit 3
fi
if rg -n 'ERROR|panic' "$app_stdout" "$log_path" >/dev/null 2>&1; then
    printf '[BD2_BLUE_VOXEL_CHECK] verdict=APP_FAILED reason=error-or-panic\n' >&2
    rg -n 'ERROR|panic' "$app_stdout" "$log_path" >&2 || true
    exit 3
fi
if [[ ! -s "$screenshot" ]]; then
    printf '[BD2_BLUE_VOXEL_CHECK] verdict=APP_FAILED reason=missing-screenshot\n' >&2
    exit 3
fi

set +e
analysis_output="$(python3 scripts/analyze_bd2_blue_voxel.py "$screenshot" 2>&1)"
analysis_status=$?
set -e
printf '%s\n' "$analysis_output"
case "$analysis_status" in
    0)
        printf '[BD2_BLUE_VOXEL_CHECK] verdict=GREEN screenshot=%s log=%s\n' \
            "$screenshot" "$log_path"
        ;;
    1)
        printf '[BD2_BLUE_VOXEL_CHECK] verdict=RED screenshot=%s log=%s\n' \
            "$screenshot" "$log_path"
        ;;
    *)
        printf '[BD2_BLUE_VOXEL_CHECK] verdict=ANALYSIS_FAILED status=%s\n' \
            "$analysis_status" >&2
        exit 5
        ;;
esac
exit "$analysis_status"
