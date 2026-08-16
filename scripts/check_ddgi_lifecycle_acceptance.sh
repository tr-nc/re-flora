#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

auto_exit="${DDGI_LIFECYCLE_AUTO_EXIT:-90}"
output_root="${DDGI_LIFECYCLE_OUTPUT_DIR:-$repo_root/target/ddgi-lifecycle-acceptance}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
analyzer="$repo_root/scripts/analyze_environment_irradiance_capture.py"
radiance_validator="$repo_root/scripts/validate_ddgi_radiance_lifecycle.py"
failures=0
dry_run=false
if [[ $# -eq 1 && "$1" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi

if ! $dry_run; then
    mkdir -p "$run_dir"
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

print_command() {
    printf '%q ' "$@"
    printf '\n'
}

field_value() {
    local line="$1"
    local field="$2"
    sed -n "s/.*[[:space:]]${field}=\([^[:space:]]*\).*/\1/p" <<<"$line"
}

require_markers() {
    local group="$1"
    local console="$2"
    shift 2
    local missing=0
    local marker
    for marker in "$@"; do
        if ! grep -Fq "$marker" "$console"; then
            echo "[DDGI_LIFECYCLE] FAIL group=$group missing_marker=$marker" >&2
            missing=$((missing + 1))
        fi
    done
    (( missing == 0 ))
}

run_hidden() {
    local group="$1"
    local scene="$2"
    local spacing_voxels="$3"
    local capture_target="$4"
    local capture="$5"
    local console="$6"
    shift 6
    local command=(
        cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene "$scene"
        --environment-probe-spacing-voxels "$spacing_voxels"
        --environment-irradiance-capture "$capture"
        --environment-irradiance-capture-target "$capture_target"
        --auto-exit "$auto_exit"
        "$@"
    )

    echo "[DDGI_LIFECYCLE] group=$group scene=$scene target=$capture_target running"
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
        echo "[DDGI_LIFECYCLE] FAIL group=$group status=$command_status capture_present=$([[ -f "$capture" ]] && echo yes || echo no)" >&2
        return 1
    fi
    if grep -Eiq '(^|[^[:alpha:]])(ERROR|panic|VUID-|validation error|destroyed descriptor|stale readback)' "$console"; then
        echo "[DDGI_LIFECYCLE] FAIL group=$group error marker in console" >&2
        grep -Ei 'ERROR|panic|VUID-|validation error|destroyed descriptor|stale readback' "$console" | tail -n 20 >&2 || true
        return 1
    fi
}

check_radiance() {
    local spacing_voxels="$1"
    local capture="$run_dir/radiance-changes-spacing-${spacing_voxels}.rfirr"
    local console="$run_dir/radiance-changes-spacing-${spacing_voxels}.console.log"
    run_hidden "RADIANCE-${spacing_voxels}" radiance-changes "$spacing_voxels" published "$capture" "$console" || return 1
    if $dry_run; then
        return 0
    fi
    require_markers RADIANCE "$console" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r1-terminal" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=baseline" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r2-next-frame" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r2-midflight" \
        "old_field_visible=true" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r3-observed" \
        "field_serial_allocated=false" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r4-next-frame" \
        "immutable_inflight_radiance_revision=2" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=r4-midflight" \
        "r3_coalesced=true" \
        "[DDGI_ACCEPT][RADIANCE] checkpoint=complete" \
        "field_serial_gap_r2_to_r4=1" \
        "geometry_unchanged=true" \
        "spacing_unchanged=true" || return 1

    local complete
    complete="$(grep -F '[DDGI_ACCEPT][RADIANCE] checkpoint=complete' "$console" | tail -n 1)"
    local field_serial source_field_serial geometry_revision
    field_serial="$(field_value "$complete" field_serial)"
    source_field_serial="$(field_value "$complete" source_field_serial)"
    geometry_revision="$(field_value "$complete" geometry_revision)"
    local checkpoint
    checkpoint="$(grep -F "[ENV_IRRADIANCE_CAPTURE] checkpoint target=published" "$console" | grep -F "field_serial=$field_serial" | tail -n 1)"
    local build_token_serial
    build_token_serial="$(field_value "$checkpoint" build_token_serial)"
    [[ -n "$field_serial" && -n "$source_field_serial" && -n "$geometry_revision" && -n "$build_token_serial" ]] || {
        echo "[DDGI_LIFECYCLE] FAIL group=RADIANCE could not extract final canonical identity" >&2
        return 1
    }

    "$analyzer" "$capture" \
        --expect-version 6 \
        --expect-spacing-voxels "$spacing_voxels" \
        --expect-geometry-revision "$geometry_revision" \
        --expect-radiance-revision 4 \
        --expect-build-token-serial "$build_token_serial" \
        --expect-field-serial "$field_serial" \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-source-state converging \
        --expect-source-update-epoch 0 \
        --expect-source-field-serial "$source_field_serial" \
        --expect-source-radiance-revision 2 \
        --expect-publication-state published \
        --expect-batch-order forward \
        --require-nonnegative-rgb >"$run_dir/radiance-changes-spacing-${spacing_voxels}.analysis.json" || return 1
    "$radiance_validator" "$capture" \
        --expect-spacing-voxels "$spacing_voxels" \
        --direct-light-sunlit-roi 0.85 0.60 1.025 0.875 0.675 1.125 \
        --min-direct-light-roi-delta 0.02 \
        >"$run_dir/radiance-changes-spacing-${spacing_voxels}.lifecycle.json" || return 1
    echo "[DDGI_LIFECYCLE] PASS group=RADIANCE spacing_voxels=$spacing_voxels field_serial=$field_serial source_field_serial=$source_field_serial"
}

check_density() {
    local capture="$run_dir/density-changes.rfirr"
    local console="$run_dir/density-changes.console.log"
    run_hidden DENSITY density-changes 32 e0 "$capture" "$console" \
        --environment-probe-rebuild-spacing-voxels 16 || return 1
    if $dry_run; then
        return 0
    fi
    require_markers DENSITY "$console" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=baseline" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=density-midflight" \
        "old_field_visible=true active_available=true" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density" \
        "queued_density_spacing_voxels=16" \
        "obsolete_density_consumer_visible=false active_available=true" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-e0-published" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=density-retry-midflight" \
        "[DDGI_ACCEPT][DENSITY] checkpoint=complete" \
        "first_consumer_visible_16_epoch=0" || return 1

    local preemption complete
    preemption="$(grep -F '[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density' "$console" | tail -n 1)"
    complete="$(grep -F '[DDGI_ACCEPT][DENSITY] checkpoint=complete' "$console" | tail -n 1)"
    local obsolete_token field_serial source_field_serial geometry_revision
    obsolete_token="$(field_value "$preemption" obsolete_density_token_serial)"
    field_serial="$(field_value "$complete" field_serial)"
    source_field_serial="$(field_value "$complete" source_field_serial)"
    geometry_revision="$(field_value "$complete" geometry_revision)"
    local checkpoint build_token_serial
    checkpoint="$(grep -F "[ENV_IRRADIANCE_CAPTURE] checkpoint target=e0" "$console" | grep -F "field_serial=$field_serial" | tail -n 1)"
    build_token_serial="$(field_value "$checkpoint" build_token_serial)"
    [[ -n "$obsolete_token" && -n "$field_serial" && -n "$source_field_serial" && -n "$geometry_revision" && -n "$build_token_serial" ]] || {
        echo "[DDGI_LIFECYCLE] FAIL group=DENSITY could not extract lifecycle identity" >&2
        return 1
    }
    if grep -Eq "\[DDGI\] staging promoted .*token_serial=${obsolete_token}([^0-9]|$)" "$console"; then
        echo "[DDGI_LIFECYCLE] FAIL group=DENSITY obsolete density token promoted token_serial=$obsolete_token" >&2
        return 1
    fi
    if grep -Eq "\[DDGI\]\[CONSUMERS\].*(active_token_serial=${obsolete_token}([^0-9]|$)|token_serial=(Some\()?${obsolete_token}([^0-9]|$))" "$console"; then
        echo "[DDGI_LIFECYCLE] FAIL group=DENSITY obsolete density token became consumer-active token_serial=$obsolete_token" >&2
        return 1
    fi

    "$analyzer" "$capture" \
        --expect-version 6 \
        --expect-spacing-voxels 16 \
        --expect-geometry-revision "$geometry_revision" \
        --expect-radiance-revision 1 \
        --expect-build-token-serial "$build_token_serial" \
        --expect-field-serial "$field_serial" \
        --expect-lifecycle-state converging \
        --expect-update-epoch 0 \
        --expect-publication-state published \
        --expect-batch-order forward \
        --require-nonnegative-rgb >"$run_dir/density-changes.analysis.json" || return 1
    echo "[DDGI_LIFECYCLE] PASS group=DENSITY field_serial=$field_serial source_field_serial=$source_field_serial obsolete_token=$obsolete_token"
}

if ! check_radiance 32; then
    failures=$((failures + 1))
fi
if ! check_radiance 16; then
    failures=$((failures + 1))
fi
if ! check_density; then
    failures=$((failures + 1))
fi

if $dry_run; then
    echo "[DDGI_LIFECYCLE] dry-run complete scenarios=3"
    exit 0
fi

echo "[DDGI_LIFECYCLE] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
