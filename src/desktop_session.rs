//! Repair expired, desktop-managed X11 credentials before native libraries load.
//!
//! Long-lived launchers can retain a deleted XAUTHORITY after a Wayland desktop
//! restart. NVIDIA's Vulkan ICD still probes X11 during vkCreateInstance. The
//! user manager owns the current session environment; never guess cookie files,
//! weaken server access control, replace working credentials, or touch overrides
//! outside the desktop runtime directory.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
struct DesktopSession {
    display: String,
    wayland_display: String,
    runtime_dir: PathBuf,
    authority: PathBuf,
}

impl DesktopSession {
    fn inherited() -> Option<Self> {
        Some(Self {
            display: std::env::var("DISPLAY").ok()?,
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok()?,
            runtime_dir: std::env::var_os("XDG_RUNTIME_DIR")?.into(),
            authority: std::env::var_os("XAUTHORITY")?.into(),
        })
    }

    fn has_expired_runtime_authority(&self) -> bool {
        !self.display.is_empty()
            && !self.wayland_display.is_empty()
            && is_runtime_file(&self.authority, &self.runtime_dir)
            && matches!(
                std::fs::metadata(&self.authority),
                Err(err) if err.kind() == io::ErrorKind::NotFound
            )
    }
}

fn is_runtime_file(path: &Path, runtime_dir: &Path) -> bool {
    runtime_dir.is_absolute() && path.parent() == Some(runtime_dir)
}

fn parse_manager_environment(json: &[u8]) -> io::Result<DesktopSession> {
    #[derive(serde::Deserialize)]
    struct EnvironmentProperty {
        #[serde(rename = "type")]
        signature: String,
        data: Vec<String>,
    }

    let property: EnvironmentProperty = serde_json::from_slice(json)?;
    if property.signature != "as" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "user manager Environment is not an array of strings",
        ));
    }
    let value = |key: &str| {
        property
            .data
            .iter()
            .filter_map(|entry| entry.split_once('='))
            .find_map(|(name, value)| (name == key).then_some(value.to_owned()))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("session lacks {key}"))
            })
    };
    Ok(DesktopSession {
        display: value("DISPLAY")?,
        wayland_display: value("WAYLAND_DISPLAY")?,
        runtime_dir: value("XDG_RUNTIME_DIR")?.into(),
        authority: value("XAUTHORITY")?.into(),
    })
}

fn replacement_authority(inherited: &DesktopSession, current: &DesktopSession) -> Option<PathBuf> {
    if !inherited.has_expired_runtime_authority()
        || inherited.display != current.display
        || inherited.wayland_display != current.wayland_display
        || inherited.runtime_dir != current.runtime_dir
        || !is_runtime_file(&current.authority, &current.runtime_dir)
    {
        return None;
    }

    // Check readability and file type without reading or logging any cookie data.
    let file = std::fs::File::open(&current.authority).ok()?;
    file.metadata()
        .ok()?
        .is_file()
        .then(|| current.authority.clone())
}

fn current_manager_session() -> io::Result<DesktopSession> {
    let output = Command::new("busctl")
        .args([
            "--user",
            "--timeout=1",
            "--json=short",
            "get-property",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "Environment",
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "user manager query failed ({})",
            output.status
        )));
    }
    parse_manager_environment(&output.stdout)
}

