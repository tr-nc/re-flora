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
app_output="${artifact}.app.log"
analyzer="${REFLORA_ANALYZER:-$repo_root/scripts/analyze_lighting_mode_acceptance.py}"
log_pointer="${REFLORA_LOG_POINTER:-$repo_root/target/re-flora-logs/latest-run-log.txt}"
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

if [[ -e "$artifact" || -e "$app_output" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=ERROR reason=output-already-exists artifact=%s\n' \
        "$artifact" >&2
    exit 2
fi
mkdir -p "$(dirname "$artifact")"
run_started_epoch="$(date +%s)"
previous_run_log=""
if [[ -f "$log_pointer" ]]; then
    previous_run_log="$(sed -n '1p' "$log_pointer")"
fi

if ! RUST_LOG="warn,re_flora::app::core::lighting_mode_acceptance=info" \
    "${command[@]}" >"$app_output" 2>&1; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED log=%s\n' "$app_output" >&2
    tail -n 80 "$app_output" >&2 || true
    exit 3
fi
run_log=""
if [[ -f "$log_pointer" ]] && [[ "$(stat -c %Y "$log_pointer")" -ge "$run_started_epoch" ]]; then
    run_log="$(sed -n '1p' "$log_pointer")"
fi
if [[ -z "$run_log" || "$run_log" == "$previous_run_log" || ! -f "$run_log" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=missing-artifact-or-run-log\n' >&2
    exit 3
fi
runtime_red="$(
    rg --no-filename -o '\[LIGHTING_MODE_ACCEPTANCE\] verdict=RED reason=.*' \
        "$app_output" "$run_log" 2>/dev/null | awk '!seen[$0]++' || true
)"
if [[ -n "$runtime_red" ]]; then
    printf '%s\n' "$runtime_red" >&2
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_REJECTED reason=see-app-verdict\n' >&2
    exit 3
fi
if rg -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >/dev/null 2>&1; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=error-marker\n' >&2
    rg -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >&2 || true
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
