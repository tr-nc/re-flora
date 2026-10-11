# egui-winit 0.34.1 — lazy native clipboard initialization

This is the crates.io 0.34.1 source, upstream commit
`a01193d032ae89e1ae80c56834993437b9467256` (`crates/egui-winit`),
MIT OR Apache-2.0 licensed. Both licenses are included.
Original crate checksum: `11a2881b2bf1a305e413e644af63f836737a33d85077705ff808e88f902ff742`.
Repository: https://github.com/emilk/egui

Only `src/clipboard.rs` differs from the published implementation: `arboard`
is initialized lazily on its first operation, caching either success or failure.
Upstream eagerly connects to both arboard (X11 on Linux) and smithay (Wayland),
even though successful Wayland text operations never use arboard. On a native
Wayland session without accessible XWayland, that unused connection emits a
startup timeout warning.

The clipboard owner now initializes only the backend an operation needs.
Smithay text handling is unchanged. Native text fallback and image copies still
use arboard, including after a failed smithay read. A failed arboard connection
is attempted once per clipboard instance; the existing app-local text fallback
remains available. Genuine failures on requested operations are still logged.
No application logging filters, environment overrides or platform features change.
Re: Flora's separate screenshot clipboard implementation is unchanged.

Local validation: the startup test failed against unpatched 0.34.1 with the
reported timeout, then passed with the original, unset and healthy isolated X11
DISPLAY values. Isolated rootless XWayland verified native text, image-first
copy and paste-first initialization; an invalid DISPLAY verified cached failure
and local text fallback. `cargo fmt --check`, `cargo check`, the Release build
and hidden muted app startup passed; the original egui clipboard warning and
rendering errors were absent from the app run log. Focused Wayland text
copy/paste was not manually exercised (its smithay implementation is unchanged).

The full debug test run exceeded 180 seconds in the existing
`climbing_plants::tests::rotating_tips_climb_holes_ledges_recesses_and_slopes_without_penetration`
test. Re-running with only that test excluded passed 1,577 tests. Initial validation
also emitted `Authorization required, but no authorization protocol specified`
from a separate X11 connection. That was later traced to expired inherited
XAUTHORITY credentials used by NVIDIA's Vulkan ICD and addressed at the application
startup boundary; see [the diagnosis and regression evidence](../../docs/debug_display_authorization.md).
This clipboard patch remains independent of that session-credential recovery.

Remove this patch when an upstream release contains the equivalent fix.

## Regression checks

`tests/clipboard_initialization.rs` uses the real egui-winit State/Clipboard
interfaces. Native checks are ignored during normal tests to avoid display and
clipboard dependencies. Run each separately:

```sh
# Native Wayland startup: no arboard connection, regardless of DISPLAY.
cargo test --test clipboard_initialization wayland_state_does_not_initialize_arboard -- --ignored --exact --nocapture

# Run against an isolated, working X11 server, not the user's clipboard.
cargo test --test clipboard_initialization native_x11_text_and_image_clipboard_still_work -- --ignored --exact --nocapture

# Failed initialization is deferred, cached, and retains app-local text.
env -u WAYLAND_DISPLAY DISPLAY=invalid cargo test --test clipboard_initialization unavailable_clipboard_initializes_once_and_preserves_local_fallback -- --ignored --exact --nocapture
```
