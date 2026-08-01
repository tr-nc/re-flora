#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
auto_exit="${DDGI_RUNTIME_TERRAIN_EDIT_AUTO_EXIT:-60}"
output_root="${DDGI_RUNTIME_TERRAIN_EDIT_OUTPUT_DIR:-$repo_root/target/ddgi-runtime-terrain-edits}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi

spacings=(32 16)
states=(initial-open closed sequential-reopened inflight-latest-wins)
failures=0

if ! $dry_run; then
    mkdir -p "$run_dir"
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

scenario_for_state() {
    case "$1" in
        initial-open) echo portal ;;
        closed) echo terrain-edits-closed ;;
        sequential-reopened) echo terrain-edits ;;
        inflight-latest-wins) echo terrain-edits-inflight ;;
        inflight-fail-closed) echo terrain-edits-inflight-capture ;;
        *) return 2 ;;
    esac
}

final_revision_for_state() {
    case "$1" in
        initial-open) echo 1 ;;
        closed) echo 2 ;;
        sequential-reopened|inflight-latest-wins|inflight-fail-closed) echo 3 ;;
        *) return 2 ;;
    esac
}

run_capture() {
    local spacing="$1"
    local state="$2"
    local view="$3"
    local label="$4"
    local flora_enabled="${5:-false}"
    local scenario
    scenario="$(scenario_for_state "$state")"
    local capture="$run_dir/${state}-spacing${spacing}-${label}.rfirr"
    local console="$run_dir/${state}-spacing${spacing}-${label}.console.log"
    local command=(
        cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene "$scenario"
        --environment-probe-spacing-voxels "$spacing"
        --ddgi-debug-view "$view"
        --environment-irradiance-capture "$capture"
        --auto-exit "$auto_exit"
    )
    if [[ "$flora_enabled" != true ]]; then
        command+=(--no-flora)
    fi
    if $dry_run; then
        printf '%q ' "${command[@]}"
        printf '\n'
        return 0
    fi

    echo "[DDGI_RUNTIME_EDIT] state=$state spacing=$spacing view=$view label=$label"
    set +e
    RUST_LOG="warn,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
        "${command[@]}" 2>&1 | tee "$console"
    local command_status=${PIPESTATUS[0]}
    set -e
    if (( command_status != 0 )) || [[ ! -f "$capture" ]]; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing label=$label status=$command_status capture_present=$([[ -f "$capture" ]] && echo yes || echo no)" >&2
        return 1
    fi
    if grep -Eiq '(^|[^[:alpha:]])(ERROR|panic|VUID-|validation error|destroyed descriptor|stale readback)' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing label=$label error marker in console" >&2
        grep -Ei 'ERROR|panic|VUID-|validation error|destroyed descriptor|stale readback' "$console" | tail -n 20 >&2 || true
        return 1
    fi
}

