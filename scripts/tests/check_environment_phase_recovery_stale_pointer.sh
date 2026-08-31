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
other_stale_log="$log_dir/re-flora-other-stale.log"
fake_cargo="$fixture_root/cargo"
mkdir -p "$log_dir" "$run_root"

stale_log="$(realpath -m "$stale_log")"
other_stale_log="$(realpath -m "$other_stale_log")"
write_valid_log() {
    local path="$1"
    local injected_frame="$2"
    printf '%s\n' \
        "[RUN_LOG] path=$path" \
        "[ENV_PHASE_RECOVERY] event=injected family=Static injected_frame=$injected_frame" \
        "[ENV_PHASE_RECOVERY] event=retried family=Static injected_frame=$injected_frame retry_frame=$((injected_frame + 1))" \
        "Application exited successfully" \
        >"$path"
}

write_valid_log "$stale_log" 10
write_valid_log "$other_stale_log" 20
printf '%s\n' "$stale_log" >"$latest_pointer"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'if [[ -n "${FAKE_NEXT_LOG:-}" ]]; then' \
    '    printf "%s\n" "$FAKE_NEXT_LOG" >"$RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_LATEST_POINTER"' \
    'fi' \
    'exit 0' \
    >"$fake_cargo"
chmod +x "$fake_cargo"

expect_rejected() {
    local label="$1"
    local next_log="$2"
    local expected="$3"
    local output="$fixture_root/$label.log"
    printf '%s\n' "$stale_log" >"$latest_pointer"
    if RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_CASE=sealed \
        RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_RUN_ROOT="$run_root" \
        RE_FLORA_ENVIRONMENT_PHASE_RECOVERY_LATEST_POINTER="$latest_pointer" \
        FAKE_NEXT_LOG="$next_log" \
        CARGO="$fake_cargo" \
        "$checker" >"$output" 2>&1; then
        echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] FAIL checker accepted $label" >&2
        sed -n '1,120p' "$output" >&2
        exit 1
    fi
    if ! grep -Fq "$expected" "$output"; then
        echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] FAIL checker rejected $label for the wrong reason" >&2
        sed -n '1,120p' "$output" >&2
        exit 1
    fi
}

expect_rejected "unchanged-pointer" "" "run-log pointer did not advance"
expect_rejected "repointed-old-log" "$other_stale_log" "run log was not created by this invocation"

echo "[ENV_PHASE_RECOVERY_STALE_POINTER_TEST] PASS"
