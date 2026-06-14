use crate::gameplay::CameraPose;
use anyhow::{Context, Result};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_CAMERA_SNAPSHOT_PATH: &str = "config/camera_snapshots.toml";
pub(crate) const PLAYER_DEFAULT_SNAPSHOT_NAME: &str = "player-default";

const FALLBACK_SNAPSHOT_NAME: &str = "snapshot";
const RESERVED_SNAPSHOT_NAMES: &[&str] = &[PLAYER_DEFAULT_SNAPSHOT_NAME, "default"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CameraSnapshot {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub position: [f32; 3],
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    #[serde(default = "default_fov_deg")]
    pub fov_deg: f32,
    #[serde(default = "default_fly_mode")]
    pub fly_mode: bool,
}

impl CameraSnapshot {
    pub fn from_pose(name: String, description: String, pose: CameraPose, fly_mode: bool) -> Self {
        Self {
            name,
            description,
            position: pose.position.to_array(),
            yaw_deg: pose.yaw_deg,
            pitch_deg: pose.pitch_deg,
            fov_deg: pose.fov_deg,
            fly_mode,
        }
    }

    pub fn pose(&self) -> CameraPose {
        CameraPose {
            position: Vec3::from_array(self.position),
            yaw_deg: self.yaw_deg,
            pitch_deg: self.pitch_deg,
            fov_deg: self.fov_deg,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CameraSnapshotFile {
    #[serde(default)]
    snapshots: Vec<CameraSnapshot>,
}

#[derive(Clone, Debug)]
pub(crate) struct CameraSnapshotLibrary {
    path: PathBuf,
    snapshots: Vec<CameraSnapshot>,
}

impl CameraSnapshotLibrary {
    pub fn load_default() -> Result<Self> {
        Self::load(DEFAULT_CAMERA_SNAPSHOT_PATH)
    }

    pub fn empty_default() -> Self {
        Self {
            path: PathBuf::from(DEFAULT_CAMERA_SNAPSHOT_PATH),
            snapshots: Vec::new(),
        }
    }

    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                snapshots: Vec::new(),
            });
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read camera snapshots from {}", path.display()))?;
        if contents.trim().is_empty() {
            return Ok(Self {
                path,
                snapshots: Vec::new(),
            });
        }

        let parsed: CameraSnapshotFile = toml::from_str(&contents)
            .with_context(|| format!("failed to parse camera snapshots from {}", path.display()))?;

        Ok(Self {
            path,
            snapshots: normalize_snapshot_list(parsed.snapshots),
        })
    }

    pub fn save(&self) -> Result<()> {
        write_snapshot_file(&self.path, &self.snapshots)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn snapshots(&self) -> &[CameraSnapshot] {
        &self.snapshots
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn names_for_cli(&self) -> Vec<String> {
        std::iter::once(PLAYER_DEFAULT_SNAPSHOT_NAME.to_owned())
            .chain(self.snapshots.iter().map(|snapshot| snapshot.name.clone()))
            .collect()
    }

    pub fn find(&self, requested_name: &str) -> Option<&CameraSnapshot> {
        let normalized_name = normalize_snapshot_name(requested_name);
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.name == requested_name)
            .or_else(|| {
                self.snapshots
                    .iter()
                    .find(|snapshot| snapshot.name == normalized_name)
            })
    }

    pub fn is_cli_name_available(&self, requested_name: &str) -> bool {
        is_player_default_snapshot_name(requested_name) || self.find(requested_name).is_some()
    }

    pub fn add_from_pose(
        &mut self,
        requested_name: &str,
        description: String,
        pose: CameraPose,
        fly_mode: bool,
    ) -> Result<String> {
        let name = self.unique_name(requested_name);
        self.snapshots.push(CameraSnapshot::from_pose(
            name.clone(),
            description,
            pose,
            fly_mode,
        ));
        if let Err(err) = self.save() {
            self.snapshots.pop();
            return Err(err);
        }
        Ok(name)
    }

    pub fn remove(&mut self, name: &str) -> Result<bool> {
        let Some(index) = self
            .snapshots
            .iter()
            .position(|snapshot| snapshot.name == name)
        else {
            return Ok(false);
        };

        let removed = self.snapshots.remove(index);
        if let Err(err) = self.save() {
            self.snapshots.insert(index, removed);
            return Err(err);
        }
        Ok(true)
    }

    pub fn unique_name(&self, requested_name: &str) -> String {
        unique_snapshot_name(
            requested_name,
            self.snapshots.iter().map(|snapshot| snapshot.name.as_str()),
        )
    }
}

pub(crate) fn is_player_default_snapshot_name(name: &str) -> bool {
    name == PLAYER_DEFAULT_SNAPSHOT_NAME || name == "default"
}

pub(crate) fn normalize_snapshot_name(input: &str) -> String {
    let mut normalized = String::new();
    let mut pending_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(ch.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = !normalized.is_empty();
        }
    }

    let normalized = if normalized.is_empty() {
        FALLBACK_SNAPSHOT_NAME.to_owned()
    } else {
        normalized
    };

    if RESERVED_SNAPSHOT_NAMES.contains(&normalized.as_str()) {
        format!("{normalized}-snapshot")
    } else {
        normalized
    }
}

pub(crate) fn unique_snapshot_name<'a>(
    requested_name: &str,
    existing_names: impl IntoIterator<Item = &'a str>,
) -> String {
    let base_name = normalize_snapshot_name(requested_name);
    let existing: HashSet<&str> = existing_names.into_iter().collect();

    if !existing.contains(base_name.as_str()) {
        return base_name;
    }

    for suffix in 2.. {
        let candidate = format!("{base_name}-{suffix}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should always return")
}

fn normalize_snapshot_list(snapshots: Vec<CameraSnapshot>) -> Vec<CameraSnapshot> {
    let mut normalized_snapshots = Vec::with_capacity(snapshots.len());
    for mut snapshot in snapshots {
        snapshot.name = unique_snapshot_name(
            &snapshot.name,
            normalized_snapshots
                .iter()
                .map(|existing: &CameraSnapshot| existing.name.as_str()),
        );
        normalized_snapshots.push(snapshot);
    }
    normalized_snapshots
}

fn write_snapshot_file(path: &Path, snapshots: &[CameraSnapshot]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create camera snapshot directory {}",
                    parent.display()
                )
            })?;
        }
    }

    let file = CameraSnapshotFile {
        snapshots: snapshots.to_vec(),
    };
    let contents = toml::to_string_pretty(&file).context("failed to serialize camera snapshots")?;
    fs::write(path, contents)
        .with_context(|| format!("failed to write camera snapshots to {}", path.display()))
}

