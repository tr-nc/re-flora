#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

auto_exit="${DDGI_TRANSPORT_ACCEPTANCE_AUTO_EXIT:-120}"
output_root="${DDGI_TRANSPORT_ACCEPTANCE_OUTPUT_DIR:-$repo_root/target/ddgi-transport-acceptance}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi

# Committed exact-gate calibration. These are correctness limits, not environment overrides;
# provenance and the tighter spacing-specific observations live in the companion document.
donor_max_s0_red_share=0.05
donor_min_s1_red_share_gain=0.065
donor_min_s1_luminance_gain=0.045
dogleg_max_s1_luminance_mean=0.00002
dogleg_min_s2_luminance_gain=0.00007
convergence_max_abs_delta=0.0025
convergence_max_rel_delta=0.02

spacings=(32 16)
donor_roi=(0.53125 0.4375 0.9375 0.8125 0.59375 0.9375)
dogleg_receiver_roi=(1.125 0.4375 0.5 1.3125 0.625 0.5)
analyzer="$repo_root/scripts/analyze_environment_irradiance_capture.py"
failures=0

echo "[DDGI_TRANSPORT] threshold_provenance=docs/ddgi_transport_acceptance.md"
echo "[DDGI_TRANSPORT] direct-sun-framebuffer=PROVEN seam=v5-direct-light-plane runner=check_ddgi_runtime_terrain_edits.sh"

if ! $dry_run; then
    mkdir -p "$run_dir"
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

print_command() {
    printf '%q ' "$@"
    printf '\n'
}

capture_path() {
    local case_name="$1"
    local spacing="$2"
    local target="$3"
    local order="$4"
    printf '%s/%s-spacing%s-%s-%s.rfirr' \
        "$run_dir" "$case_name" "$spacing" "$target" "$order"
}

run_capture() {
    local case_name="$1"
    local spacing="$2"
    local target="$3"
    local order="$4"
    local capture
    capture="$(capture_path "$case_name" "$spacing" "$target" "$order")"
    local console="${capture%.rfirr}.console.log"
    local command=(
        cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene "$case_name"
        --environment-probe-spacing-voxels "$spacing"
        --environment-irradiance-capture-target "$target"
        --ddgi-batch-order "$order"
        --ddgi-debug-view final
        --environment-irradiance-capture "$capture"
        --auto-exit "$auto_exit"
    )
    echo "[DDGI_TRANSPORT] capture case=$case_name spacing=$spacing target=$target order=$order"
    if $dry_run; then
        print_command "${command[@]}"
        return 0
    fi

    set +e
    RUST_LOG="warn,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
        "${command[@]}" 2>&1 | tee "$console"
    local command_status=${PIPESTATUS[0]}
    set -e
    if (( command_status != 0 )) || [[ ! -f "$capture" ]]; then
        echo "[DDGI_TRANSPORT] FAIL capture case=$case_name spacing=$spacing target=$target order=$order status=$command_status" >&2
        return 1
    fi
    if grep -Eiq '(^|[^[:alpha:]])(ERROR|panic|VUID-|validation error|stale readback)' "$console"; then
        echo "[DDGI_TRANSPORT] FAIL error marker case=$case_name spacing=$spacing target=$target order=$order" >&2
        return 1
    fi
}

run_analysis() {
    local label="$1"
    local capture="$2"
    shift 2
    local json="${capture%.rfirr}.analysis.json"
    local command=(
        "$analyzer" "$capture"
        --correctness
        --expect-version 5
        --require-nonnegative-rgb
        "$@"
    )
    echo "[DDGI_TRANSPORT] analyze label=$label json=$json"
    if $dry_run; then
        print_command "${command[@]}"
        return 0
    fi
    if ! "${command[@]}" | tee "$json"; then
        echo "[DDGI_TRANSPORT] FAIL analysis label=$label" >&2
        return 1
    fi
}

run_stage() {
    local case_name="$1"
    local spacing="$2"
    local target="$3"
    local order="$4"
    shift 4
    local capture
    capture="$(capture_path "$case_name" "$spacing" "$target" "$order")"
    if ! run_capture "$case_name" "$spacing" "$target" "$order"; then
        failures=$((failures + 1))
        return 1
    fi
    if ! run_analysis "$case_name-spacing$spacing-$target-$order" "$capture" \
        --expect-spacing-voxels "$spacing" \
        --expect-batch-order "$order" \
        "$@"; then
        failures=$((failures + 1))
        return 1
    fi
}

