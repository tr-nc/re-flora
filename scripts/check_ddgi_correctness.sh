#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
auto_exit="${DDGI_CORRECTNESS_AUTO_EXIT:-60}"
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
capture_specs=(
    final:final-a
    final:final-b
    moment-visibility:moment-visibility
    exact-visibility:exact-visibility
    exact-irradiance:exact-irradiance
    unoccluded-irradiance:unoccluded-irradiance
    equal-weight-irradiance:equal-weight-irradiance
    raw-cage-irradiance:raw-cage-irradiance
)
analyzer="$repo_root/scripts/analyze_environment_irradiance_capture.py"
source "$repo_root/scripts/lib/capture_process_evidence.sh"
capture_rust_log="warn,re_flora::run_log_binding=info,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info"

print_command() {
    printf '%q ' "$@"
    printf '\n'
}

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
        unoccluded="$run_dir/${case_name}-spacing${spacing}-unoccluded-irradiance.rfirr"
        equal_weight="$run_dir/${case_name}-spacing${spacing}-equal-weight-irradiance.rfirr"
        raw_cage="$run_dir/${case_name}-spacing${spacing}-raw-cage-irradiance.rfirr"
        thresholds=()
        case "$case_name" in
            sealed)
                thresholds=(--max-luminance 0.00001 --max-reference-error-p99 0.00001)
                ;;
            portal)
                thresholds=(--min-luminance-p99 0.10 --max-reference-error-p99 0.01)
                ;;
            walls)
                # Runtime consumers intentionally use Moment visibility only. These bounds retain
                # the measured thin-wall leakage ceiling after the Full/Exact consumer was
                # removed; the exact view remains the fixed oracle, not the production path.
                if [[ "$spacing" == "32" ]]; then
                    thresholds=(--max-reference-error-p99 0.40)
                else
                    thresholds=(--max-reference-error-p99 0.375)
                fi
                ;;
        esac
        command=(
            cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
            --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
            --environment-lighting-test-scene "$case_name"
            --environment-probe-spacing-voxels "$spacing"
            --environment-irradiance-capture-target converged
            --auto-exit "$auto_exit"
        )
        if [[ -n "$terrain_hard_origin" ]]; then
            command+=(--ddgi-terrain-hard-origin "$terrain_hard_origin")
        fi
        capture_failed=false
        for capture_spec in "${capture_specs[@]}"; do
            view="${capture_spec%%:*}"
            suffix="${capture_spec#*:}"
            path="$run_dir/${case_name}-spacing${spacing}-${suffix}.rfirr"
            console="${path%.rfirr}.console.log"
            echo "[DDGI_CORRECTNESS] case=$case_name spacing=$spacing backend=ddgi view=$view"
            if $dry_run; then
                print_command "${command[@]}" --ddgi-debug-view "$view" \
                    --environment-irradiance-capture "$path"
                continue
            fi
            if ! run_capture_with_process_evidence \
                "$console" "$path" "$capture_rust_log" \
                --require-test-scene-startup -- \
                "${command[@]}" --ddgi-debug-view "$view" \
                    --environment-irradiance-capture "$path"; then
                echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing view=$view process evidence" >&2
                failures=$((failures + 1))
                capture_failed=true
            fi
        done
        visibility_thresholds=()
        debug_difference_thresholds=()
        if [[ "$case_name" == "walls" ]]; then
            visibility_thresholds=(--min-reference-error-p99 0.01)
            debug_difference_thresholds=(--min-reference-error-p99 0.01)
        fi
        final_analysis=(
            "$analyzer" "$first"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view final
            --reference "$exact_irradiance"
            "${thresholds[@]}"
        )
        stability_analysis=()
        if [[ "$case_name" == "walls" ]]; then
            # The converged DDGI result is numerically stable, while temporal direct-light
            # capture planes are not cross-process bit exact. Compare the production
            # environment output numerically instead of treating unrelated plane hashes as
            # the DDGI determinism contract.
            stability_analysis=(
                "$analyzer" "$first"
                --correctness
                --expect-version 8
                --require-nonnegative-rgb
                --expect-debug-view final
                --reference "$second"
                --max-reference-error-p99 0.00001
                --max-reference-error-max 0.00001
            )
        else
            final_analysis+=(--compare "$second")
        fi
        visibility_analysis=(
            "$analyzer" "$moment"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view moment-visibility
            --reference "$exact_visibility"
            "${visibility_thresholds[@]}"
        )
        exact_visibility_analysis=(
            "$analyzer" "$exact_visibility"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view exact-visibility
        )
        exact_irradiance_analysis=(
            "$analyzer" "$exact_irradiance"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view exact-irradiance
        )
        unoccluded_analysis=(
            "$analyzer" "$unoccluded"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view unoccluded-irradiance
            --reference "$first"
            "${debug_difference_thresholds[@]}"
        )
        equal_weight_analysis=(
            "$analyzer" "$equal_weight"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view equal-weight-irradiance
            --reference "$unoccluded"
            "${debug_difference_thresholds[@]}"
        )
        raw_cage_analysis=(
            "$analyzer" "$raw_cage"
            --correctness
            --expect-version 8
            --require-nonnegative-rgb
            --expect-debug-view raw-cage-irradiance
            --reference "$equal_weight"
            "${debug_difference_thresholds[@]}"
        )
        if $dry_run; then
            print_command "${final_analysis[@]}"
            if (( ${#stability_analysis[@]} != 0 )); then
                print_command "${stability_analysis[@]}"
            fi
            print_command "${visibility_analysis[@]}"
            print_command "${exact_visibility_analysis[@]}"
            print_command "${exact_irradiance_analysis[@]}"
            print_command "${unoccluded_analysis[@]}"
            print_command "${equal_weight_analysis[@]}"
            print_command "${raw_cage_analysis[@]}"
            continue
        fi
        if $capture_failed; then
            continue
        fi
        if [[ ! -f "$first" || ! -f "$second" || ! -f "$moment" || \
              ! -f "$exact_visibility" || ! -f "$exact_irradiance" || \
              ! -f "$unoccluded" || ! -f "$equal_weight" || ! -f "$raw_cage" ]]; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing missing capture; backend likely never became ready" >&2
            failures=$((failures + 1))
            continue
        fi
        if ! "${final_analysis[@]}"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing invalid, nondeterministic, or incompatible irradiance capture" >&2
            failures=$((failures + 1))
            continue
        fi
        if (( ${#stability_analysis[@]} != 0 )) && \
           ! "${stability_analysis[@]}"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing numerically unstable production irradiance" >&2
            failures=$((failures + 1))
            continue
        fi
        if ! "${visibility_analysis[@]}"; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing invalid or incompatible visibility capture" >&2
            failures=$((failures + 1))
            continue
        fi
        debug_route_failed=false
        if ! "${exact_visibility_analysis[@]}"; then debug_route_failed=true; fi
        if ! "${exact_irradiance_analysis[@]}"; then debug_route_failed=true; fi
        if ! "${unoccluded_analysis[@]}"; then debug_route_failed=true; fi
        if ! "${equal_weight_analysis[@]}"; then debug_route_failed=true; fi
        if ! "${raw_cage_analysis[@]}"; then debug_route_failed=true; fi
        if $debug_route_failed; then
            echo "[DDGI_CORRECTNESS] FAIL case=$case_name spacing=$spacing debug route acceptance" >&2
            failures=$((failures + 1))
            continue
        fi
        echo "[DDGI_CORRECTNESS] PASS case=$case_name spacing=$spacing visibility_and_debug_routes"
    done
done

if $dry_run; then
    echo "[DDGI_CORRECTNESS] dry-run matrix cases=3 spacings=2 views=${#capture_specs[@]}"
    exit 0
fi

echo "[DDGI_CORRECTNESS] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