/// Called only for an actual app run, before logging, winit, Vulkan or audio.
/// Re-exec preserves PID, arguments and caller overrides, changing XAUTHORITY
/// only in the new process image. No process-global environment mutation races
/// with native/library threads, and the launcher's environment is untouched.
pub fn refresh_expired_xauthority() {
    use std::os::unix::process::CommandExt;

    let Some(inherited) = DesktopSession::inherited() else {
        return;
    };
    if !inherited.has_expired_runtime_authority() {
        return;
    }
    let current = match current_manager_session() {
        Ok(current) => current,
        Err(err) => {
            eprintln!("[DISPLAY_AUTH] Cannot query current desktop credentials: {err}");
            return;
        }
    };
    let Some(authority) = replacement_authority(&inherited, &current) else {
        return;
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(err) => {
            eprintln!("[DISPLAY_AUTH] Cannot resolve executable for credential refresh: {err}");
            return;
        }
    };
    eprintln!("[DISPLAY_AUTH] Refreshing expired desktop XAUTHORITY from current user session");
    let err = Command::new(executable)
        .args(std::env::args_os().skip(1))
        .env("XAUTHORITY", authority)
        .exec();
    eprintln!("[DISPLAY_AUTH] Cannot restart with current desktop credentials: {err}");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        inherited: DesktopSession,
        current: DesktopSession,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let current = DesktopSession {
                display: ":0".into(),
                wayland_display: "wayland-0".into(),
                runtime_dir: dir.path().to_owned(),
                authority: dir.path().join("current-cookie"),
            };
            std::fs::write(&current.authority, b"test credentials").unwrap();
            let inherited = DesktopSession {
                authority: dir.path().join("expired-cookie"),
                ..current.clone()
            };
            Self {
                _dir: dir,
                inherited,
                current,
            }
        }

        fn replacement(&self) -> Option<PathBuf> {
            replacement_authority(&self.inherited, &self.current)
        }
    }

    #[test]
    fn expired_runtime_cookie_uses_readable_credentials_from_same_session() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.replacement(),
            Some(fixture.current.authority.clone())
        );
    }

    #[test]
    fn existing_authority_is_preserved_even_if_manager_has_another_cookie() {
        let fixture = Fixture::new();
        std::fs::write(&fixture.inherited.authority, b"caller credentials").unwrap();
        assert_eq!(fixture.replacement(), None);
        // The restarted process sees its current readable cookie, so never re-execs again.
        assert_eq!(
            replacement_authority(&fixture.current, &fixture.current),
            None
        );
    }

    #[test]
    fn custom_authority_outside_runtime_directory_is_not_replaced() {
        let mut fixture = Fixture::new();
        fixture.inherited.authority = fixture.inherited.runtime_dir.join("custom/missing");
        assert_eq!(fixture.replacement(), None);
        fixture.inherited.authority = "relative-cookie".into();
        assert_eq!(fixture.replacement(), None);
        fixture.inherited.authority = fixture.inherited.runtime_dir.join("../missing");
        assert_eq!(fixture.replacement(), None);
        fixture.inherited.authority = PathBuf::new();
        assert_eq!(fixture.replacement(), None);
    }

    #[test]
    fn another_display_wayland_connection_or_runtime_cannot_supply_credentials() {
        let fixture = Fixture::new();
        for current in [
            DesktopSession {
                display: ":1".into(),
                ..fixture.current.clone()
            },
            DesktopSession {
                wayland_display: "wayland-1".into(),
                ..fixture.current.clone()
            },
            DesktopSession {
                runtime_dir: fixture.current.runtime_dir.join("other-session"),
                ..fixture.current.clone()
            },
        ] {
            assert_eq!(replacement_authority(&fixture.inherited, &current), None);
        }
    }

    #[test]
    fn missing_directory_or_outside_runtime_current_authority_is_rejected() {
        let mut fixture = Fixture::new();
        std::fs::remove_file(&fixture.current.authority).unwrap();
        assert_eq!(fixture.replacement(), None);
        std::fs::create_dir(&fixture.current.authority).unwrap();
        assert_eq!(fixture.replacement(), None);
        fixture.current.authority = fixture.current.runtime_dir.join("../other-cookie");
        assert_eq!(fixture.replacement(), None);
    }

    #[test]
    fn empty_display_names_and_relative_runtime_do_not_trigger_recovery() {
        let mut fixture = Fixture::new();
        fixture.inherited.display.clear();
        assert_eq!(fixture.replacement(), None);
        fixture.inherited.display = fixture.current.display.clone();
        fixture.inherited.wayland_display.clear();
        assert_eq!(fixture.replacement(), None);
        fixture.inherited.runtime_dir = "runtime".into();
        fixture.inherited.authority = "runtime/missing".into();
        assert!(!fixture.inherited.has_expired_runtime_authority());
    }

    #[test]
    fn manager_json_preserves_values_without_shell_parsing_or_cookie_contents() {
        let json = serde_json::json!({
            "type": "as",
            "data": [
                "DISPLAY=:0",
                "WAYLAND_DISPLAY=wayland-0",
                "XDG_RUNTIME_DIR=/run/user/1000",
                "XAUTHORITY=/run/user/1000/cookie name=with'quotes",
                "UNRELATED_SECRET=not used"
            ]
        });
        let session = parse_manager_environment(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(session.display, ":0");
        assert_eq!(session.wayland_display, "wayland-0");
        assert_eq!(session.runtime_dir, PathBuf::from("/run/user/1000"));
        assert_eq!(
            session.authority,
            PathBuf::from("/run/user/1000/cookie name=with'quotes")
        );
    }

    #[test]
    fn malformed_wrong_type_and_incomplete_manager_properties_are_rejected() {
        for json in [
            b"not json".as_slice(),
            br#"{"type":"s","data":["DISPLAY=:0"]}"#,
            br#"{"type":"as","data":["DISPLAY=:0"]}"#,
            br#"{"type":"as","data":42}"#,
        ] {
            assert!(parse_manager_environment(json).is_err());
        }
    }
}