for spacing in "${spacings[@]}"; do
    run_stage sealed "$spacing" s0 forward \
        --expect-transport-stage seed-sky \
        --expect-transport-iteration 0 \
        --expect-publication-state unpublished \
        --require-zero-rgb || true
    run_stage sealed "$spacing" s1 forward \
        --expect-transport-stage single-bounce \
        --expect-transport-iteration 1 \
        --expect-source-stage seed-sky \
        --expect-source-iteration 0 \
        --expect-publication-state published \
        --require-zero-rgb || true
    run_stage sealed "$spacing" s2 forward \
        --expect-transport-stage feedback \
        --expect-transport-iteration 2 \
        --expect-source-stage single-bounce \
        --expect-source-iteration 1 \
        --expect-publication-state published \
        --require-zero-rgb || true
    run_stage sealed "$spacing" converged forward \
        --expect-transport-stage converged \
        --expect-publication-state published \
        --convergence-max-abs-delta "$convergence_max_abs_delta" \
        --convergence-max-rel-delta "$convergence_max_rel_delta" \
        --require-zero-rgb || true

    donor_s0="$(capture_path donor "$spacing" s0 forward)"
    run_stage donor "$spacing" s0 forward \
        --expect-transport-stage seed-sky \
        --expect-transport-iteration 0 \
        --expect-publication-state unpublished \
        --world-roi "${donor_roi[@]}" \
        --roi-channel red \
        --max-roi-channel-share "$donor_max_s0_red_share" \
        --max-exact-direct-sun-visibility 0 || true
    donor_s1="$(capture_path donor "$spacing" s1 forward)"
    run_stage donor "$spacing" s1 forward \
        --expect-transport-stage single-bounce \
        --expect-transport-iteration 1 \
        --expect-source-stage seed-sky \
        --expect-source-iteration 0 \
        --expect-publication-state published \
        --world-roi "${donor_roi[@]}" \
        --roi-channel red \
        --baseline "$donor_s0" \
        --min-roi-channel-share-gain "$donor_min_s1_red_share_gain" \
        --min-roi-luminance-gain "$donor_min_s1_luminance_gain" \
        --max-exact-direct-sun-visibility 0 || true
    donor_reverse="$(capture_path donor "$spacing" s1 reverse)"
    run_stage donor "$spacing" s1 reverse \
        --expect-transport-stage single-bounce \
        --expect-transport-iteration 1 \
        --expect-source-stage seed-sky \
        --expect-source-iteration 0 \
        --expect-publication-state published \
        --world-roi "${donor_roi[@]}" \
        --roi-channel red \
        --baseline "$donor_s0" \
        --min-roi-channel-share-gain "$donor_min_s1_red_share_gain" \
        --min-roi-luminance-gain "$donor_min_s1_luminance_gain" \
        --max-exact-direct-sun-visibility 0 \
        --compare "$donor_s1" || true

    dogleg_s1="$(capture_path dogleg "$spacing" s1 forward)"
    run_stage dogleg "$spacing" s1 forward \
        --expect-transport-stage single-bounce \
        --expect-transport-iteration 1 \
        --expect-source-stage seed-sky \
        --expect-source-iteration 0 \
        --expect-publication-state published \
        --world-roi "${dogleg_receiver_roi[@]}" \
        --max-roi-luminance-mean "$dogleg_max_s1_luminance_mean" \
        --max-exact-direct-sun-visibility 0 || true
    run_stage dogleg "$spacing" s2 forward \
        --expect-transport-stage feedback \
        --expect-transport-iteration 2 \
        --expect-source-stage single-bounce \
        --expect-source-iteration 1 \
        --expect-publication-state published \
        --world-roi "${dogleg_receiver_roi[@]}" \
        --baseline "$dogleg_s1" \
        --min-roi-luminance-gain "$dogleg_min_s2_luminance_gain" \
        --max-exact-direct-sun-visibility 0 || true

    if ! $dry_run && [[ -f "$donor_s0" ]]; then
        echo "[DDGI_TRANSPORT] evidence donor_s0=$donor_s0"
    fi
    if ! $dry_run && [[ -f "$donor_reverse" ]]; then
        echo "[DDGI_TRANSPORT] evidence donor_reverse=$donor_reverse"
    fi
done

run_child() {
    local script="$1"
    if $dry_run; then
        print_command "$script" --dry-run
    elif ! "$script"; then
        failures=$((failures + 1))
    fi
}

# Preserve the calibrated portal/walls exact-reference thresholds in the existing runner.
run_child "$repo_root/scripts/check_ddgi_correctness.sh"
run_child "$repo_root/scripts/check_ddgi_runtime_terrain_edits.sh"

lifecycle_runner="$repo_root/scripts/check_ddgi_lifecycle_acceptance.sh"
if [[ -x "$lifecycle_runner" ]]; then
    run_child "$lifecycle_runner"
elif $dry_run; then
    echo "[DDGI_TRANSPORT] lifecycle-runner=pending-integration path=$lifecycle_runner"
else
    echo "[DDGI_TRANSPORT] FAIL lifecycle runner missing: $lifecycle_runner" >&2
    failures=$((failures + 1))
fi

if $dry_run; then
    echo "[DDGI_TRANSPORT] dry-run complete spacings=2 sealed_stages=4 donor_stages=2 dogleg_stages=2 batch_orders=2"
    exit 0
fi

echo "[DDGI_TRANSPORT] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
