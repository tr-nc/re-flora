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
for spacing in "${spacings[@]}"; do
    capture="$run_dir/terrain-edits-spacing${spacing}-reopened.rfirr"
    console="$run_dir/terrain-edits-spacing${spacing}.console.log"
    command=(
        cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" --
        --hidden --mute --no-flora --no-particles --no-god-rays --no-lens-flare --no-clouds
        --environment-lighting-test-scene terrain-edits
        --environment-probe-spacing-voxels "$spacing"
        --environment-irradiance-capture "$capture"
        --auto-exit "$auto_exit"
    )
    if $dry_run; then
        printf '%q ' "${command[@]}"
        printf '\n'
        continue
    fi

    echo "[DDGI_TERRAIN_EDIT] spacing=$spacing running close/reopen lifecycle"
    set +e
    RUST_LOG="warn,re_flora::app::core::environment_irradiance_capture=info,re_flora::app::core::environment_lighting_test_scene=info" \
        "${command[@]}" 2>&1 | tee "$console"
    command_status=${PIPESTATUS[0]}
    set -e

    log_path="$(cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" -- --latest-log)"
    if [[ -z "$log_path" || ! -f "$log_path" ]]; then
        echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing could not locate run log" >&2
        failures=$((failures + 1))
        continue
    fi
    required=(
        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] edited terrain ready edit=close-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=close-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] edited terrain ready edit=reopen-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=reopen-skylight"
        "[ENV_LIGHT_EDIT_CYCLE] complete"
        "[ENV_IRRADIANCE_CAPTURE] saved"
    )
    missing=()
    for marker in "${required[@]}"; do
        if ! grep -Fq "$marker" "$log_path"; then
            missing+=("$marker")
        fi
    done
    if (( command_status != 0 || ${#missing[@]} != 0 )) || [[ ! -f "$capture" ]]; then
        echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing status=$command_status missing_markers=${#missing[@]} capture_present=$([[ -f "$capture" ]] && echo yes || echo no)" >&2
        for marker in "${missing[@]}"; do
            echo "[DDGI_TERRAIN_EDIT] missing: $marker" >&2
        done
        grep -E "ENV_LIGHT_EDIT_CYCLE|runtime terrain invalidation" "$log_path" | tail -n 20 >&2 || true
        failures=$((failures + 1))
        continue
    fi
    if ! "$repo_root/scripts/analyze_environment_irradiance_capture.py" \
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_TERRAIN_EDIT] FAIL spacing=$spacing reopened portal capture is not lit" >&2
        failures=$((failures + 1))
        continue
    fi
    echo "[DDGI_TERRAIN_EDIT] PASS spacing=$spacing initial-close-reopen revisions ready before final capture"
done

if $dry_run; then
    echo "[DDGI_TERRAIN_EDIT] dry-run matrix spacings=2 lifecycle=initial-close-reopen"
    exit 0
fi

echo "[DDGI_TERRAIN_EDIT] output=$run_dir failures=$failures"
if (( failures != 0 )); then
    exit 1
fi
