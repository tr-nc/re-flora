#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo_bin="${CARGO:-cargo}"
auto_exit="${RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_AUTO_EXIT:-0.75}"
case_filter="${RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_CASE:-}"
run_root="$repo_root/target/environment-phase-recovery"
latest_pointer="$repo_root/target/re-flora-logs/latest-run-log.txt"
cases=(
    "sealed:Static"
    "terrain-edits:Terrain"
    "radiance-changes:Radiance"
    "point-light-changes:PointLight"
    "voxel-emissive-changes:VoxelEmissive"
    "raster-emitter-changes:RasterEmitter"
    "multi-source-stress:MultiSource"
    "local-light-scaling:LocalLightScaling"
)

mkdir -p "$run_root"

for entry in "${cases[@]}"; do
    case_name="${entry%%:*}"
    family="${entry#*:}"
    if [[ -n "$case_filter" && "$case_filter" != "$case_name" ]]; then
        continue
    fi

    stdout_log="$run_root/$case_name.stdout.log"
    stderr_log="$run_root/$case_name.stderr.log"
    RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_DIAGNOSTIC=1 \
        "$cargo_bin" run --quiet --release --manifest-path "$repo_root/Cargo.toml" -- \
        --hidden --mute --environment-lighting-test-scene "$case_name" --auto-exit "$auto_exit" \
        >"$stdout_log" 2>"$stderr_log"

    if [[ ! -s "$latest_pointer" ]]; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing latest run-log pointer" >&2
        exit 1
    fi
    latest_log="$(<"$latest_pointer")"
    if [[ ! -f "$latest_log" ]]; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing run log: $latest_log" >&2
        exit 1
    fi

    injected="[ENV_PHASE_RECOVERY] injected family=$family"
    retried="[ENV_PHASE_RECOVERY] retried family=$family exact_payload=true exact_phase=true"
    if ! grep -Fq "$injected" "$latest_log"; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing injection evidence" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi
    if ! grep -Fq "$retried" "$latest_log"; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing exact retry evidence" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi
    if ! grep -Fq "Application exited successfully" "$latest_log"; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing successful exit" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi

    echo "[ENV_PHASE_RECOVERY_CHECK] PASS case=$case_name family=$family run_log=$latest_log"
done
