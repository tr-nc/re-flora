# macOS trackpad camera input

## Verified input capability

Before changing game input, a temporary AppKit window recorded real user gestures:
284 precision scroll events (including 165 with both axes nonzero and 82 momentum
events), plus 134 pinch events with both positive and negative magnification.
The probe received all four signed scroll directions. These were raw `NSEvent`
samples, not synthetic events or a winit capture. No UI-region samples were
recorded in that session; UI behavior is covered separately by the real-egui test.
The probe source/binary/logs were temporary files, not game assets or dependencies.

The installed winit 0.30.13 macOS backend maps `scrollingDeltaX/Y` with
`hasPreciseScrollingDeltas` to physical `PixelDelta(x, y)` and maps
`magnifyWithEvent` to `PinchGesture`. The game previously discarded scroll X and
converted scroll Y into wheel lines. Neither native event capture nor egui-winit
needs to be patched.

## Behavior and ownership

- In orbit editing, macOS precision scrolling over the world uses both signed
  physical axes for camera rotation, sharing the mouse-drag rotation function,
  smoothing, and elevation limits. It keeps the current orbit focus and does not
  synthesize a button press, edit terrain, or plant anything.
- Pinch changes the target distance proportionally; spreading zooms in, pinching
  together zooms out. Pending dolly is included so event batching does not change
  the target. The existing zoom/walk transitions still own mode changes.
- UI hit testing and active UI drags take priority. UI events reach egui unchanged;
  world gestures are intercepted before egui so a camera pinch cannot also zoom UI.
  UI-owned gesture segments cannot fall through into world/tool handlers even if
  the pointer moves away before their end.
- Existing Shift/Control tool-size/height scrolling is preserved. Ordinary
  `LineDelta` wheel events and non-macOS input retain their existing paths.
- Focus loss, leaving the window, mode changes, unavailable camera controls, and
  gesture cancellation clear pending gesture movement. Nonfinite camera input is
  ignored. No settings or additional Debug controls were introduced.

The router owns scroll and pinch independently. winit can report both macOS
`mayBegin` and `began` as `Started`; repeated starts do not steal UI ownership.
The backend also flattens touch/momentum phases, so the app cannot separately
identify all momentum packets. It preserves delivered motion, re-evaluates the
recipient when a new segment begins, and does not drop queued rotation when a
momentum segment starts. Direction follows the system's natural-scroll setting.

`PixelDelta` identifies precision input, **not a unique hardware class**. A
precision/touch-sensitive mouse can emit the same event and take the trackpad
path. Ordinary notched mouse wheels remain unaffected.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo build --release`.
- `cargo test --bin re-flora camera_control`: 34 passed.
- `cargo test --bin re-flora panel_input_tests`: 4 passed, including actual Debug
  scroll-area movement and unchanged egui zoom input at a 2× UI scale.
- Hidden muted Release smoke (`--hidden --mute --auto-exit 0.5`) passed; the run
  log exited with `failures=0` and no ERROR/VUID/panic diagnostics.
- New tests cover signed/diagonal deltas, platform and wheel separation, pointer
  ownership, repeated starts, cancellation, shared mouse/trackpad rotation limits,
  momentum phase continuity, and pinch direction/event-batching equivalence.
- Full Rust suite: 1,721 passed, 9 failed, 13 ignored, plus 4 passing collision
  tests. The nine failures are the previously confirmed saved-default/raster-tree
  baseline failures, not input regressions.

Real hardware established event availability before integration. Automated tests
establish routing and camera math, not final physical gesture feel inside the
running game; that remains a manual try-out on the Release build.
