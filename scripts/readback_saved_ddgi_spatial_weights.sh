#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

output_path="${1:-target/ddgi-seam-repro/phase2/ddgi-spatial-weight-readback.txt}"
terrain_path="${TERRAIN_SNAPSHOT_PATH:-saves/terrain_snapshot.rflterrain}"
auto_exit="${SAVED_TERRAIN_AUTO_EXIT:-18}"
output_dir="$(dirname "$output_path")"
app_stdout="$output_dir/ddgi-spatial-weight-readback.stdout.log"

if [[ ! -f "$terrain_path" ]]; then
    printf '[DDGI_SPATIAL_WEIGHT_READBACK] missing terrain snapshot: %s\n' "$terrain_path" >&2
    exit 2
fi

mkdir -p "$output_dir"
command=(
    cargo run --release --
    --hidden --mute --no-god-rays
    --terrain-load "$terrain_path"
    --camera-snapshot snapshot
    --ddgi-debug-view spatial-weight-readback
    --ddgi-spatial-weight-readback "$output_path"
    --auto-exit "$auto_exit"
)
printf '[DDGI_SPATIAL_WEIGHT_READBACK] run'
printf ' %q' "${command[@]}"
printf '\n'

if ! "${command[@]}" >"$app_stdout" 2>&1; then
    printf '[DDGI_SPATIAL_WEIGHT_READBACK] verdict=APP_FAILED\n' >&2
    tail -n 50 "$app_stdout" >&2 || true
    exit 3
fi

log_path="$(sed -n 's/.*Run log saved to //p' "$app_stdout" | tail -n 1)"
if [[ -z "$log_path" || ! -f "$log_path" ]]; then
    printf '[DDGI_SPATIAL_WEIGHT_READBACK] verdict=APP_FAILED reason=missing-run-log\n' >&2
    exit 3
fi
if rg -n 'ERROR|panic' "$app_stdout" "$log_path" >/dev/null 2>&1; then
    printf '[DDGI_SPATIAL_WEIGHT_READBACK] verdict=APP_FAILED reason=error-or-panic\n' >&2
    rg -n 'ERROR|panic' "$app_stdout" "$log_path" >&2 || true
    exit 3
fi
if [[ ! -s "$output_path" ]]; then
    printf '[DDGI_SPATIAL_WEIGHT_READBACK] verdict=READBACK_FAILED reason=missing-output\n' >&2
    exit 4
fi

printf '[DDGI_SPATIAL_WEIGHT_READBACK] verdict=GREEN output=%s log=%s stdout=%s\n' \
    "$output_path" "$log_path" "$app_stdout"
