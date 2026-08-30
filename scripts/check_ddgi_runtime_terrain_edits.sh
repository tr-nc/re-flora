#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source "$repo_root/scripts/lib/capture_process_evidence.sh"
capture_rust_log="warn,re_flora::run_log_binding=info,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info"
auto_exit="${DDGI_RUNTIME_TERRAIN_EDIT_AUTO_EXIT:-60}"
minimum_local_recovery_epoch="${DDGI_RUNTIME_TERRAIN_EDIT_MIN_RECOVERY_EPOCH:-4}"
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

echo "[DDGI_RUNTIME_EDIT] direct-sun-evidence=v6-direct-light-plane sunlit_min_mean=0.15 shadowed_max=0"

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
        inflight-stale-active) echo terrain-edits-inflight-capture ;;
        *) return 2 ;;
    esac
}

final_revision_for_state() {
    local state="$1"
    local console="$2"
    local initial_revision
    if [[ "$state" == initial-open ]]; then
        initial_revision="$(sed -n 's/.*\[ENV_LIGHT_TEST\] ready case=portal backend=ddgi terrain_revision=\([0-9][0-9]*\).*/\1/p' "$console" | tail -n 1)"
    else
        initial_revision="$(sed -n 's/.*\[ENV_LIGHT_EDIT_CYCLE\] initial probe field ready terrain_revision=\([0-9][0-9]*\).*/\1/p' "$console" | tail -n 1)"
    fi
    [[ -n "$initial_revision" ]] || return 2
    case "$state" in
        initial-open) echo "$initial_revision" ;;
        closed) echo "$((initial_revision + 1))" ;;
        sequential-reopened|inflight-latest-wins|inflight-stale-active) echo "$((initial_revision + 2))" ;;
        *) return 2 ;;
    esac
}

run_capture() {
    local spacing="$1"
    local state="$2"
    local view="$3"
    local label="$4"
    local flora_enabled="${5:-false}"
    # Local geometry candidates first become visible after their private recovery window. Epoch
    # eight is the early-quality checkpoint; closed scenes separately wait for convergence.
    local capture_target="${6:-e8}"
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
    command+=(--environment-irradiance-capture-target "$capture_target")
    if $dry_run; then
        printf '%q ' "${command[@]}"
        printf '\n'
        return 0
    fi

    echo "[DDGI_RUNTIME_EDIT] state=$state spacing=$spacing view=$view label=$label"
    if ! run_capture_with_process_evidence \
        "$console" "$capture" "$capture_rust_log" \
        --require-test-scene-startup -- "${command[@]}"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing label=$label process evidence" >&2
        return 1
    fi
}

