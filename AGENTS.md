# AGENTS.md

## Development Notes

- Keep changes small and focused.
- Prefer measuring before guessing on performance work.
- Run `cargo check` after shader or Rust changes. It also regenerates shader-derived Rust structs.
- Do not edit generated files directly unless they are part of the generated output from a build/check.

## Basic Perf Test

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
