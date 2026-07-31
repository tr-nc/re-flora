#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
backend="${DDGI_CORRECTNESS_BACKEND:-ddgi}"
auto_exit="${DDGI_CORRECTNESS_AUTO_EXIT:-30}"
output_root="${DDGI_CORRECTNESS_OUTPUT_DIR:-$repo_root/target/ddgi-correctness}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi

cases=(sealed portal walls)
spacings=(32 16)

if ! $dry_run; then
    mkdir -p "$run_dir"
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

failures=0
for case_name in "${cases[@]}"; do
    for spacing in "${spacings[@]}"; do
        first="$run_dir/${case_name}-spacing${spacing}-a.rfirr"
        second="$run_dir/${case_name}-spacing${spacing}-b.rfirr"
        command=(
            cargo run --release --manifest-path "$repo_root/Cargo.toml" --
            --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
            --environment-lighting-test-scene "$case_name"
            --environment-lighting-backend "$backend"
            --environment-probe-spacing-voxels "$spacing"
            --auto-exit "$auto_exit"
        )
        if $dry_run; then
            printf '%q ' "${command[@]}" --environment-irradiance-capture "$first"
            printf '\n'
            printf '%q ' "${command[@]}" --environment-irradiance-capture "$second"
            printf '\n'
            continue
        fi

        echo "[DDGI_CORRECTNESS] case=$case_name spacing=$spacing backend=$backend run=a"
        "${command[@]}" --environment-irradiance-capture "$first"
        echo "[DDGI_CORRECTNESS] case=$case_name spacing=$spacing backend=$backend run=b"
        "${command[@]}" --environment-irradiance-capture "$second"
        if [[ ! -f "$first" || ! -f "$second" ]]; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing missing capture; backend likely never became ready" >&2
            failures=$((failures + 1))
            continue
        fi
        if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
            "$first" --compare "$second"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing invalid or nondeterministic capture" >&2
            failures=$((failures + 1))
            continue
        fi
        echo "[DDGI_CORRECTNESS] PASS case=$case_name spacing=$spacing capture_and_determinism"
    done
done

if $dry_run; then
    echo "[DDGI_CORRECTNESS] dry-run matrix cases=3 spacings=2 repetitions=2"
    exit 0
fi

echo "[DDGI_CORRECTNESS] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
