//! Native startup regression; keep GPU/session checks out of normal unit tests.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn help_does_not_query_or_repair_the_desktop_environment() {
    let runtime = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_re-flora"))
        .arg("--help")
        .env("DISPLAY", ":invalid")
        .env("WAYLAND_DISPLAY", "invalid-wayland")
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("XAUTHORITY", runtime.path().join("expired"))
        .env("PATH", "/no-session-commands")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("[DISPLAY_AUTH]"));
}

#[test]
#[ignore = "requires a systemd Wayland desktop and GPU; run with cargo test --release"]
fn expired_desktop_authority_is_recovered_before_vulkan_startup() {
    assert!(!cfg!(debug_assertions), "native checks must use Release");
    let runtime_dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let expired = runtime_dir.join(format!(
        ".re-flora-expired-xauthority-test-{}",
        std::process::id()
    ));
    assert!(!expired.exists());

    let output = Command::new(env!("CARGO_BIN_EXE_re-flora"))
        .args(["--hidden", "--mute", "--auto-exit", "0.5"])
        .env("XAUTHORITY", &expired)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}\n{stderr}");
    assert!(output.status.success(), "{combined}");
    assert!(
        combined.contains("[DISPLAY_AUTH] Refreshing expired desktop XAUTHORITY"),
        "startup must recover the expired authority before loading Vulkan: {combined}"
    );
    assert_eq!(
        combined.matches("[DISPLAY_AUTH] Refreshing").count(),
        1,
        "credential refresh must not cause a re-exec loop: {combined}"
    );
    for unexpected in [
        "Authorization required",
        "Failed to initialize arboard clipboard",
        " ERROR ",
        "Validation Error",
        "panicked",
    ] {
        assert!(!combined.contains(unexpected), "{combined}");
    }
    assert!(combined.contains("Application exited successfully"));
}