check_lifecycle_markers() {
    local spacing="$1"
    local state="$2"
    local console="$3"
    local final_revision
    final_revision="$(final_revision_for_state "$state" "$console")"
    local initial_revision
    if [[ "$state" == initial-open ]]; then
        initial_revision="$final_revision"
    elif [[ "$state" == closed ]]; then
        initial_revision="$((final_revision - 1))"
    else
        initial_revision="$((final_revision - 2))"
    fi
    local closed_revision="$((initial_revision + 1))"
    local required=()
    if [[ "$state" == "initial-open" ]]; then
        required=(
            "[ENV_LIGHT_TEST] ready case=portal backend=ddgi terrain_revision=$initial_revision geometry=static"
            "[ENV_IRRADIANCE_CAPTURE] saved"
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
        )
    else
        required=(
            "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=$initial_revision"
            "[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=$initial_revision target_revision=$closed_revision"
            "invalidation_voxel_bound=Some((UVec3("
            "target_terrain_revision=$final_revision"
            "[DDGI] staging promoted"
            "terrain_revision=$final_revision"
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster"
            "active_token_serial="
            "[ENV_IRRADIANCE_CAPTURE] saved"
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
        )
        if [[ "$state" == "closed" ]]; then
            required+=("[ENV_LIGHT_EDIT_CYCLE] complete mode=closed final_terrain_revision=$final_revision")
        else
            required+=(
                "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=$closed_revision target_revision=$final_revision"
                "[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=$final_revision"
            )
        fi
        if [[ "$state" == "inflight-latest-wins" ]]; then
            required+=(
                "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=$closed_revision"
                "[DDGI] obsolete staging promotion skipped"
                "replacement_terrain_revision=$final_revision"
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
        if grep -Fq "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] full-domain invalidation returned state=$state spacing=$spacing" >&2
            missing=$((missing + 1))
        fi
        local consumer_publication consumer_epoch
        consumer_publication="$(grep -E "\\[DDGI\\]\\[CONSUMERS\\] consumer_set=terrain_compute,flora_raster .*active_token_serial=[0-9]+.*geometry_revision=$final_revision([^0-9]|$).*state=Converging" "$console" | head -n 1 || true)"
        consumer_epoch="$(sed -n 's/.*update_epoch=\([0-9][0-9]*\).*/\1/p' <<<"$consumer_publication")"
        if [[ -z "$consumer_epoch" ]] || (( consumer_epoch < minimum_local_recovery_epoch )); then
            echo "[DDGI_RUNTIME_EDIT] shared consumer exposed an insufficiently recovered geometry state=$state spacing=$spacing revision=$final_revision epoch=${consumer_epoch:-missing} minimum=$minimum_local_recovery_epoch" >&2
            missing=$((missing + 1))
        fi
        local promotion promotion_epoch
        promotion="$(grep -E "\\[DDGI\\] staging promoted .*kind=Terrain .*geometry_revision=$final_revision([^0-9]|$).*published_state=Converging" "$console" | tail -n 1 || true)"
        promotion_epoch="$(sed -n 's/.*published_update_epoch=\([0-9][0-9]*\).*/\1/p' <<<"$promotion")"
        if [[ -z "$promotion_epoch" ]] || (( promotion_epoch < minimum_local_recovery_epoch )); then
            echo "[DDGI_RUNTIME_EDIT] terrain candidate promoted before local recovery state=$state spacing=$spacing revision=$final_revision epoch=${promotion_epoch:-missing} minimum=$minimum_local_recovery_epoch" >&2
            missing=$((missing + 1))
        fi
        if ! grep -Eq "\\[DDGI\\] staging promoted .*geometry_revision=$final_revision([^0-9]|$).*published_source=Some\\(" "$console"; then
            echo "[DDGI_RUNTIME_EDIT] terrain promotion did not retain resident history state=$state spacing=$spacing revision=$final_revision" >&2
            missing=$((missing + 1))
        fi
    fi
    if [[ "$state" == "inflight-latest-wins" ]] && \
        grep -Eq "\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision=$closed_revision([^0-9]|$)" "$console"; then
        echo "[DDGI_RUNTIME_EDIT] obsolete revision $closed_revision promoted state=$state spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    (( missing == 0 ))
}

check_inflight_stale_active_markers() {
    local spacing="$1"
    local console="$2"
    local required=(
        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision="
        "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision="
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision="
        "[DDGI] obsolete staging promotion skipped"
        "invalidation_voxel_bound=Some((UVec3("
        "coordinator=BuildingTerrain"
        "invalidation=stale-active"
        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision=Some("
        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision=Some("
        "staging_token_serial=Some("
        "staging_stage=Rebuilding"
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
    if grep -Fq "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" "$console"; then
        echo "[DDGI_RUNTIME_EDIT] transient state unexpectedly invalidated the full DDGI domain spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    if ! grep -Eq '\[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE\] recording .*staging_progress=[0-9]+/[1-9][0-9]* .*coordinator=BuildingTerrain' "$console"; then
        echo "[DDGI_RUNTIME_EDIT] missing GPU-visible staging progress spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    local armed active_revision target_revision
    armed="$(grep -F '[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed' "$console" | tail -n 1 || true)"
    active_revision="$(sed -n 's/.*active_terrain_revision=Some(\([0-9][0-9]*\)).*/\1/p' <<<"$armed")"
    target_revision="$(sed -n 's/.*target_terrain_revision=\([0-9][0-9]*\).*/\1/p' <<<"$armed")"
    if [[ -z "$active_revision" || -z "$target_revision" || "$active_revision" == "$target_revision" ]]; then
        echo "[DDGI_RUNTIME_EDIT] transient capture lacks distinct active/target revisions spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    if grep -E '\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision=' "$console" \
        | sed -n 's/.*geometry_revision=\([0-9][0-9]*\).*/\1/p' \
        | awk -v active="$active_revision" '$1 > active { found = 1 } END { exit !found }'; then
        echo "[DDGI_RUNTIME_EDIT] transient capture occurred after a terrain promotion spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    if grep -E '\[DDGI\]\[CONSUMERS\].*geometry_revision=' "$console" \
        | sed -n 's/.*geometry_revision=\([0-9][0-9]*\).*/\1/p' \
        | awk -v active="$active_revision" '$1 > active { found = 1 } END { exit !found }'; then
        echo "[DDGI_RUNTIME_EDIT] transient capture exposed an unready terrain revision spacing=$spacing" >&2
        missing=$((missing + 1))
    fi
    local obsolete_finished_line latest_started_line
    obsolete_finished_line="$(grep -nE '\[DDGI\] obsolete staging promotion skipped .*coordinator=' "$console" | head -n 1 | cut -d: -f1 || true)"
    latest_started_line="$(grep -nE '\[DDGI\] staging prepared .*target_terrain_revision=' "$console" | tail -n 1 | cut -d: -f1 || true)"
    if [[ -z "$obsolete_finished_line" || -z "$latest_started_line" ]] \
        || (( latest_started_line <= obsolete_finished_line )); then
        echo "[DDGI_RUNTIME_EDIT] terrain staging updates overlapped or lacked serialized lifecycle evidence spacing=$spacing" >&2
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
        # Temporal terrain refresh retains stable history; after its bounded recovery sweep the
        # sealed-room residual remains below a visually black HDR tolerance.
        thresholds=(--max-luminance 0.00005 --max-reference-error-p99 0.00005)
    else
        thresholds+=(--min-luminance-p99 0.10)
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$first" --compare "$second" --reference "$reference" "${thresholds[@]}"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL state=$state spacing=$spacing environment determinism/luminance/reference threshold" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS state=$state spacing=$spacing environment-bit-exact exact-reference-qualified"
}

check_inflight_stale_active_captures() {
    local spacing="$1"
    local first="$run_dir/inflight-stale-active-spacing${spacing}-final-a.rfirr"
    local second="$run_dir/inflight-stale-active-spacing${spacing}-final-b.rfirr"
    local console="$run_dir/inflight-stale-active-spacing${spacing}-final-a.console.log"
    local active_revision
    active_revision="$(sed -n 's/.*\[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE\] armed active_terrain_revision=Some(\([0-9][0-9]*\)).*/\1/p' "$console" | tail -n 1)"
    if [[ -z "$active_revision" ]]; then
        echo "[DDGI_RUNTIME_EDIT] FAIL transient spacing=$spacing missing resident active revision" >&2
        return 1
    fi
    if ! cmp -s "$first" "$second"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL transient spacing=$spacing captures are not bit-exact" >&2
        return 1
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$first" --compare "$second" --compare-direct-light --expect-version 8 \
        --expect-geometry-revision "$active_revision" --expect-publication-state published \
        --min-luminance-p99 0.10 --require-nonnegative-rgb \
        --correctness \
        --direct-light-sunlit-roi 0.85 0.60 1.025 0.875 0.675 1.125 \
        --min-direct-light-sunlit-luminance-mean 0.15 \
        --direct-light-shadowed-roi 0.425 0.60 1.075 0.45 0.85 1.275 \
        --max-direct-light-shadowed-luminance-max 0; then
        echo "[DDGI_RUNTIME_EDIT] FAIL transient spacing=$spacing expected lit resident active DDGI" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS state=inflight-stale-active spacing=$spacing GPU-visible stale-active deterministic direct-sun-independent"
}

check_flora_consumer() {
    local console="$1"
    local capture="$2"
    local final_revision
    final_revision="$(final_revision_for_state sequential-reopened "$console")"
    local consumer_line
    consumer_line="$(grep -E "\[DDGI\]\[CONSUMERS\].*geometry_revision=$final_revision([^0-9]|$)" "$console" | tail -n 1 || true)"
    local active_token
    active_token="$(sed -n 's/.*active_token_serial=\([0-9][0-9]*\).*/\1/p' <<<"$consumer_line")"
    if [[ -z "$active_token" ]]; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora run missing final shared-consumer token" >&2
        return 1
    fi
    if ! grep -Eq "\\[DDGI\\]\\[FLORA_CONSUMER\\] draw_recorded active_token_serial=$active_token terrain_revision=$final_revision([^0-9]|$).*instance_count=[1-9][0-9]*" "$console"; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora draw did not consume final token=$active_token revision=$final_revision" >&2
        return 1
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_RUNTIME_EDIT] FAIL flora-enabled final capture is not lit" >&2
        return 1
    fi
    echo "[DDGI_RUNTIME_EDIT] PASS flora draw consumed final token=$active_token terrain_revision=$final_revision"
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
            capture_target=""
            if [[ "$state" == closed ]]; then
                capture_target="converged"
            fi
            if ! run_capture "$spacing" "$state" "$view" "$label" false "$capture_target"; then
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
        if ! run_capture "$spacing" inflight-stale-active final "$label" false published; then
            transient_failed=true
        elif ! $dry_run && ! check_inflight_stale_active_markers \
            "$spacing" "$run_dir/inflight-stale-active-spacing${spacing}-${label}.console.log"; then
            transient_failed=true
        fi
    done
    if ! $dry_run && ! check_inflight_stale_active_captures "$spacing"; then
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
