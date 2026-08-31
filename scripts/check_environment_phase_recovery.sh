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

matched_cases=0
for entry in "${cases[@]}"; do
    case_name="${entry%%:*}"
    family="${entry#*:}"
    if [[ -n "$case_filter" && "$case_filter" != "$case_name" ]]; then
        continue
    fi
    matched_cases=$((matched_cases + 1))

    stdout_log="$run_root/$case_name.stdout.log"
    stderr_log="$run_root/$case_name.stderr.log"
    extra_args=()
    if [[ "$case_name" == "local-light-scaling" ]]; then
        extra_args+=(--perf)
    fi
    RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_DIAGNOSTIC=1 \
        "$cargo_bin" run --quiet --release --manifest-path "$repo_root/Cargo.toml" -- \
        --hidden --mute --environment-lighting-test-scene "$case_name" \
        "${extra_args[@]}" --auto-exit "$auto_exit" \
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

    injected="[ENV_PHASE_RECOVERY] event=injected family=$family"
    retried="[ENV_PHASE_RECOVERY] event=retried family=$family"
    exit_marker="Application exited successfully"
    mapfile -t injected_matches < <(grep -nF "$injected" "$latest_log" || true)
    mapfile -t retried_matches < <(grep -nF "$retried" "$latest_log" || true)
    mapfile -t exit_matches < <(grep -nF "$exit_marker" "$latest_log" || true)
    if (( ${#injected_matches[@]} != 1 )); then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name expected one injection, observed=${#injected_matches[@]}" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi
    if (( ${#retried_matches[@]} != 1 )); then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name expected one retry, observed=${#retried_matches[@]}" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi
    if (( ${#exit_matches[@]} != 1 )); then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name expected one successful exit, observed=${#exit_matches[@]}" >&2
        tail -n 80 "$latest_log" >&2
        exit 1
    fi

    injected_line="${injected_matches[0]}"
    retried_line="${retried_matches[0]}"
    exit_line="${exit_matches[0]}"
    injected_line_number="${injected_line%%:*}"
    retried_line_number="${retried_line%%:*}"
    exit_line_number="${exit_line%%:*}"
    injected_frame="$(sed -nE 's/.* injected_frame=([0-9]+).*/\1/p' <<<"$injected_line")"
    retried_injected_frame="$(sed -nE 's/.* injected_frame=([0-9]+).*/\1/p' <<<"$retried_line")"
    retry_frame="$(sed -nE 's/.* retry_frame=([0-9]+).*/\1/p' <<<"$retried_line")"
    if [[ -z "$injected_frame" || -z "$retried_injected_frame" || -z "$retry_frame" ]]; then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name missing structured frame serials" >&2
        exit 1
    fi
    if (( injected_line_number >= retried_line_number || retried_line_number >= exit_line_number )); then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name invalid log order inject=$injected_line_number retry=$retried_line_number exit=$exit_line_number" >&2
        exit 1
    fi
    if (( retried_injected_frame != injected_frame || retry_frame != injected_frame + 1 )); then
        echo "[ENV_PHASE_RECOVERY_CHECK] FAIL case=$case_name expected next-frame retry injected=$injected_frame retried_injected=$retried_injected_frame retry=$retry_frame" >&2
        exit 1
    fi

    echo "[ENV_PHASE_RECOVERY_CHECK] PASS case=$case_name family=$family injected_frame=$injected_frame retry_frame=$retry_frame run_log=$latest_log"
done

if (( matched_cases == 0 )); then
    echo "[ENV_PHASE_RECOVERY_CHECK] FAIL unknown case filter: $case_filter" >&2
    exit 1
fi
