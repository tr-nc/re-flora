#!/usr/bin/env bash

# Execute one capture process while preserving both its console stream and canonical run log.
# Arguments before `--` are forwarded to validate_capture_process_evidence.py.
run_capture_with_process_evidence() {
    local console="$1"
    local capture="$2"
    local rust_log="$3"
    shift 3
    local validator_args=()
    while [[ "${1:-}" != "--" ]]; do
        validator_args+=("$1")
        shift
    done
    shift

    set +e
    RUST_LOG="$rust_log" "$@" 2>&1 | tee "$console"
    local pipeline_status=("${PIPESTATUS[@]}")
    set -e
    local command_status="${pipeline_status[0]}"
    local tee_status="${pipeline_status[1]}"
    if (( command_status != 0 || tee_status != 0 )); then
        echo "capture process pipeline failed app_status=$command_status tee_status=$tee_status" >&2
        return 1
    fi
    if [[ ! -f "$capture" ]]; then
        echo "capture process produced no artifact: $capture" >&2
        return 1
    fi
    "$repo_root/scripts/validate_capture_process_evidence.py" \
        "${validator_args[@]}" "$console"
}
