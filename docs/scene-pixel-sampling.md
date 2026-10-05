# Scene pixel sampling

Open **Debug → Scene Pixel Sampling**. These are saved controls, not game-startup flags.

## Final pixel resolution

Ratios count physical screen pixels, not the reduction of each dimension. The grid follows the actual game window's physical resolution (the screen resolution when fullscreen), and updates on resize. At 2560×1440:

| Selection | Physical pixels per scene pixel | Final scene grid |
| --- | --- | --- |
| 1:1 | 1×1 | 2560×1440 |
| 4:1 | 2×2 | 1280×720 |
| 16:1 | 4×4 | 640×360 |
| 64:1 (original/default) | 8×8 | 320×180 |

Dimensions round **up** to cover the window. Display uses exact integer-sized blocks; only the final block at an edge may be partially cropped. For example, a 1023×767 window at 64:1 has a 128×96 grid, with the last column/row displaying seven physical pixels. The rounded grid is not stretched. UI remains native-resolution.

## Antialiasing

The checkbox switches between one sample per final pixel (unchecked) and supersampling (checked). Quality requests **4×** (2×2) or **16×** (4×4) color samples per final pixel. All samples are averaged in linear HDR before tone mapping; changing quality does not change the final pixel grid.

Sampling is deliberately bounded at the native-equivalent resolution, including padding for partial edge blocks:

| Final resolution | Effective AA when requesting 4× / 16× |
| --- | --- |
| 1:1 | Bypassed / bypassed |
| 4:1 | 4× / 4× |
| 16:1 or 64:1 | 4× / 16× |

The requested quality and checkbox remain saved even when bypassed/capped, and apply again when selecting a coarser grid. A capped or disabled quality preference alone does not rebuild GPU resources. Actual sampling changes drain submitted frames, replace scene attachments without recreating the native swapchain, and invalidate temporal history; they may briefly pause.

Use the panel's normal **Save** action. Older saves retain their AA checkbox and receive the original 64:1 grid and 4× quality defaults. Very small scene grids remain explicitly 2D textures, including one-pixel-high attachments.

Supersampling can soften edges and outlines. It cannot restore independently identifiable detail smaller than the final pixel grid. Correctness checks are not visual or performance acceptance; those remain separate user review and Release measurement steps.

## Validation

- Rust tests cover presets, odd/tiny extents, unchanged grids/aspects across AA modes, quality capping, migration, saved preferences, and searchable ownership.
- `shader/tests/scene_pixel_filter_test.slang` checks exact blocks, edge coverage, all 4/16 taps, bounds, and linear HDR averaging.
- After `cargo build --release`, `node scripts/validate-scene-supersampling.mjs` validates native startup and live transitions through all presets and both qualities with Vulkan synchronization validation. It includes 1023×767, 9×8, and 1280×720 windows, depth outlines, and unchanged saved-file hashes. Logs and summary are under `target/scene-supersampling/`.