fn default_fov_deg() -> f32 {
    60.0
}

fn default_fly_mode() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_snapshot_names_to_kebab_case() {
        assert_eq!(normalize_snapshot_name("Tree Closeup!!"), "tree-closeup");
        assert_eq!(
            normalize_snapshot_name("  shadow__detail  "),
            "shadow-detail"
        );
        assert_eq!(normalize_snapshot_name(""), "snapshot");
        assert_eq!(
            normalize_snapshot_name("player-default"),
            "player-default-snapshot"
        );
    }

    #[test]
    fn unique_names_get_deterministic_suffixes() {
        let existing = ["tree-closeup", "tree-closeup-2"];
        assert_eq!(
            unique_snapshot_name("Tree Closeup", existing.iter().copied()),
            "tree-closeup-3"
        );
    }

    #[test]
    fn snapshot_toml_round_trip_preserves_pose_fields() {
        let snapshot = CameraSnapshot::from_pose(
            "overview".to_owned(),
            "island overview".to_owned(),
            CameraPose {
                position: Vec3::new(0.5, 0.8, 0.5),
                yaw_deg: 135.0,
                pitch_deg: -5.0,
                fov_deg: 60.0,
            },
            true,
        );
        let file = CameraSnapshotFile {
            snapshots: vec![snapshot.clone()],
        };
        let serialized = toml::to_string_pretty(&file).unwrap();
        let parsed: CameraSnapshotFile = toml::from_str(&serialized).unwrap();
        let parsed_snapshot = &parsed.snapshots[0];

        assert_eq!(parsed_snapshot.name, snapshot.name);
        assert_eq!(parsed_snapshot.description, snapshot.description);
        assert_eq!(parsed_snapshot.position, snapshot.position);
        assert_eq!(parsed_snapshot.yaw_deg, snapshot.yaw_deg);
        assert_eq!(parsed_snapshot.pitch_deg, snapshot.pitch_deg);
        assert_eq!(parsed_snapshot.fov_deg, snapshot.fov_deg);
        assert_eq!(parsed_snapshot.fly_mode, snapshot.fly_mode);
    }

    #[test]
    fn missing_snapshot_file_loads_empty_library() {
        let path = std::env::temp_dir().join(format!(
            "verdarium-missing-camera-snapshots-{}.toml",
            uuid::Uuid::new_v4()
        ));
        let library = CameraSnapshotLibrary::load(&path).unwrap();
        assert!(library.is_empty());
    }
}
