# AGENTS.md

## Development Notes

- Keep changes small and focused.
- Prefer measuring before guessing on performance work.
- Run `cargo check` after shader or Rust changes. It also regenerates shader-derived Rust structs.
- Do not edit generated files directly unless they are part of the generated output from a build/check.

## Basic Perf Test

Use the built-in CLI perf path:

```bash
cargo run --release -- --windowed --auto-exit 20 --perf
```

Do not pass `--present-mode` by default. The app auto-selects the best supported mode, preferring `MAILBOX`, then `FIFO`, then the first supported mode.

Useful log lines:

```text
[PERF] 60.0 fps at frame 1020
[PERF] frame 1020 total 17.82ms egui 0.11ms gpu+present 16.60ms
```

The windowed run may be display-capped near 60 FPS, so it is best for smoke tests and large regressions. For small GPU perf changes, prefer a dedicated benchmark path or GPU timestamps when available.

## Flora Instance Perf Note

The 4-byte flora instance vertex stride was slower than the 8-byte stride on the tested GPU/driver. Keep the aligned 8-byte instance vertex layout unless a replacement path, such as storage-buffer instance fetch, benchmarks better.
