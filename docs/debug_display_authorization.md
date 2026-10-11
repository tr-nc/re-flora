# X11 authorization during native Wayland startup

## Confirmed cause

The console message `Authorization required, but no authorization protocol specified`
was not emitted by egui or a rendering validation error. A debugger breakpoint at
`XOpenDisplay` identified NVIDIA's Vulkan ICD (`libGLX_nvidia.so.0`) calling it from
`vk_icdNegotiateLoaderICDInterfaceVersion` during `vkCreateInstance`. Standalone
`vulkaninfo --summary` also reproduced the diagnostic.

The long-lived launcher inherited an `XAUTHORITY` path for a previous GNOME
Wayland session. That file no longer existed; the current user manager and
XWayland used another readable credential file. Supplying the current file to
an otherwise identical Release app run removed the diagnostic.

## Startup contract

`src/desktop_session.rs` owns a Linux-only recovery at the actual app-run
boundary, before logging, winit, Vulkan and audio start:

- Only a missing `XAUTHORITY` file directly inside the caller's absolute
  `XDG_RUNTIME_DIR`, with nonempty `DISPLAY` and `WAYLAND_DISPLAY`, is eligible.
- The authoritative source is `org.freedesktop.systemd1.Manager.Environment`
  on the current user's bus, queried with `busctl --user --timeout=1 --json=short`.
  There is no shell evaluation, process scanning, filename guessing or cookie logging.
- `DISPLAY`, `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` must all match the current
  manager's session. Its replacement must be a readable regular file directly
  inside that same runtime directory.
- Existing credentials, non-runtime custom paths, empty/absent settings,
  permission errors and other sessions are left alone. If the user manager is
  unavailable or mismatched, no guessed replacement is installed.
- Recovery re-execs the same executable with unchanged arguments and PID,
  overriding only its own `XAUTHORITY`. It avoids concurrent process-global
  environment mutation. The refreshed process sees an existing file and does
  not restart again. Parent-launcher/global desktop environment is not modified.
- Help/log inspection and other non-running CLI commands do not query the desktop.

An eligible recovery reports `[DISPLAY_AUTH] Refreshing expired desktop XAUTHORITY`
before replacing the process image. Genuine authorization failures remain visible;
no stderr filters or `xhost` access-control changes are used. This is deliberately
not a universal repair of arbitrary wrong-but-existing credentials or non-systemd
sessions, and other programs launched with the stale parent environment remain
outside Re: Flora's startup contract.

## Evidence and regression checks

`tests/display_authorization.rs` reproduced the exact authorization diagnostic
against the pre-fix Release executable using a missing runtime credential path.
The same test now passes through the real startup, driver initialization and game
exit, and asserts a single recovery (no restart loop), no authorization/clipboard
warning, no rendering error and successful exit.

```sh
cargo fmt --check
cargo check
cargo test desktop_session::tests
cargo test --test display_authorization
cargo build --release
cargo test --release --test display_authorization expired_desktop_authority_is_recovered_before_vulkan_startup -- --ignored --exact --nocapture
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

The native regression is opt-in so normal unit tests remain display/GPU independent.
Pure filesystem/session-policy tests cover valid/custom credentials, session
mismatches, missing/unusable replacements, empty/relative paths and JSON values
without shell parsing. The native-free CLI test ensures `--help` needs no session
query even when inherited credentials are expired.

Final checks also passed the original inherited-environment command, valid
authority startup with `busctl` absent from PATH, and native X11 startup with
Wayland variables unset. An intentionally missing external custom authority
was not overridden and still emitted the original diagnostic, demonstrating
that genuine authorization errors are not filtered. Regular tests passed
1,586 checks with the one previously timing-out climbing-plants long test
excluded; the native Release regression passed separately. No generated shader
or GUI configuration files changed.
