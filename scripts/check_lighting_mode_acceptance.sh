#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
    shift
fi
artifact="${1:-target/lighting-mode-acceptance/r13-e2.rflma}"
if [[ $# -gt 1 ]]; then
    printf 'usage: %s [--dry-run] [artifact.rflma]\n' "$0" >&2
    exit 2
fi

cargo_bin="${REFLORA_CARGO:-cargo}"
rg_bin="${REFLORA_RG:-rg}"
python_bin="${REFLORA_PYTHON:-python3}"
timeout_bin="${REFLORA_TIMEOUT:-timeout}"
timeout_seconds="${REFLORA_LIGHTING_MODE_ACCEPTANCE_TIMEOUT_SECONDS:-1200}"
app_output="${artifact}.app.log"
analyzer="${REFLORA_ANALYZER:-$repo_root/scripts/analyze_lighting_mode_acceptance.py}"
command=(
    "$cargo_bin" run --release --
    --hidden --mute
    --lighting-mode-acceptance "$artifact"
)
analysis=("$analyzer" "$artifact")

printf 'cargo-command='
printf ' %q' "${command[@]}"
printf '\n'
printf 'analyzer-command='
printf ' %q' "${analysis[@]}"
printf '\n'
if $dry_run; then
    exit 0
fi

require_command() {
    local label="$1"
    local command_name="$2"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=ERROR reason=missing-dependency dependency=%s command=%s\n' \
            "$label" "$command_name" >&2
        exit 2
    fi
}

require_command cargo "$cargo_bin"
require_command rg "$rg_bin"
require_command python "$python_bin"
require_command timeout "$timeout_bin"
require_command analyzer "$analyzer"
require_command awk awk
require_command tail tail
if [[ ! "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=ERROR reason=invalid-timeout seconds=%s\n' \
        "$timeout_seconds" >&2
    exit 2
fi

if [[ -e "$artifact" || -e "$app_output" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=ERROR reason=output-already-exists artifact=%s\n' \
        "$artifact" >&2
    exit 2
fi
mkdir -p "$(dirname "$artifact")"

set +e
RUST_LOG="warn,re_flora::run_log_binding=info,re_flora::app::core::lighting_mode_acceptance=info" \
    "$timeout_bin" --signal=TERM --kill-after=15s "${timeout_seconds}s" \
    "${command[@]}" >"$app_output" 2>&1
app_status=$?
set -e

mapfile -t run_log_markers < <(
    "$rg_bin" --no-filename -o '\[RUN_LOG\] path=.*' "$app_output" 2>/dev/null || true
)
if [[ ${#run_log_markers[@]} -ne 1 ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=run-log-marker-count count=%s app-status=%s log=%s\n' \
        "${#run_log_markers[@]}" "$app_status" "$app_output" >&2
    if [[ $app_status -ne 0 ]]; then
        tail -n 80 "$app_output" >&2 || true
    fi
    exit 3
fi
run_log="${run_log_markers[0]#'[RUN_LOG] path='}"
if [[ "$run_log" != /* || ! -f "$run_log" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=run-log-marker-path app-status=%s path=%s log=%s\n' \
        "$app_status" "$run_log" "$app_output" >&2
    if [[ $app_status -ne 0 ]]; then
        tail -n 80 "$app_output" >&2 || true
    fi
    exit 3
fi
runtime_red="$(
    "$rg_bin" --no-filename -o '\[LIGHTING_MODE_ACCEPTANCE\] verdict=RED reason=.*' \
        "$app_output" "$run_log" 2>/dev/null | awk '!seen[$0]++' || true
)"
if [[ -n "$runtime_red" ]]; then
    printf '%s\n' "$runtime_red" >&2
    if [[ $app_status -eq 124 ]]; then
        printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=timeout app-status=%s seconds=%s path=%s\n' \
            "$app_status" "$timeout_seconds" "$run_log" >&2
        exit 3
    fi
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_REJECTED reason=see-app-verdict app-status=%s\n' \
        "$app_status" >&2
    exit 3
fi
if [[ $app_status -eq 124 ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=timeout app-status=%s seconds=%s path=%s\n' \
        "$app_status" "$timeout_seconds" "$run_log" >&2
    exit 3
fi
if "$rg_bin" -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >/dev/null 2>&1; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=error-marker\n' >&2
    "$rg_bin" -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >&2 || true
    exit 3
fi
if [[ $app_status -ne 0 ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=app-command-nonzero app-status=%s log=%s\n' \
        "$app_status" "$app_output" >&2
    tail -n 80 "$app_output" >&2 || true
    exit 3
fi
if [[ ! -s "$artifact" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=missing-artifact\n' >&2
    exit 3
fi

set +e
analysis_output="$("${analysis[@]}" 2>&1)"
analysis_status=$?
set -e
if [[ $analysis_status -ne 0 ]]; then
    printf '%s\n' "$analysis_output" >&2
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=ANALYZER_FAILED reason=see-analyzer-verdict\n' >&2
    exit "$analysis_status"
fi
printf '%s\n' "$analysis_output"
printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=GREEN artifact=%s log=%s\n' \
    "$artifact" "$run_log"
