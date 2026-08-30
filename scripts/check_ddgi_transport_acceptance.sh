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
donor_min_e0_luminance_mean=0.045
dogleg_max_e0_luminance_mean=0.00002
# Epoch one retains 50% history under the sample-age cap, so the old unblended 0.00007 gate maps
# to a minimum blended gain of 0.000035.
dogleg_min_e1_luminance_gain=0.000035
convergence_max_abs_delta=0.0025
convergence_max_rel_delta=0.02
convergence_consecutive_epochs=2
convergence_minimum_epoch_count=8
convergence_max_epoch=63

spacings=(32 16)
donor_roi=(0.53125 0.4375 0.9375 0.8125 0.59375 0.9375)
dogleg_receiver_roi=(1.125 0.4375 0.5 1.3125 0.625 0.5)
analyzer="$repo_root/scripts/analyze_environment_irradiance_capture.py"
convergence_summarizer="$repo_root/scripts/summarize_ddgi_convergence.py"
process_validator="$repo_root/scripts/validate_capture_process_evidence.py"
failures=0
filter_history_outcome_accepted=true

echo "[DDGI_TRANSPORT] threshold_provenance=docs/ddgi_transport_acceptance.md"
echo "[DDGI_TRANSPORT] convergence_provenance=docs/ddgi_convergence_calibration.md"
echo "[DDGI_TRANSPORT] direct-sun-framebuffer=REQUIRED seam=v6-direct-light-plane runner=check_ddgi_runtime_terrain_edits.sh"
echo "[DDGI_TRANSPORT] filter-history-outcome=REQUIRED seam=dogleg-e0-e1-production-capture"

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
    RUST_LOG="warn,re_flora::run_log_binding=info,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
        "${command[@]}" 2>&1 | tee "$console"
    local command_status=${PIPESTATUS[0]}
    set -e
    if (( command_status != 0 )) || [[ ! -f "$capture" ]]; then
        echo "[DDGI_TRANSPORT] FAIL capture case=$case_name spacing=$spacing target=$target order=$order status=$command_status" >&2
        return 1
    fi
    if ! "$process_validator" "$console"; then
        echo "[DDGI_TRANSPORT] FAIL process evidence case=$case_name spacing=$spacing target=$target order=$order" >&2
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
        --expect-version 8
        --expect-debug-view final
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
    run_stage sealed "$spacing" e0 forward \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-publication-state published \
        --require-zero-rgb || true
    run_stage sealed "$spacing" e1 forward \
        --expect-lifecycle-state converging \
        --expect-update-epoch 1 \
        --expect-source-state converging \
        --expect-source-update-epoch 0 \
        --expect-publication-state published \
        --require-zero-rgb || true
    run_stage sealed "$spacing" converged forward \
        --expect-lifecycle-state converged \
        --expect-publication-state published \
        --require-zero-rgb || true

    donor_e0="$(capture_path donor "$spacing" e0 forward)"
    run_stage donor "$spacing" e0 forward \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-publication-state published \
        --world-roi "${donor_roi[@]}" \
        --min-roi-luminance-mean "$donor_min_e0_luminance_mean" \
        --max-exact-direct-sun-visibility 0 || true
    donor_reverse="$(capture_path donor "$spacing" e0 reverse)"
    run_stage donor "$spacing" e0 reverse \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-publication-state published \
        --world-roi "${donor_roi[@]}" \
        --min-roi-luminance-mean "$donor_min_e0_luminance_mean" \
        --max-exact-direct-sun-visibility 0 \
        --compare "$donor_e0" || true

    dogleg_e0="$(capture_path dogleg "$spacing" e0 forward)"
    if ! run_stage dogleg "$spacing" e0 forward \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-publication-state published \
        --world-roi "${dogleg_receiver_roi[@]}" \
        --max-roi-luminance-mean "$dogleg_max_e0_luminance_mean" \
        --max-exact-direct-sun-visibility 0; then
        filter_history_outcome_accepted=false
    fi
    if ! run_stage dogleg "$spacing" e1 forward \
        --expect-lifecycle-state converging \
        --expect-update-epoch 1 \
        --expect-source-state converging \
        --expect-source-update-epoch 0 \
        --expect-publication-state published \
        --world-roi "${dogleg_receiver_roi[@]}" \
        --baseline "$dogleg_e0" \
        --min-roi-luminance-gain "$dogleg_min_e1_luminance_gain" \
        --max-exact-direct-sun-visibility 0; then
        filter_history_outcome_accepted=false
    fi

    for convergence_case in portal donor dogleg; do
        run_stage "$convergence_case" "$spacing" converged forward \
            --expect-lifecycle-state converged \
            --expect-publication-state published \
            || true
    done

    if ! $dry_run && [[ -f "$donor_e0" ]]; then
        echo "[DDGI_TRANSPORT] evidence donor_e0=$donor_e0"
    fi
    if ! $dry_run && [[ -f "$donor_reverse" ]]; then
        echo "[DDGI_TRANSPORT] evidence donor_reverse=$donor_reverse"
    fi
