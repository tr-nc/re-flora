# Re: Flora

> Cultivate your own island ecosystem — a meditative voxel gardening game.

![Re: Flora](./demo/img/splash.png)

Re: Flora is a cozy voxel game prototype about shaping terrain, planting flora, and building a calm island atmosphere. Built with Vulkan ray tracing in Rust.

## Getting Started

**Prerequisites for building from source:**

- [Rust](https://rustup.rs/) via the pinned [`rust-toolchain.toml`](./rust-toolchain.toml)
- Vulkan-capable GPU with up-to-date drivers (RTX not required)
- Vulkan development packages:
  - Linux: `libvulkan-dev` + `vulkan-tools` (or distro equivalent)
  - Windows: [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#vulkansdk)
- Slang 2025 compiler and shared library, supplied by a compatible Vulkan SDK or installed with `python3 scripts/install_slang.py`

Official release packages bundle Steam Audio and other non-system native libraries. Players only need a Vulkan-capable GPU/driver; the macOS package also bundles MoltenVK.

If your Vulkan SDK does not provide Slang, install the pinned compiler and point the build at it:

```bash
python3 scripts/install_slang.py
export SLANGC="$PWD/.tools/slang-2025.23/bin/slangc"
```

**Build and run:**

```bash
cargo run --release
```

The first build takes a while — shaders compile from source.

## Controls

Basic controls:

- **WASD** or **arrow keys** to move the edit camera.
- **Q/E** to lower/raise the edit camera.
- **MMB drag** to pan, **RMB drag** to rotate, and **mouse wheel** to zoom.
- **G** to toggle walk/fly mode.
- **R** to toggle the config panel.
- **Esc** to quit.

Most values can be tuned live from the config panel. Runtime defaults are stored in [`config/gui.toml`](./config/gui.toml).

## Tech Stack

| Domain    | Crate                                      |
| --------- | ------------------------------------------ |
| Rendering | `ash` (Vulkan) with ray tracing extensions |
| Windowing | `winit`                                    |
| UI        | `egui`                                     |
| Audio     | `petalsonic`                               |
| Terrain   | `fastnoise-lite` + `noise`                 |

## Documentation

- [Camera snapshots](./docs/camera_snapshots.md)
- [Packaging paid builds](./docs/packaging.md)
- [Technical references](./docs/references.md)
- [Game direction](./docs/game_direction.md)
- [Inspirations and art direction](./docs/inspirations.md)
- [Roadmap](./ROADMAP.md)
- [Contributing](./CONTRIBUTING.md)

## Acknowledgements

- [egui-ash-renderer](https://github.com/adrien-ben/egui-ash-renderer) — `ash` + `egui` integration
- TheMaister, Khronos Group, and the broader graphics programming community for Vulkan guidance

## License

Dual-licensed:

- **Code:** [GPL-3.0](./LICENSE)
- **Assets** (art, audio, images, config): [CC BY-NC-SA 4.0](./LICENSE-ASSETS)
