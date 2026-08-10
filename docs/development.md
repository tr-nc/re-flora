# Development Guide

This guide contains source-build and engineering information for Re: Flora. Players looking for a
packaged build should use [Playing Re: Flora](./playing.md) instead.

## Prerequisites

- [Rust](https://rustup.rs/) through the pinned `rust-toolchain.toml`
- A Vulkan-capable GPU with a current graphics driver
- Vulkan development packages:
  - Linux: `libvulkan-dev`, `vulkan-tools`, or the distribution equivalents
  - Windows: the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home)
  - macOS: Vulkan Loader and MoltenVK
- Slang 2025 compiler and shared library, supplied by a compatible Vulkan SDK or installed with the
  repository helper

If the Vulkan SDK does not provide the pinned-compatible Slang compiler, install it locally:

```bash
python3 scripts/install_slang.py
export SLANGC="$PWD/.tools/slang-2025.23/bin/slangc"
```

## Build and Run

Build and launch the game from the repository root:

```bash
cargo run --release
```

The first build takes longer because shaders compile from source. Use `cargo run --release -- --help`
to inspect supported command-line options.

For a short background smoke run that retains the normal window, Vulkan surface, and swapchain path:

```bash
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Run commands from the same worktree so log discovery and generated artifacts remain scoped to the
work that produced them.

## Validation

The normal validation ladder is:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

`cargo check` also regenerates shader-derived Rust structures. Do not edit generated files directly;
change their source and include generated diffs only when they follow from the source change.

Performance conclusions require measured release-mode app runs. Debug builds and unit tests are not
performance evidence. See [Performance Benchmarking](./performance-benchmarking.md).

## Technology Overview

| Area | Primary implementation |
| --- | --- |
| Rendering | Vulkan through `ash`, including ray-traced and raster paths |
| Windowing | `winit` |
| UI | `egui` |
| Spatial audio | `petalsonic` |
| Terrain and world generation | Voxel terrain plus procedural noise |
| Shaders | Slang compiled into embedded artifacts |

## Project Documentation

- [Contributing](../CONTRIBUTING.md) — pull-request scope and review expectations
- [Agent instructions](../AGENTS.md) — repository-specific agent and validation rules
- [Game Direction](./game_direction.md) — canonical product direction
- [First Garden Moment](./first_garden_moment.md) — current player-facing milestone
- [Roadmap](../ROADMAP.md) — implementation order
- [Packaging](./packaging.md) — release build and tag process
- [Technical References](./references.md) — rendering and simulation references
- [Camera Snapshots](./camera_snapshots.md) — repeatable capture commands

Technical progress documents record implementation evidence. They do not override the canonical game
direction or reorder product priorities.

## Release Packages

Official player packages are created by the release workflow and published on GitHub Releases. They
embed compiled shaders and bundle required non-system runtime libraries. See
[Packaging](./packaging.md) for maintainer instructions.
