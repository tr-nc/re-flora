#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

analyze_current_capture() {
    "$repo_root/scripts/analyze_current_environment_irradiance_capture.py" "$@"
}


output_root="${DDGI_LOCAL_TERRAIN_OUTPUT_DIR:-$repo_root/target/ddgi-local-terrain-convergence}"
auto_exit="${DDGI_LOCAL_TERRAIN_AUTO_EXIT:-30}"
minimum_recovery_epoch="${DDGI_LOCAL_TERRAIN_MIN_RECOVERY_EPOCH:-4}"
maximum_post_promotion_high_delta_epochs="${DDGI_LOCAL_TERRAIN_MAX_POST_PROMOTION_HIGH_DELTA_EPOCHS:-0}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
capture="$run_dir/closed-spacing32.rfirr"
console="$run_dir/closed-spacing32.console.log"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi

command=(
    cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
    --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
    --environment-lighting-test-scene terrain-edits-closed
    --environment-probe-spacing-voxels 32
    --environment-irradiance-capture "$capture"
    --environment-irradiance-capture-target converged
    --auto-exit "$auto_exit"
)

if $dry_run; then
    printf '%q ' "${command[@]}"
    printf '\n'
    echo "[DDGI_LOCAL_TERRAIN] dry-run"
    exit 0
fi

mkdir -p "$run_dir"
cargo build --quiet --release --manifest-path "$repo_root/Cargo.toml"

set +e
RUST_LOG="warn,re_flora::tracer=debug,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
    "${command[@]}" >"$console" 2>&1
command_status=$?
set -e

failures=0
fail() {
    echo "[DDGI_LOCAL_TERRAIN] FAIL $*" >&2
    failures=$((failures + 1))
}

if (( command_status != 0 )); then
    fail "runtime status=$command_status"
fi
if [[ ! -f "$capture" ]]; then
    fail "capture missing path=$capture"
fi
if grep -Eiq '(^|[^[:alpha:]])(ERROR|panic|VUID-|validation error|destroyed descriptor|stale readback)' "$console"; then
    fail "runtime error marker"
fi

initial_revision="$(sed -n 's/.*\[ENV_LIGHT_EDIT_CYCLE\] initial probe field ready terrain_revision=\([0-9][0-9]*\).*/\1/p' "$console" | tail -n 1)"
if [[ -z "$initial_revision" ]]; then
    fail "missing initial terrain revision"
    final_revision=""
else
    final_revision="$((initial_revision + 1))"
fi

promotion=""
promotion_line=""
if [[ -n "$final_revision" ]]; then
    if grep -Eq "\[DDGI\] runtime observed visible terrain revision=$final_revision([^0-9]|$).*invalidation_voxel_bound=Some\(\(UVec3\(0, 0, 0\), UVec3\(512, 512, 512\)\)\)" "$console"; then
        fail "terrain revision=$final_revision invalidated the full DDGI domain"
    fi

    recovery="$(grep -E "\[DDGI\]\[LOCAL_RECOVERY\] prepared .*geometry_revision=$final_revision([^0-9]|$)" "$console" | tail -n 1 || true)"
    dirty_probes="$(sed -n 's/.*dirty_probes=\([0-9][0-9]*\).*/\1/p' <<<"$recovery")"
    preserved_probes="$(sed -n 's/.*preserved_probes=\([0-9][0-9]*\).*/\1/p' <<<"$recovery")"
    if [[ -z "$dirty_probes" || -z "$preserved_probes" ]] \
        || (( dirty_probes == 0 || preserved_probes == 0 )); then
        fail "revision=$final_revision lacks a nonempty dirty/preserved probe partition"
    fi

    promotion="$(grep -E "\[DDGI\] staging promoted .*geometry_revision=$final_revision([^0-9]|$)" "$console" | tail -n 1 || true)"
    promotion_line="$(grep -nE "\[DDGI\] staging promoted .*geometry_revision=$final_revision([^0-9]|$)" "$console" | tail -n 1 | cut -d: -f1 || true)"
    promoted_epoch="$(sed -n 's/.*published_update_epoch=\([0-9][0-9]*\).*/\1/p' <<<"$promotion")"
    if [[ -z "$promotion" ]]; then
        fail "missing promotion for revision=$final_revision"
    elif [[ "$promotion" != *"published_source=Some("* ]]; then
        fail "revision=$final_revision promotion did not retain an explicit history source"
    fi
    if [[ -z "$promoted_epoch" ]] || (( promoted_epoch < minimum_recovery_epoch )); then
        fail "revision=$final_revision promoted_epoch=${promoted_epoch:-missing} minimum=$minimum_recovery_epoch"
    fi

    if [[ -n "$promotion_line" ]]; then
        post_promotion_high_delta_epochs="$(tail -n "+$promotion_line" "$console" | awk -v revision="$final_revision" '
            $0 ~ /\[DDGI\] full-atlas validated/ && $0 ~ ("geometry_revision=" revision "([^0-9]|$)") {
                if (match($0, /max_abs_rgb_delta=[0-9.]+/)) {
                    value = substr($0, RSTART + 18, RLENGTH - 18) + 0.0
                    if (value > 0.1) count += 1
                }
            }
            END { print count + 0 }
        ')"
        if (( post_promotion_high_delta_epochs > maximum_post_promotion_high_delta_epochs )); then
            fail "revision=$final_revision post_promotion_high_delta_epochs=$post_promotion_high_delta_epochs maximum=$maximum_post_promotion_high_delta_epochs"
        fi
    fi
fi

if [[ -f "$capture" ]] && ! analyze_current_capture \
    "$capture" --max-luminance 0.00005 >/dev/null; then
    fail "closed scene retained stale light"
fi

if (( failures != 0 )); then
    echo "[DDGI_LOCAL_TERRAIN] verdict=RED failures=$failures output=$run_dir" >&2
    rg -n "LOCAL_RECOVERY|runtime observed visible terrain|staging promoted|transport converged|ENV_LIGHT_EDIT_CYCLE" "$console" | tail -n 32 >&2 || true
    exit 1
fi

echo "[DDGI_LOCAL_TERRAIN] verdict=GREEN revision=$final_revision dirty_probes=$dirty_probes preserved_probes=$preserved_probes promoted_epoch=$promoted_epoch post_promotion_high_delta_epochs=$post_promotion_high_delta_epochs output=$run_dir"
