# egui 0.34.1 — opt-in whole-cell panel sizing

Source: crates.io `egui` 0.34.1, upstream commit
`a01193d032ae89e1ae80c56834993437b9467256` (`crates/egui`).
Repository: https://github.com/emilk/egui
Original crate checksum: `f34aaf627da598dfadd64b0fee6101d22e9c451d1e5348157312720b7f459f0f`.
License: MIT OR Apache-2.0; both upstream licenses are included.

Only `src/containers/frame.rs` and `src/containers/window.rs` are locally modified.
`Frame` preparation can reserve whole-size-step padding before allocation/painting;
`Window` can opt into outer-frame size steps, including its title and collapsed
state. Size steps do not quantize movement, text, or individual controls. Zero
steps preserve upstream behavior. Re: Flora enables this only in its explicit
matte-glass adapters, using the renderer's DPI-rounded cell pitch.

This belongs at layout/resize ownership: extending only a painted background
would leave the allocation, resize handles and hit rectangle at a different size.
The upstream public window adapter cannot intercept its private frame completion
or edge-resize request before layout. The patch keeps those responsibilities in
egui instead of reimplementing windows or relying on private persisted-state IDs
from application code. Replace this local patch with an equivalent upstream API
when available. Do not add unrelated egui changes here.

Behavioral regression tests use the application's real matte adapters and egui
pointer events in `src/egui_renderer/matte_glass/grid_tests.rs`; ordinary window
movement is also covered by `src/egui_renderer/composition/tests.rs`.
