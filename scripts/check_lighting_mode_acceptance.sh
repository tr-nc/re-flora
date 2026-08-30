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
analyzer="$repo_root/scripts/analyze_lighting_mode_acceptance.py"
command=(
    "$cargo_bin" run --release --
    --hidden --mute
    --lighting-mode-acceptance "$artifact"
)
analysis=(python3 "$analyzer" "$artifact")

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

if ! RUST_LOG="warn,re_flora::app::core::lighting_mode_acceptance=info" \
    "${command[@]}" >"$app_output" 2>&1; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED log=%s\n' "$app_output" >&2
    tail -n 80 "$app_output" >&2 || true
    exit 3
fi
run_log="$(sed -n 's/.*Run log saved to //p' "$app_output" | tail -n 1)"
if [[ -z "$run_log" || ! -f "$run_log" || ! -s "$artifact" ]]; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=missing-artifact-or-run-log\n' >&2
    exit 3
fi
if rg -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >/dev/null 2>&1; then
    printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=APP_FAILED reason=error-marker\n' >&2
    rg -n 'ERROR|panic|VUID-|validation error|stale readback' "$app_output" "$run_log" >&2 || true
    exit 3
fi

"${analysis[@]}"
printf '[LIGHTING_MODE_ACCEPTANCE_RUNNER] verdict=GREEN artifact=%s log=%s\n' \
    "$artifact" "$run_log"