check_lifecycle_markers() {
    local spacing="$1"
    local state="$2"
    local console="$3"
    local final_revision
    final_revision="$(final_revision_for_state "$state")"
    local required=()
    if [[ "$state" == "initial-open" ]]; then
        required=(
            "[ENV_LIGHT_TEST] ready case=portal backend=ddgi terrain_revision=1 geometry=static"
            "[ENV_IRRADIANCE_CAPTURE] saved"
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
        )
    else
        required=(
            "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=1"
            "[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=1 target_revision=2"
            "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))"
            "target_terrain_revision=$final_revision"
            "[DDGI] staging promoted"
            "terrain_revision=$final_revision"
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster"
            "active_token_serial="
            "[ENV_IRRADIANCE_CAPTURE] saved"
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
        )
        if [[ "$state" == "closed" ]]; then
            required+=("[ENV_LIGHT_EDIT_CYCLE] complete mode=closed final_terrain_revision=2")
        else
            required+=(
                "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=2 target_revision=3"
                "[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=3"
            )
        fi
        if [[ "$state" == "inflight-latest-wins" ]]; then
            required+=(
                "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=2"
                "[DDGI] obsolete staging promotion skipped"
                "replacement_terrain_revision=3"
            )
        fi
    fi

    local missing=0
    local marker
    for marker in "${required[@]}"; do
        if ! grep -Fq "$marker" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] missing state=$state spacing=$spacing marker=$marker" >&2
            missing=$((missing + 1))
        fi
    done
    if [[ "$state" != "initial-open" ]]; then
        if ! grep -Eq "\\[DDGI\\]\\[CONSUMERS\\] consumer_set=terrain_compute,flora_raster .*active_token_serial=[0-9]+.*geometry_revision=$final_revision([^0-9]|$).*transport=SingleBounce.*iteration=1([^0-9]|$)" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] missing shared consumer first-current-geometry S1 publication state=$state spacing=$spacing revision=$final_revision" >&2
            missing=$((missing + 1))
        fi
        if ! grep -Eq "\\[DDGI\\] staging promoted .*token_serial=[0-9]+.*geometry_revision=$final_revision([^0-9]|$).*published_transport=SingleBounce.*published_iteration=1([^0-9]|$)" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] missing exact active S1 promotion state=$state spacing=$spacing revision=$final_revision" >&2
            missing=$((missing + 1))
        fi
    fi
    if [[ "$state" == "inflight-latest-wins" ]] && \
        grep -Eq '\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision=2([^0-9]|$)' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] obsolete revision 2 promoted state=$state spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    (( missing == 0 ))
}

check_inflight_fail_closed_markers() {
    local spacing="$1"
    local console="$2"
    local required=(
        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=1"
        "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=2"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=2 target_revision=3"
        "[DDGI] obsolete staging promotion skipped"
        "replacement_terrain_revision=3"
        "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))"
        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision=Some(1) target_terrain_revision=3"
        "coordinator=BuildingTerrain"
        "invalidation=full-domain-fail-closed"
        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision=Some(1) target_terrain_revision=3"
        "staging_token_serial=Some("
        "staging_stage=Some(Rebuilding)"
        "[ENV_IRRADIANCE_CAPTURE] saved"
        "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
    )
    local missing=0
    local marker
    for marker in "${required[@]}"; do
        if ! grep -Fq "$marker" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] missing transient state spacing=$spacing marker=$marker" >&2
            missing=$((missing + 1))
        fi
    done
    if ! grep -Eq '\[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE\] recording .*staging_progress=[0-9]+/[1-9][0-9]* .*coordinator=BuildingTerrain' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] missing GPU-visible staging progress spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    if grep -Eq '\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision=(2|3)([^0-9]|$)' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] transient capture occurred after a terrain promotion spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    if grep -Eq '\[DDGI\]\[CONSUMERS\].*geometry_revision=(2|3)([^0-9]|$)' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] transient capture exposed an unready terrain revision spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    (( missing == 0 ))
}

check_captures() {
    local spacing="$1"
    local state="$2"
    local first="$run_dir/${state}-spacing${spacing}-final-a.rfirr"
    local second="$run_dir/${state}-spacing${spacing}-final-b.rfirr"
    local reference="$run_dir/${state}-spacing${spacing}-exact-irradiance.rfirr"
    local thresholds=(--max-reference-error-p99 0.01)
    if [[ "$state" == "closed" ]]; then
        thresholds=(--max-luminance 0.00001 --max-reference-error-p99 0.00001)
    else
        thresholds+=(--min-luminance-p99 0.10)
    fi
    if ! cmp -s "$first" "$second"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing final captures are not bit-exact" >&2
        return 1
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$first" --compare "$second" --reference "$reference" "${thresholds[@]}"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing luminance/reference threshold" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS state=$state spacing=$spacing deterministic exact-reference-qualified"
}

