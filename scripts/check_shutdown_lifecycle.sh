#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

auto_exit="${RE_FLORA_SHUTDOWN_AUTO_EXIT:-0}"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="${RE_FLORA_SHUTDOWN_OUTPUT_DIR:-$repo_root/target/shutdown-lifecycle}/$run_id"
stdout_log="$run_dir/stdout.log"
stderr_log="$run_dir/stderr.log"
latest_pointer="$repo_root/target/re-flora-logs/latest-run-log.txt"

mkdir -p "$run_dir"
cargo build --release --manifest-path "$repo_root/Cargo.toml"

set +e
cargo run --quiet --release --manifest-path "$repo_root/Cargo.toml" -- \
    --hidden --mute --auto-exit "$auto_exit" \
    >"$stdout_log" 2>"$stderr_log"
command_exit=$?
set -e

if (( command_exit != 0 )); then
    echo "[SHUTDOWN_LIFECYCLE] FAIL process_exit=$command_exit" >&2
    tail -n 80 "$stdout_log" >&2 || true
    tail -n 80 "$stderr_log" >&2 || true
    exit "$command_exit"
fi

if [[ ! -s "$latest_pointer" ]]; then
    echo "[SHUTDOWN_LIFECYCLE] FAIL missing latest run-log pointer" >&2
    exit 1
fi
latest_log="$(<"$latest_pointer")"
if [[ ! -f "$latest_log" ]]; then
    echo "[SHUTDOWN_LIFECYCLE] FAIL missing latest run log: $latest_log" >&2
    exit 1
fi

fatal_pattern='Validation Error|VUID-|managed GPU job|libc\+\+abi|uncaught exception|panicked|panic'
for output in "$stderr_log" "$stdout_log" "$latest_log"; do
    if grep -Eiq "$fatal_pattern" "$output"; then
        echo "[SHUTDOWN_LIFECYCLE] FAIL fatal marker in $output" >&2
        grep -Ein "$fatal_pattern" "$output" >&2 || true
        exit 1
    fi
done

if ! grep -Fq "Application exited successfully" "$latest_log"; then
    echo "[SHUTDOWN_LIFECYCLE] FAIL missing successful application exit marker" >&2
    tail -n 80 "$latest_log" >&2
    exit 1
fi

echo "[SHUTDOWN_LIFECYCLE] PASS process_exit=0 auto_exit=$auto_exit run_log=$latest_log"
