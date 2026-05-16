# AGENTS.md

## Development Notes

- Keep changes small and focused.
- Prefer measuring before guessing on performance work.
- For performance work, release-mode app benchmarks are authoritative; debug builds and unit tests are not performance evidence.
- Run `cargo check` after shader or Rust changes. It also regenerates shader-derived Rust structs.
- Do not edit generated files directly unless they are part of the generated output from a build/check.

## Validation Policy

Keep `cargo test` fast and deterministic. Unit tests are valuable for pure logic guardrails such as chunk math, queue behavior, allocator correctness, CPU voxel sampling, and revision tracking. Do not turn long-running random benchmarks, GPU/window/audio checks, or perf experiments into normal unit tests; make them lightweight, `#[ignore]`, bench targets, scripts, or hidden app runs instead.

Use real app runs plus logs for end-to-end verification, especially for Vulkan, audio, windowing, terrain editing, water collider refreshes, and performance regressions. A good default validation ladder is:

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 0.5
```

Inspect the generated temp run log after hidden runs. Prefer checking concrete expected log lines, hashes, dimensions, timings, and absence of errors over relying on visual behavior when running headless. Use the built-in log helpers instead of guessing file names:

```bash
cargo run --release -- --latest-log
cargo run --release -- --tail-latest-log 200
```

## Basic Perf Test

Benchmark in release mode is king. Use `cargo run --release` hidden app runs with logs for performance decisions.

Before running the app, source the shell environment:

```bash
source ~/.zshrc
```

Discover CLI usage and testing recipes from the binary:

```bash
cargo run --release -- --help
cargo run --release -- -h
```

Prefer `--hidden` for background validation; it keeps the normal native window, Vulkan surface, and swapchain path, but leaves the window invisible. Do not pass `--present-mode` by default; let the app auto-select it.

## Flora Instance Perf Note

The 4-byte flora instance vertex stride was slower than the 8-byte stride on the tested GPU/driver. Keep the aligned 8-byte instance vertex layout unless a replacement path, such as storage-buffer instance fetch, benchmarks better.