check_inflight_fail_closed_captures() {
    local spacing="$1"
    local first="$run_dir/inflight-fail-closed-spacing${spacing}-final-a.rfirr"
    local second="$run_dir/inflight-fail-closed-spacing${spacing}-final-b.rfirr"
    if ! cmp -s "$first" "$second"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL transient spacing=$spacing captures are not bit-exact" >&2
        return 1
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$first" --compare "$second" --max-luminance 0.00001 --require-zero-rgb; then
        echo "[DDGI_RUNTIME_EDIT] FAIL transient spacing=$spacing expected terrain hits and strict zero" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS state=inflight-fail-closed spacing=$spacing GPU-visible strict-zero deterministic"
}

check_flora_consumer() {
    local console="$1"
    local capture="$2"
    local consumer_line
    consumer_line="$(grep -E '\[DDGI\]\[CONSUMERS\].*geometry_revision=3([^0-9]|$)' "$console" | tail -n 1 || true)"
    local active_token
    active_token="$(sed -n 's/.*active_token_serial=\([0-9][0-9]*\).*/\1/p' <<<"$consumer_line")"
    if [[ -z "$active_token" ]]; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora run missing final shared-consumer token" >&2
        return 1
    fi
    if ! grep -Eq "\\[DDGI\\]\\[FLORA_CONSUMER\\] draw_recorded active_token_serial=$active_token terrain_revision=3([^0-9]|$).*instance_count=[1-9][0-9]*" "$console"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora draw did not consume final token=$active_token revision=3" >&2
        return 1
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora-enabled final capture is not lit" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS flora draw consumed final token=$active_token terrain_revision=3"
}

for spacing in "${spacings[@]}"; do
    for state in "${states[@]}"; do
        case_failed=false
        for view_and_label in \
            "final:final-a" \
            "final:final-b" \
            "exact-irradiance:exact-irradiance"; do
            view="${view_and_label%%:*}"
            label="${view_and_label#*:}"
            if ! run_capture "$spacing" "$state" "$view" "$label"; then
                case_failed=true
            elif ! $dry_run && ! check_lifecycle_markers \
                "$spacing" "$state" "$run_dir/${state}-spacing${spacing}-${label}.console.log"; then
                case_failed=true
            fi
        done
        if ! $dry_run && ! check_captures "$spacing" "$state"; then
            case_failed=true
        fi
        if $case_failed; then
            failures=$((failures + 1))
        fi
    done
done

for spacing in "${spacings[@]}"; do
    transient_failed=false
    for label in final-a final-b; do
        if ! run_capture "$spacing" inflight-fail-closed final "$label"; then
            transient_failed=true
        elif ! $dry_run && ! check_inflight_fail_closed_markers \
            "$spacing" "$run_dir/inflight-fail-closed-spacing${spacing}-${label}.console.log"; then
            transient_failed=true
        fi
    done
    if ! $dry_run && ! check_inflight_fail_closed_captures "$spacing"; then
        transient_failed=true
    fi
    if $transient_failed; then
        failures=$((failures + 1))
    fi
done

flora_capture="$run_dir/flora-consumer-spacing32-final.rfirr"
flora_console="$run_dir/flora-consumer-spacing32-final.console.log"
if ! run_capture 32 sequential-reopened final flora-final true; then
    failures=$((failures + 1))
elif ! $dry_run; then
    mv "$run_dir/sequential-reopened-spacing32-flora-final.rfirr" "$flora_capture"
    mv "$run_dir/sequential-reopened-spacing32-flora-final.console.log" "$flora_console"
    if ! check_lifecycle_markers 32 sequential-reopened "$flora_console" \
        || ! check_flora_consumer "$flora_console" "$flora_capture"; then
        failures=$((failures + 1))
    fi
fi

if $dry_run; then
    echo "[DDGI_RUNTIME_EDIT] dry-run matrix final_states=4x2x3 transient=2x2 flora=1 total_runs=29"
    exit 0
fi

echo "[DDGI_RUNTIME_EDIT] output=$run_dir failed_cases=$failures"
if (( failures != 0 )); then
    exit 1
fi
