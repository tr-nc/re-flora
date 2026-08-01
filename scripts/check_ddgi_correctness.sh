#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
auto_exit="${DDGI_CORRECTNESS_AUTO_EXIT:-12}"
output_root="${DDGI_CORRECTNESS_OUTPUT_DIR:-$repo_root/target/ddgi-correctness}"
terrain_hard_origin="${DDGI_CORRECTNESS_TERRAIN_HARD_ORIGIN:-}"
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
        first="$run_dir/${case_name}-spacing${spacing}-final-a.rfirr"
        second="$run_dir/${case_name}-spacing${spacing}-final-b.rfirr"
        moment="$run_dir/${case_name}-spacing${spacing}-moment-visibility.rfirr"
        exact_visibility="$run_dir/${case_name}-spacing${spacing}-exact-visibility.rfirr"
        exact_irradiance="$run_dir/${case_name}-spacing${spacing}-exact-irradiance.rfirr"
        thresholds=()
        case "$case_name" in
            sealed)
                thresholds=(--max-luminance 0.00001 --max-reference-error-p99 0.00001)
                ;;
            portal)
                thresholds=(--min-luminance-p99 0.10 --max-reference-error-p99 0.01)
                ;;
            walls)
                if [[ "$spacing" == "32" ]]; then
                    thresholds=(--max-reference-error-p99 0.15)
                else
                    thresholds=(--max-reference-error-p99 0.133)
                fi
                ;;
        esac
        command=(
            cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
            --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
            --environment-lighting-test-scene "$case_name"
            --environment-probe-spacing-voxels "$spacing"
            --auto-exit "$auto_exit"
        )
        if [[ -n "$terrain_hard_origin" ]]; then
            command+=(--ddgi-terrain-hard-origin "$terrain_hard_origin")
        fi
        if $dry_run; then
            for view_and_path in \
                "final:$first" \
                "final:$second" \
                "moment-visibility:$moment" \
                "exact-visibility:$exact_visibility" \
                "exact-irradiance:$exact_irradiance"; do
                view="${view_and_path%%:*}"
                path="${view_and_path#*:}"
                printf '%q ' "${command[@]}" --ddgi-debug-view "$view" \
                    --environment-irradiance-capture "$path"
                printf '\n'
            done
            continue
        fi

        for view_and_path in \
            "final:$first" \
            "final:$second" \
            "moment-visibility:$moment" \
            "exact-visibility:$exact_visibility" \
            "exact-irradiance:$exact_irradiance"; do
            view="${view_and_path%%:*}"
            path="${view_and_path#*:}"
            echo "[DDGI_CORRECTNESS] case=$case_name spacing=$spacing backend=ddgi view=$view"
            RUST_LOG="warn,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
                "${command[@]}" --ddgi-debug-view "$view" \
                    --environment-irradiance-capture "$path"
        done
        if [[ ! -f "$first" || ! -f "$second" || ! -f "$moment" || \
              ! -f "$exact_visibility" || ! -f "$exact_irradiance" ]]; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing missing capture; backend likely never became ready" >&2
            failures=$((failures + 1))
            continue
        fi
        if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
            "$first" --compare "$second" --reference "$exact_irradiance" \
            "${thresholds[@]}"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing invalid, nondeterministic, or incompatible irradiance capture" >&2
            failures=$((failures + 1))
            continue
        fi
        if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
            "$moment" --reference "$exact_visibility"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing invalid or incompatible visibility capture" >&2
            failures=$((failures + 1))
            continue
        fi
        echo "[DDGI_CORRECTNESS] PASS case=$case_name spacing=$spacing capture_determinism_and_exact_reference"
    done
done

if $dry_run; then
    echo "[DDGI_CORRECTNESS] dry-run matrix cases=3 spacings=2 views=5"
    exit 0
fi

echo "[DDGI_CORRECTNESS] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
