#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
auto_exit="${DDGI_TERRAIN_EDIT_AUTO_EXIT:-60}"
output_root="${DDGI_TERRAIN_EDIT_OUTPUT_DIR:-$repo_root/target/ddgi-terrain-edit-cycle}"
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
if ! $dry_run; then
    mkdir -p "$run_dir"
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
fi

failures=0
run_case() {
    local spacing="$1"
    local scenario="$2"
    local mode="$3"
    local capture="$run_dir/terrain-edits-spacing${spacing}-${mode}.rfirr"
    local console="$run_dir/terrain-edits-spacing${spacing}-${mode}.console.log"
    command=(
        cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene "$scenario"
        --environment-probe-spacing-voxels "$spacing"
        --environment-irradiance-capture "$capture"
        --auto-exit "$auto_exit"
    )
    if $dry_run; then
        printf '%q ' "${command[@]}"
        printf '\n'
        return 0
    fi

    echo "[DDGI_TERRAIN_EDIT] spacing=$spacing running mode=$mode lifecycle"
    set +e
    RUST_LOG="warn,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
        "${command[@]}" 2>&1 | tee "$console"
    command_status=${PIPESTATUS[0]}
    set -e

    required=(
        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=1 target_revision=2"
        "[ENV_LIGHT_EDIT_CYCLE] edited terrain ready edit=close-skylight target_revision=2"
        "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=close-skylight terrain_revision=2"
        "[ENV_IRRADIANCE_CAPTURE] saved"
    )
    if [[ "$mode" == "closed" ]]; then
        required+=(
            "[ENV_LIGHT_EDIT_CYCLE] complete mode=closed final_terrain_revision=2"
        )
    else
        required+=(
            "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=2 target_revision=3"
            "[ENV_LIGHT_EDIT_CYCLE] edited terrain ready edit=reopen-skylight target_revision=3"
            "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=reopen-skylight terrain_revision=3"
            "[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision=3 spacing_voxels=$spacing"
            "[ENV_LIGHT_EDIT_CYCLE] density rebuild ready terrain_revision=3"
            "[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=3"
        )
    fi
    missing=()
    for marker in "${required[@]}"; do
        if ! grep -Fq "$marker" "$console"; then
            missing+=("$marker")
        fi
    done
    if (( command_status != 0 || ${#missing[@]} != 0 )) || [[ ! -f "$capture" ]]; then
        echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing mode=$mode status=$command_status missing_markers=${#missing[@]} capture_present=$([[ -f "$capture" ]] && echo yes || echo no)" >&2
        for marker in "${missing[@]}"; do
            echo "[DDGI_TERRAIN_EDIT] missing: $marker" >&2
        done
        grep -E "ENV_LIGHT_EDIT_CYCLE|runtime terrain invalidation" "$console" | tail -n 24 >&2 || true
        return 1
    fi
    if [[ "$mode" == "closed" ]]; then
        if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
            "$capture" --max-luminance 0.00001; then
            echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing post-close capture leaks light" >&2
            return 1
        fi
    elif ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing reopened portal capture is not lit" >&2
        return 1
    fi
    echo "[DDGI_TERRAIN_EDIT] PASS spacing=$spacing mode=$mode exact revisions ready before capture"
}

for spacing in "${spacings[@]}"; do
    if ! run_case "$spacing" terrain-edits-closed closed; then
        failures=$((failures + 1))
    fi
    if ! run_case "$spacing" terrain-edits reopened; then
        failures=$((failures + 1))
    fi
done

if $dry_run; then
    echo "[DDGI_TERRAIN_EDIT] dry-run matrix spacings=2 scenarios=closed,reopened"
    exit 0
fi

echo "[DDGI_TERRAIN_EDIT] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
