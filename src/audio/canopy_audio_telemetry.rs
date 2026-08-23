use crate::audio::{
    CanopyAcousticSampleProvenance, CanopyAudioSourceKey, CanopyTreeLifecycleDiagnostics,
};
use glam::Vec3;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyDirectPathTelemetry {
    pub candidate_membership: bool,
    pub hit: bool,
    pub hit_material: Option<String>,
    pub hit_material_transmission: Option<[f32; 3]>,
    pub visible_fraction: f32,
    pub raw_direct_gain: [f32; 3],
    pub filtered_direct_gain: [f32; 3],
    pub transition_count: u64,
    pub superseded_response_count: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyAudioSampleTelemetry {
    pub key: CanopyAudioSourceKey,
    pub emitter_uuid: Uuid,
    pub position_tree_voxels: Vec3,
    pub position_world: Vec3,
    pub clearance_voxels: f32,
    pub weight: f32,
    pub lifecycle_power: f32,
    pub content_seed: u64,
    pub phase: f32,
    pub provenance: CanopyAcousticSampleProvenance,
    pub target_wind_response: f32,
    pub current_wind_response: f32,
    pub current_volume_db: f32,
    pub direct_path: Option<CanopyDirectPathTelemetry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanopyAudioTreeTelemetry {
    pub tree_id: u32,
    pub lifecycle: CanopyTreeLifecycleDiagnostics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanopyAudioTelemetrySnapshot {
    pub trees: Vec<CanopyAudioTreeTelemetry>,
    pub samples: Vec<CanopyAudioSampleTelemetry>,
    pub petal_superseded_solve_count: u64,
}

#[derive(Default)]
pub struct CanopyAudioTelemetry {
    enabled: bool,
    direct_paths: HashMap<CanopyAudioSourceKey, CanopyDirectPathTelemetry>,
}

impl CanopyAudioTelemetry {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.direct_paths.clear();
        }
    }

    pub fn observe_direct_path(
        &mut self,
        key: CanopyAudioSourceKey,
        observation: CanopyDirectPathTelemetry,
    ) {
        if self.enabled {
            self.direct_paths.insert(key, observation);
        }
    }

    pub fn direct_path(&self, key: CanopyAudioSourceKey) -> Option<&CanopyDirectPathTelemetry> {
        self.enabled.then(|| self.direct_paths.get(&key)).flatten()
    }

    pub fn remove_source(&mut self, key: CanopyAudioSourceKey) {
        self.direct_paths.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{CanopyAcousticSampleId, CanopyAudioSourceKey};

    fn key() -> CanopyAudioSourceKey {
        CanopyAudioSourceKey::new_for_test(3, 7, CanopyAcousticSampleId::new_for_test(11))
    }

    fn direct_path() -> CanopyDirectPathTelemetry {
        CanopyDirectPathTelemetry {
            candidate_membership: true,
            hit: true,
            hit_material: Some("wood".to_owned()),
            hit_material_transmission: Some([0.08, 0.035, 0.015]),
            visible_fraction: 0.75,
            raw_direct_gain: [0.8, 0.7, 0.6],
            filtered_direct_gain: [0.85, 0.75, 0.65],
            transition_count: 4,
            superseded_response_count: 2,
        }
    }

    #[test]
    fn telemetry_is_opt_in_and_discards_observations_when_disabled() {
        let mut telemetry = CanopyAudioTelemetry::default();

        telemetry.observe_direct_path(key(), direct_path());
        assert!(!telemetry.is_enabled());
        assert_eq!(telemetry.direct_path(key()), None);

        telemetry.set_enabled(true);
        telemetry.observe_direct_path(key(), direct_path());
        assert_eq!(telemetry.direct_path(key()), Some(&direct_path()));

        telemetry.set_enabled(false);
        assert_eq!(telemetry.direct_path(key()), None);
    }
}
