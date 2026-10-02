# Native loading splash

Integrated the approved green-checkerboard HTML study into the existing native loading render path. The HTML experiment was removed after integration.

## Presentation

- Full-window dark greens `#294f40` / `#305746`, 144-point cells (112 on narrow windows).
- Alternating yellow five-petal and white seven-petal flowers, viewed from above.
- Local/model-space pixelization: sample each flower once on a 32×32 grid, represent occupied pixels as colored unit-square quads, then scale and rotate the mesh as a whole. Pixels are never rebuilt in screen space. No new textures, sampler changes, shaders or external assets.
- Rotation advances in held 15-degree steps: approximately 1.07 seconds per step, 25.6 seconds per revolution. Opposite directions and staggered step times; no growth, fades or interpolated rotation.
- Existing Pixelify Sans title, white backing, center at 36.5% of height; no subtitle or status labels.
- Three-point progress line at 86% of height, driven by the existing terrain/build/collider progress and completion state. No simulated progress or artificial minimum loading duration.

`src/app/core/loading/splash.rs` owns loading-only presentation and cached local flower meshes. `LoadingState` owns that presentation until the normal loading-to-game handoff. Existing loading work, swapchain submission and finalization remain unchanged. No new Debug controls or saved settings.

## Validation

- `cargo fmt --check`: passed.
- `cargo check`: passed; existing warnings, no generated-file changes.
- `cargo test`: 4 + 1274 passed, 4 ignored. New deterministic tests cover discrete rotation/looping, rigid local pixel quads and batched mesh indices/colors.
- `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed.
- Native run log: `target/re-flora-logs/re-flora-20261003-022311.008-481357.log`; Pixelify Sans loaded, no ERROR/VUID/panic, shutdown `failures=0`, application exited successfully.

The hidden run validates the native rendering and startup path, not a human visual review. No visible game was automatically launched. User changes in `config/gui.toml` remain uncommitted and untouched.
