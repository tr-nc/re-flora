# Native loading splash — approved B direction

The approved research-board direction replaces the initial checkerboard/white-title-panel implementation. Final design and primary-source research are recorded in [`../research/splash-art-direction.md`](../research/splash-art-direction.md).

## Shipped presentation

- B layout: one dark background with fine grid lines, small leaf marks in quiet cells, alternating yellow five-petal and cream six-petal flowers.
- Title occupies exactly 4×1 logical cells, with no independent white backing. The grid scales uniformly and extends at the viewport edges for other aspect ratios.
- Three palettes (`forest`, `moss`, `pond`), randomly selected once when the loading presentation is created and retained throughout loading. Selection is not persisted and may repeat on consecutive launches.
- Four held poses: −10°, 0°, +10°, 0°, 1.2 seconds each. Four spatial phase groups are offset by 0.45 seconds. No full-circle rotation or interpolated movement.
- Local/model-space pixelization: the original 16×16 flower pixels become rigid colored quads; the four pose meshes are cached once. No new image textures, sampler changes, shaders or borrowed art assets.
- Real loading progress appears directly below the title as a thin underline, aligned to the visible glyph bounds. Text plus underline are centered together inside the merged grid cell. The old bottom progress line is removed.

`src/app/core/loading/splash.rs` owns palette selection, geometry, layout and presentation; existing `LoadingState`, terrain/build/collider progress and finalization remain unchanged. No new Debug controls or saved settings.

## Validation

- `cargo fmt --check`: passed.
- `cargo check`: passed, with existing warnings; no generated-file changes.
- `cargo test app::core::loading::`: 6 passed.
- `cargo test`: 4 + 1276 passed, 4 ignored.
- Deterministic tests cover four-frame holds/looping, phase offsets, title grid alignment and underline containment at 960×576, 1920×1080, 3440×1440 and 390×844, all three palettes, rigid local pixel geometry, valid mesh batching and palette stability across repaints.
- `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed.
- Native log: `target/re-flora-logs/re-flora-20261003-031454.675-495696.log`.
  - `[LOADING][SPLASH] layout=B palette=moss local_pixels=16 title_cells=4x1 motion=sway poses=-10,0,10,0 step_seconds=1.2`
  - No ERROR/VUID; shutdown `failures=0`, application exited successfully.
- The before/after diff of user-owned `config/gui.toml` is identical.

The native hidden run validates startup/rendering correctness, not a new human visual review or a performance claim. No visible game was automatically launched. Temporary HTML/JavaScript and cached reference images were removed; source links, visual reasoning and final parameters remain in the design document.
