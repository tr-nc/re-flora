#!/usr/bin/env bash
set -euo pipefail

readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

analyze_current_capture() {
    if $dry_run; then
        printf '%q ' analyze_current_capture "$@" >&2
        printf '\n' >&2
        return 0
    fi
    "$repo_root/scripts/analyze_current_environment_irradiance_capture.py" "$@"
}

auto_exit="${DDGI_INFLIGHT_EDIT_AUTO_EXIT:-12}"
output_root="${DDGI_INFLIGHT_EDIT_OUTPUT_DIR:-$repo_root/target/ddgi-inflight-terrain-edits}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$output_root/$run_id"
dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--dry-run]" >&2
    exit 2
fi
readonly dry_run

spacings=(32 16)
repeats=(1 2)
if ! $dry_run; then
    mkdir -p "$run_dir"
    /usr/bin/env cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

failures=0
run_case() {
    local spacing="$1"
    local repeat="$2"
    local capture="$run_dir/terrain-edits-inflight-spacing${spacing}-repeat${repeat}.rfirr"
    local console="$run_dir/terrain-edits-inflight-spacing${spacing}-repeat${repeat}.console.log"
    command=(
        /usr/bin/env cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene terrain-edits-inflight
        --environment-probe-spacing-voxels "$spacing"
        --environment-irradiance-capture "$capture"
        --auto-exit "$auto_exit"
    )
    if $dry_run; then
        printf '%q ' "${command[@]}"
        printf '\n'
    else
        echo "[DDGI_INFLIGHT_EDIT] spacing=$spacing repeat=$repeat running"
        set +e
        RUST_LOG="warn,re_flora::tracer=info,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
            "${command[@]}" 2>&1 | /usr/bin/env tee "$console"
        command_status=${PIPESTATUS[0]}
        set -e

        initial_revision="$(sed -n 's/.*\[ENV_LIGHT_EDIT_CYCLE\] initial probe field ready terrain_revision=\([0-9][0-9]*\).*/\1/p' "$console" | tail -n 1)"
        if [[ -z "$initial_revision" ]]; then
            echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing repeat=$repeat missing initial terrain revision" >&2
            return 1
        fi
        obsolete_revision="$((initial_revision + 1))"
        replacement_revision="$((initial_revision + 2))"
        required=(
        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=$initial_revision"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=$initial_revision target_revision=$obsolete_revision"
        "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=close-skylight target_revision=$obsolete_revision"
        "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=$obsolete_revision"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=$obsolete_revision target_revision=$replacement_revision"
        "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=$replacement_revision"
        "[DDGI] obsolete staging promotion skipped"
        "replacement_terrain_revision=$replacement_revision"
        "[DDGI] staging promoted"
        "kind=Terrain"
        "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=reopen-skylight terrain_revision=$replacement_revision"
        "[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=$replacement_revision"
        "[ENV_IRRADIANCE_CAPTURE] saved"
        )
        missing=()
        for marker in "${required[@]}"; do
            if ! grep -Fq "$marker" "$console"; then
                missing+=("$marker")
            fi
        done
        if grep -Eq "\[DDGI\] staging promoted .*kind=Terrain.*terrain_revision=${obsolete_revision}([^0-9]|$)" "$console"; then
            echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing repeat=$repeat obsolete terrain revision $obsolete_revision became active" >&2
            return 1
        fi
        if (( command_status != 0 || ${#missing[@]} != 0 )) || [[ ! -f "$capture" ]]; then
            echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing repeat=$repeat status=$command_status missing_markers=${#missing[@]} capture_present=$([[ -f "$capture" ]] && echo yes || echo no)" >&2
            for marker in "${missing[@]}"; do
                echo "[DDGI_INFLIGHT_EDIT] missing: $marker" >&2
            done
            grep -E "ENV_LIGHT_EDIT|\[DDGI\].*(staging|rebuild)" "$console" | tail -n 40 >&2 || true
            return 1
        fi
    fi
    if ! analyze_current_capture \
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing repeat=$repeat final reopened portal is not lit" >&2
        return 1
    fi
    if ! $dry_run; then
        echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing repeat=$repeat active revision skipped $obsolete_revision and reached $replacement_revision"
    fi
}

for spacing in "${spacings[@]}"; do
    for repeat in "${repeats[@]}"; do
        if ! run_case "$spacing" "$repeat"; then
            failures=$((failures + 1))
        fi
    done
    first="$run_dir/terrain-edits-inflight-spacing${spacing}-repeat1.rfirr"
    second="$run_dir/terrain-edits-inflight-spacing${spacing}-repeat2.rfirr"
    analysis_output=/dev/null
    if analyze_current_capture \
        "$first" --compare "$second" --compare-direct-light >"$analysis_output"; then
        if ! $dry_run; then
            echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing deterministic final captures"
        fi
    else
        echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing final captures differ" >&2
        failures=$((failures + 1))
    fi
done

if $dry_run; then
    echo "[DDGI_INFLIGHT_EDIT] dry-run matrix spacings=2 repeats=2"
    exit 0
fi

echo "[DDGI_INFLIGHT_EDIT] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