done

if ! $dry_run && $filter_history_outcome_accepted; then
    echo "[DDGI_TRANSPORT] filter-history-outcome=ACCEPTED seam=dogleg-e0-e1-production-capture"
fi

convergence_summary="$run_dir/convergence-calibration.json"
convergence_summary_command=(
    "$convergence_summarizer"
    --run-dir "$run_dir"
    --output "$convergence_summary"
    --absolute-threshold "$convergence_max_abs_delta"
    --relative-threshold "$convergence_max_rel_delta"
    --consecutive-epochs "$convergence_consecutive_epochs"
    --minimum-epoch-count "$convergence_minimum_epoch_count"
    --maximum-update-epoch "$convergence_max_epoch"
)
if $dry_run; then
    print_command "${convergence_summary_command[@]}"
elif ! "${convergence_summary_command[@]}"; then
    echo "[DDGI_TRANSPORT] FAIL convergence provenance summary" >&2
    failures=$((failures + 1))
else
    echo "[DDGI_TRANSPORT] convergence-calibration=$convergence_summary"
fi

run_child() {
    local script="$1"
    if $dry_run; then
        print_command "$script" --dry-run
        return 0
    fi
    if ! "$script"; then
        failures=$((failures + 1))
        return 1
    fi
    return 0
}

# Preserve the calibrated portal/walls exact-reference thresholds in the existing runner.
run_child "$repo_root/scripts/check_ddgi_correctness.sh" || true
if run_child "$repo_root/scripts/check_ddgi_runtime_terrain_edits.sh"; then
    if ! $dry_run; then
        echo "[DDGI_TRANSPORT] direct-sun-framebuffer=PROVEN seam=v6-direct-light-plane runner=check_ddgi_runtime_terrain_edits.sh"
    fi
fi

normalization_evidence_checker="$repo_root/scripts/check_ddgi_sky_normalization_evidence.py"
if $dry_run; then
    print_command python3 "$normalization_evidence_checker"
elif ! python3 "$normalization_evidence_checker"; then
    failures=$((failures + 1))
fi

lifecycle_runner="$repo_root/scripts/check_ddgi_lifecycle_acceptance.sh"
if [[ ! -x "$lifecycle_runner" ]]; then
    echo "[DDGI_TRANSPORT] FAIL lifecycle runner missing: $lifecycle_runner" >&2
    exit 1
fi
run_child "$lifecycle_runner" || true

if $dry_run; then
    echo "[DDGI_TRANSPORT] dry-run complete spacings=2 sealed_epochs=3 donor_epochs=2 dogleg_epochs=2 convergence_curves=8 batch_orders=2"
    exit 0
fi

echo "[DDGI_TRANSPORT] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
