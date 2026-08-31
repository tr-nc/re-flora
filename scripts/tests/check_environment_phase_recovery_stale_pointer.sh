#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
checker="$repo_root/scripts/check_environment_phase_recovery.sh"
fixture_root="$(mktemp -d)"
trap 'rm -rf -- "$fixture_root"' EXIT

log_dir="$fixture_root/logs"
run_root="$fixture_root/run"
latest_pointer="$log_dir/latest-run-log.txt"
stale_log="$log_dir/re-flora-stale.log"
fake_cargo="$fixture_root/cargo"
mkdir -p "$log_dir" "$run_root"

stale_log="$(realpath -m "$stale_log")"
printf '%s\n' \
    "[RUN_LOG] path=$stale_log" \
    "[ENV_PHASE_RECOVERY] event=injected family=Static injected_frame=10" \
    "[ENV_PHASE_RECOVERY] event=retried family=Static injected_frame=10 retry_frame=11" \
    "Application exited successfully" \
    >"$stale_log"
printf '%s\n' "$stale_log" >"$latest_pointer"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$fake_cargo"
chmod +x "$fake_cargo"

output="$fixture_root/checker.log"
if RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_CASE=sealed \
    RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_RUN_ROOT="$run_root" \
    RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_LATEST_POINTER="$latest_pointer" \
    CARGO="$fake_cargo" \
    "$checker" >"$output" 2>&1; then
    echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] FAIL checker accepted an unchanged stale pointer" >&2
    sed -n '1,120p' "$output" >&2
    exit 1
fi

if ! grep -Fq "run-log pointer did not advance" "$output"; then
    echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] FAIL checker rejected for the wrong reason" >&2
    sed -n '1,120p' "$output" >&2
    exit 1
fi

echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] PASS"
