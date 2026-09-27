# Cloud retirement integration — scope v1

The controller inspected the actual diff and worker evidence, accepted the independent
review, and fast-forwarded `main` from `7310bd82` through `c940746c`. The architecture
step remains a separate commit (`9fb15163`) before retirement (`9cc485e2`). Historical
capture error reporting (`7ea44622`), worker evidence (`0a5ac169`), and the reviewed
v11 documentation/fixture-whitespace correction (`c940746c`) remain separate commits.

## Actual main-worktree validation

Logs are under `target/cloud-integration/` in `/home/terence/code/re-flora`:

- `cargo fmt --check`: passed (`fmt.log`).
- `cargo check`: passed, with 12 existing warnings (`check.log`).
- `cargo test`: passed, 1,166 game tests plus four benchmark tests; four ignored
  (`test.log`). The first combined validation command reached its shell timeout
  during tests; a separate complete rerun supplied this successful result.
- `cargo build --release`: passed (`release-build.log`).
- `flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5`:
  passed (`smoke.log`), shutdown `failures=0`, no run errors or Vulkan validation errors.
- Same-worktree `--latest-log`, `--tail-latest-log 200`, and
  `python3 scripts/check_latest_run_log.py`: inspected/passed (`latest-log.txt`,
  `smoke-tail.log`, `log-check.log`).
- Candidate aggregate `git diff --check` passed after the review correction;
  build/run left tracked files unchanged.

The candidate's resize, active voxel-glass, no-shadows, 22 Slang policy tests, and
RFIRR v11 foliage evidence were inspected, not redundantly rerun after documentation
and whitespace-only changes. See [worker evidence](cloud-retirement-v1.md).

## Acceptance limits and evidence retention

This accepts the cloud retirement and interface refactor, not full DDGI transport
acceptance, performance acceptance, or a player visual review. The full DDGI suite
failed in the worker: 92 panics in 100 launches, with bounded original-baseline
reproductions of the documented failure classes. Extended Python/VKN/manifest checks
also retain the documented baseline limitations. These are not reported as passes.

During task-owned worktree cleanup, retain its `target/arch-validation/`,
`target/remove-validation/`, and `target/ddgi-transport-acceptance/` under the main
worktree's `target/cloud-integration/worker-evidence/` with those same directory names.
This is the replacement location for raw evidence paths in the earlier worker reports.
Committed evidence summaries remain in `docs/evidence/`. Other worker branches and
worktrees are outside this task's integration and cleanup scope.
