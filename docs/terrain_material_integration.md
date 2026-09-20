# Terrain material main integration

2026-09-20: integrate fixed Worker revision `8cf38212d0b949b6faf17cdee599a7a8fb8a2067` into main at `2bf6385079dced7b84c461106b576a3408199eb3`.

The sole conflict in `src/app/gui_config_loader.rs` retains both main DDGI experiment parameter migrations and the Terrain Material section migration. A read-only merge review confirmed main's continuous sampling, qualified aggregate history, fresh visibility, and diagnostics remain intact. Generated bindings were checked by `cargo check`; no additional generated diff resulted.

Validation in the main worktree:

- `cargo fmt --check`, `cargo check`, and `git diff --check`: passed.
- `cargo test`: 1047 binary and 4 library tests passed; 2 ignored, 0 filtered. Includes frozen material transport identity tests.
- `python3 scripts/run_slang_tests.py`: 18 passed; material failures=0, reseeded=128, neighbors_changed=128,128,128.
- `python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture`: 63 passed.
- `env -u WAYLAND_DISPLAY flock --close /tmp/re-flora-summer-gpu.lock cargo run --release -- --hidden --mute --auto-exit 0.5`: passed.
- Same-worktree `--latest-log` and `--tail-latest-log 200` used. Full canonical log `target/re-flora-logs/re-flora-20260920-222028.747-1019627.log` has no ERROR/panic/VUID, reports shutdown failures=0 and successful exit.
- GUI and camera configuration SHA-256 unchanged across smoke validation.

Evidence is retained under `target/terrain-material-integration/`, including copied Worker final evidence and both final source canonical logs. Source remained clean at its fixed revision throughout validation.

This validates integration correctness, not new performance acceptance. The accepted result is visual; historical macro-candidate GPU regressions/gates are not measurements of this final implementation. No visible game or release was launched.
