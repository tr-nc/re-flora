use crate::audio::{
    CanopyAcousticDescriptor, CanopyAcousticSampleId, CanopyAcousticSampleProvenance,
    CanopyAudioGenerationKey, CanopyAudioSourceKey, CanopyTreeLifecycleDiagnostics,
};
use glam::Vec3;
use std::collections::HashMap;
use uuid::Uuid;

const TELEMETRY_FLOAT_EPSILON: f32 = 1.0e-4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanopyAcousticSolveStatus {
    Solved,
    Retained,
    Deferred,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanopyOcclusionClassification {
    Visible,
    Occluded,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanopySampleAcousticObservation {
    pub sample_id: CanopyAcousticSampleId,
    pub normalized_power_weight: f32,
    pub world_position: Vec3,
    pub hit: bool,
    pub transmission: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyRouteAcousticObservation {
    pub samples: Vec<CanopySampleAcousticObservation>,
    pub ray_count: usize,
    pub cache_hit_count: usize,
    pub hit_count: usize,
    pub visible_fraction: f32,
    pub raw_gain: [f32; 3],
    pub filtered_gain: [f32; 3],
    pub classification: CanopyOcclusionClassification,
    pub dwell_seconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyExtentAcousticObservation {
    pub voice_id: u64,
    pub spatial_revision: u64,
    pub geometry_version: u64,
    pub response_spatial_revision: u64,
    pub response_geometry_version: u64,
    pub extent_sample_count: usize,
    pub direct: CanopyRouteAcousticObservation,
    pub solve_status: CanopyAcousticSolveStatus,
    pub cache_age_seconds: f32,
    pub budget_member: bool,
    pub lobe_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyDirectPathTelemetry {
    pub voice_id: u64,
    pub candidate_membership: bool,
    pub solve_status: CanopyAcousticSolveStatus,
    pub hit: bool,
    pub hit_material: Option<String>,
    pub transmission: [f32; 3],
    pub normalized_power_weight: f32,
    pub observed_world_position: Vec3,
    pub visible_fraction: f32,
    pub raw_direct_gain: [f32; 3],
    pub filtered_direct_gain: [f32; 3],
    pub classification: CanopyOcclusionClassification,
    pub dwell_seconds: f32,
    pub ray_count: usize,
    pub cache_hit_count: usize,
    pub hit_count: usize,
    pub cache_age_seconds: f32,
    pub spatial_revision: u64,
    pub geometry_version: u64,
    pub response_spatial_revision: u64,
    pub response_geometry_version: u64,
    pub lobe_count: usize,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CanopyAudioTelemetryDiagnostics {
    pub extent_response_count: u64,
    pub solve_discard_count: u64,
    pub voice_identity_violation_count: u64,
    pub revision_rollback_count: u64,
    pub sample_contract_violation_count: u64,
    pub aggregate_mismatch_count: u64,
    pub last_discard_spatial_revision: u64,
    pub last_discard_geometry_version: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanopyAudioTelemetrySnapshot {
    pub trees: Vec<CanopyAudioTreeTelemetry>,
    pub samples: Vec<CanopyAudioSampleTelemetry>,
    pub telemetry: CanopyAudioTelemetryDiagnostics,
    pub petal_superseded_solve_count: u64,
    pub petal_telemetry_queue_depth: usize,
    pub petal_telemetry_queue_high_water: usize,
    pub petal_telemetry_dropped_events: u64,
    pub petal_direct_ray_count: u64,
    pub petal_sample_cache_hit_count: u64,
    pub petal_processed_extent_count: u64,
    pub petal_lobe_count: u64,
    pub petal_retained_response_count: u64,
    pub petal_deferred_response_count: u64,
    pub petal_render_rejected_response_count: u64,
}

#[derive(Default)]
struct GenerationObservationState {
    voice_id: Option<u64>,
    spatial_revision: u64,
    geometry_version: u64,
    response_spatial_revision: u64,
    response_geometry_version: u64,
    classification: Option<CanopyOcclusionClassification>,
    transition_count: u64,
}

#[derive(Default)]
pub struct CanopyAudioTelemetry {
    enabled: bool,
    direct_paths: HashMap<CanopyAudioSourceKey, CanopyDirectPathTelemetry>,
    generations: HashMap<CanopyAudioGenerationKey, GenerationObservationState>,
    diagnostics: CanopyAudioTelemetryDiagnostics,
}

impl CanopyAudioTelemetry {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.direct_paths.clear();
            self.generations.clear();
            self.diagnostics = CanopyAudioTelemetryDiagnostics::default();
        }
    }

    #[allow(dead_code)]
    pub fn observe_direct_path(
        &mut self,
        key: CanopyAudioSourceKey,
        observation: CanopyDirectPathTelemetry,
    ) {
        if self.enabled {
            self.direct_paths.insert(key, observation);
        }
    }

    pub fn observe_extent_response(
        &mut self,
        key: CanopyAudioGenerationKey,
        descriptor: &CanopyAcousticDescriptor,
        observation: CanopyExtentAcousticObservation,
    ) {
        if !self.enabled {
            return;
        }
        self.diagnostics.extent_response_count += 1;
        let state = self.generations.entry(key).or_default();
        if state
            .voice_id
            .is_some_and(|voice_id| voice_id != observation.voice_id)
        {
            self.diagnostics.voice_identity_violation_count += 1;
        }
        state.voice_id.get_or_insert(observation.voice_id);
        if observation.spatial_revision < state.spatial_revision
            || observation.geometry_version < state.geometry_version
            || observation.response_spatial_revision < state.response_spatial_revision
            || observation.response_geometry_version < state.response_geometry_version
        {
            self.diagnostics.revision_rollback_count += 1;
        }
        state.spatial_revision = state.spatial_revision.max(observation.spatial_revision);
        state.geometry_version = state.geometry_version.max(observation.geometry_version);
        state.response_spatial_revision = state
            .response_spatial_revision
            .max(observation.response_spatial_revision);
        state.response_geometry_version = state
            .response_geometry_version
            .max(observation.response_geometry_version);
        if state
            .classification
            .is_some_and(|classification| classification != observation.direct.classification)
        {
            state.transition_count += 1;
        }
        state.classification = Some(observation.direct.classification);

        if observation.extent_sample_count != descriptor.samples().len() {
            self.diagnostics.sample_contract_violation_count += 1;
        }
        if observation.direct.samples.is_empty() {
            for (sample_key, direct) in &mut self.direct_paths {
                if sample_key.tree_id() == key.tree_id()
                    && sample_key.generation() == key.generation()
                {
                    direct.solve_status = observation.solve_status;
                    direct.candidate_membership = observation.budget_member;
                    direct.cache_age_seconds = observation.cache_age_seconds;
                    direct.spatial_revision = observation.spatial_revision;
                    direct.geometry_version = observation.geometry_version;
                    direct.response_spatial_revision = observation.response_spatial_revision;
                    direct.response_geometry_version = observation.response_geometry_version;
                    direct.superseded_response_count = self.diagnostics.solve_discard_count;
                }
            }
            return;
        }

        let reconstructed_gain = reconstruct_gain(&observation.direct.samples);
        if reconstructed_gain
            .iter()
            .zip(observation.direct.raw_gain)
            .any(|(actual, expected)| (actual - expected).abs() > TELEMETRY_FLOAT_EPSILON)
        {
            self.diagnostics.aggregate_mismatch_count += 1;
        }
        let observed_hit_count = observation
            .direct
            .samples
            .iter()
            .filter(|sample| sample.hit)
            .count();
        if observed_hit_count != observation.direct.hit_count {
            self.diagnostics.sample_contract_violation_count += 1;
        }

        let mut observed_ids = Vec::with_capacity(observation.direct.samples.len());
        for sample in &observation.direct.samples {
            observed_ids.push(sample.sample_id);
            let Some(expected) = descriptor
                .samples()
                .iter()
                .find(|expected| expected.id() == sample.sample_id)
            else {
                self.diagnostics.sample_contract_violation_count += 1;
                continue;
            };
            let expected_world = descriptor.sample_world_position(expected);
            if (expected.weight() - sample.normalized_power_weight).abs() > TELEMETRY_FLOAT_EPSILON
                || expected_world.distance(sample.world_position) > TELEMETRY_FLOAT_EPSILON
            {
                self.diagnostics.sample_contract_violation_count += 1;
            }
            let source_key =
                CanopyAudioSourceKey::new(key.tree_id(), key.generation(), sample.sample_id);
            self.direct_paths.insert(
                source_key,
                CanopyDirectPathTelemetry {
                    voice_id: observation.voice_id,
                    candidate_membership: observation.budget_member,
                    solve_status: observation.solve_status,
                    hit: sample.hit,
                    hit_material: sample
                        .hit
                        .then(|| {
                            crate::builder::acoustic_material_label_for_transmission(
                                sample.transmission,
                            )
                        })
                        .flatten()
                        .map(str::to_owned),
                    transmission: sample.transmission,
                    normalized_power_weight: sample.normalized_power_weight,
                    observed_world_position: sample.world_position,
                    visible_fraction: observation.direct.visible_fraction,
                    raw_direct_gain: observation.direct.raw_gain,
                    filtered_direct_gain: observation.direct.filtered_gain,
                    classification: observation.direct.classification,
                    dwell_seconds: observation.direct.dwell_seconds,
                    ray_count: observation.direct.ray_count,
                    cache_hit_count: observation.direct.cache_hit_count,
                    hit_count: observation.direct.hit_count,
                    cache_age_seconds: observation.cache_age_seconds,
                    spatial_revision: observation.spatial_revision,
                    geometry_version: observation.geometry_version,
                    response_spatial_revision: observation.response_spatial_revision,
                    response_geometry_version: observation.response_geometry_version,
                    lobe_count: observation.lobe_count,
                    transition_count: state.transition_count,
                    superseded_response_count: self.diagnostics.solve_discard_count,
                },
            );
        }
        if descriptor
            .samples()
            .iter()
            .any(|expected| !observed_ids.contains(&expected.id()))
        {
            self.diagnostics.sample_contract_violation_count += 1;
        }
    }

    pub fn record_solve_discard(&mut self, spatial_revision: u64, geometry_version: u64) {
        if self.enabled {
            self.diagnostics.solve_discard_count += 1;
            self.diagnostics.last_discard_spatial_revision = spatial_revision;
            self.diagnostics.last_discard_geometry_version = geometry_version;
        }
    }

    pub fn direct_path(&self, key: CanopyAudioSourceKey) -> Option<&CanopyDirectPathTelemetry> {
        self.enabled.then(|| self.direct_paths.get(&key)).flatten()
    }

    pub fn remove_source(&mut self, key: CanopyAudioSourceKey) {
        self.direct_paths.remove(&key);
    }

    pub fn diagnostics(&self) -> CanopyAudioTelemetryDiagnostics {
        self.diagnostics
    }
}

fn reconstruct_gain(samples: &[CanopySampleAcousticObservation]) -> [f32; 3] {
    let mut energy = [0.0_f64; 3];
    for sample in samples {
        for (band_energy, transmission) in energy.iter_mut().zip(sample.transmission) {
            *band_energy += f64::from(sample.normalized_power_weight)
                * f64::from(transmission)
                * f64::from(transmission);
        }
    }
    energy.map(|energy| energy.max(0.0).sqrt() as f32)
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
            voice_id: 41,
            candidate_membership: true,
            solve_status: CanopyAcousticSolveStatus::Solved,
            hit: true,
            hit_material: Some("wood".to_owned()),
            transmission: [0.08, 0.035, 0.015],
            normalized_power_weight: 0.125,
            observed_world_position: Vec3::ONE,
            visible_fraction: 0.75,
            raw_direct_gain: [0.8, 0.7, 0.6],
            filtered_direct_gain: [0.85, 0.75, 0.65],
            classification: CanopyOcclusionClassification::Occluded,
            dwell_seconds: 0.3,
            ray_count: 8,
            cache_hit_count: 0,
            hit_count: 2,
            cache_age_seconds: 0.0,
            spatial_revision: 5,
            geometry_version: 2,
            response_spatial_revision: 5,
            response_geometry_version: 2,
            lobe_count: 3,
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

    #[test]
    fn extent_response_maps_stable_samples_and_reconstructs_one_of_eight_energy() {
        let descriptor = eight_sample_descriptor();
        let generation_key = CanopyAudioGenerationKey::new(3, descriptor.generation());
        let samples = descriptor
            .samples()
            .iter()
            .enumerate()
            .map(|(index, sample)| CanopySampleAcousticObservation {
                sample_id: sample.id(),
                normalized_power_weight: sample.weight(),
                world_position: descriptor.sample_world_position(sample),
                hit: index == 0,
                transmission: if index == 0 { [0.1; 3] } else { [1.0; 3] },
            })
            .collect::<Vec<_>>();
        let expected_gain = (7.01_f32 / 8.0).sqrt();
        let observation = CanopyExtentAcousticObservation {
            voice_id: 91,
            spatial_revision: 20,
            geometry_version: 4,
            response_spatial_revision: 20,
            response_geometry_version: 4,
            extent_sample_count: 8,
            direct: CanopyRouteAcousticObservation {
                samples,
                ray_count: 8,
                cache_hit_count: 0,
                hit_count: 1,
                visible_fraction: 0.875,
                raw_gain: [expected_gain; 3],
                filtered_gain: [expected_gain; 3],
                classification: CanopyOcclusionClassification::Visible,
                dwell_seconds: 0.4,
            },
            solve_status: CanopyAcousticSolveStatus::Solved,
            cache_age_seconds: 0.0,
            budget_member: true,
            lobe_count: 3,
        };
        let mut telemetry = CanopyAudioTelemetry::default();
        telemetry.set_enabled(true);

        telemetry.observe_extent_response(generation_key, &descriptor, observation);

        let first = telemetry
            .direct_path(CanopyAudioSourceKey::new(
                3,
                descriptor.generation(),
                descriptor.samples()[0].id(),
            ))
            .unwrap();
        assert_eq!(first.voice_id, 91);
        assert!(first.hit);
        assert_eq!(first.transmission, [0.1; 3]);
        assert!((20.0 * first.raw_direct_gain[0].log10() - (-0.574)).abs() < 0.002);
        assert_eq!(telemetry.diagnostics().voice_identity_violation_count, 0);
        assert_eq!(telemetry.diagnostics().revision_rollback_count, 0);
        assert_eq!(telemetry.diagnostics().aggregate_mismatch_count, 0);

        let half_samples = descriptor
            .samples()
            .iter()
            .enumerate()
            .map(|(index, sample)| CanopySampleAcousticObservation {
                sample_id: sample.id(),
                normalized_power_weight: sample.weight(),
                world_position: descriptor.sample_world_position(sample),
                hit: index < 4,
                transmission: if index < 4 { [0.1; 3] } else { [1.0; 3] },
            })
            .collect::<Vec<_>>();
        let half_gain = 0.505_f32.sqrt();
        telemetry.observe_extent_response(
            generation_key,
            &descriptor,
            CanopyExtentAcousticObservation {
                voice_id: 91,
                spatial_revision: 21,
                geometry_version: 4,
                response_spatial_revision: 21,
                response_geometry_version: 4,
                extent_sample_count: 8,
                direct: CanopyRouteAcousticObservation {
                    samples: half_samples,
                    ray_count: 8,
                    cache_hit_count: 0,
                    hit_count: 4,
                    visible_fraction: 0.5,
                    raw_gain: [half_gain; 3],
                    filtered_gain: [half_gain; 3],
                    classification: CanopyOcclusionClassification::Visible,
                    dwell_seconds: 0.5,
                },
                solve_status: CanopyAcousticSolveStatus::Solved,
                cache_age_seconds: 0.0,
                budget_member: true,
                lobe_count: 3,
            },
        );
        let half = telemetry
            .direct_path(CanopyAudioSourceKey::new(
                3,
                descriptor.generation(),
                descriptor.samples()[0].id(),
            ))
            .unwrap();
        assert!((20.0 * half.raw_direct_gain[0].log10() - (-2.967)).abs() < 0.002);
        assert_eq!(telemetry.diagnostics().voice_identity_violation_count, 0);
        assert_eq!(telemetry.diagnostics().revision_rollback_count, 0);
        assert_eq!(telemetry.diagnostics().aggregate_mismatch_count, 0);
    }

    #[test]
    fn deferred_response_keeps_last_good_samples_and_revision_rollback_is_observable() {
        let descriptor = eight_sample_descriptor();
        let generation_key = CanopyAudioGenerationKey::new(3, descriptor.generation());
        let visible_samples = descriptor
            .samples()
            .iter()
            .map(|sample| CanopySampleAcousticObservation {
                sample_id: sample.id(),
                normalized_power_weight: sample.weight(),
                world_position: descriptor.sample_world_position(sample),
                hit: false,
                transmission: [1.0; 3],
            })
            .collect::<Vec<_>>();
        let solved_route = CanopyRouteAcousticObservation {
            samples: visible_samples,
            ray_count: 8,
            cache_hit_count: 0,
            hit_count: 0,
            visible_fraction: 1.0,
            raw_gain: [1.0; 3],
            filtered_gain: [1.0; 3],
            classification: CanopyOcclusionClassification::Visible,
            dwell_seconds: 0.2,
        };
        let mut telemetry = CanopyAudioTelemetry::default();
        telemetry.set_enabled(true);
        telemetry.observe_extent_response(
            generation_key,
            &descriptor,
            CanopyExtentAcousticObservation {
                voice_id: 7,
                spatial_revision: 10,
                geometry_version: 3,
                response_spatial_revision: 10,
                response_geometry_version: 3,
                extent_sample_count: 8,
                direct: solved_route.clone(),
                solve_status: CanopyAcousticSolveStatus::Solved,
                cache_age_seconds: 0.0,
                budget_member: true,
                lobe_count: 3,
            },
        );
        telemetry.observe_extent_response(
            generation_key,
            &descriptor,
            CanopyExtentAcousticObservation {
                voice_id: 7,
                spatial_revision: 11,
                geometry_version: 3,
                response_spatial_revision: 10,
                response_geometry_version: 3,
                extent_sample_count: 8,
                direct: CanopyRouteAcousticObservation {
                    samples: Vec::new(),
                    ray_count: 0,
                    cache_hit_count: 0,
                    hit_count: 0,
                    visible_fraction: 1.0,
                    raw_gain: [1.0; 3],
                    filtered_gain: [1.0; 3],
                    classification: CanopyOcclusionClassification::Visible,
                    dwell_seconds: 0.2,
                },
                solve_status: CanopyAcousticSolveStatus::Deferred,
                cache_age_seconds: 0.3,
                budget_member: false,
                lobe_count: 0,
            },
        );

        let sample_key =
            CanopyAudioSourceKey::new(3, descriptor.generation(), descriptor.samples()[0].id());
        let deferred = telemetry.direct_path(sample_key).unwrap();
        assert_eq!(deferred.solve_status, CanopyAcousticSolveStatus::Deferred);
        assert_eq!(deferred.transmission, [1.0; 3]);
        assert_eq!(deferred.raw_direct_gain, [1.0; 3]);
        assert!(!deferred.candidate_membership);
        assert_eq!(deferred.response_spatial_revision, 10);
        assert_eq!(telemetry.diagnostics().revision_rollback_count, 0);

        telemetry.observe_extent_response(
            generation_key,
            &descriptor,
            CanopyExtentAcousticObservation {
                voice_id: 7,
                spatial_revision: 9,
                geometry_version: 3,
                response_spatial_revision: 9,
                response_geometry_version: 3,
                extent_sample_count: 8,
                direct: solved_route,
                solve_status: CanopyAcousticSolveStatus::Solved,
                cache_age_seconds: 0.0,
                budget_member: true,
                lobe_count: 3,
            },
        );
        assert_eq!(telemetry.diagnostics().revision_rollback_count, 1);
    }

    fn eight_sample_descriptor() -> crate::audio::CanopyAcousticDescriptor {
        let leaves = [-4.0, 4.0]
            .into_iter()
            .flat_map(|x| {
                [-4.0, 4.0].into_iter().flat_map(move |y| {
                    [-4.0, 4.0]
                        .into_iter()
                        .map(move |z| crate::tree_gen::LeafPlacement {
                            position: Vec3::new(x, y, z),
                            anchor: Vec3::ZERO,
                        })
                })
            })
            .collect::<Vec<_>>();
        crate::audio::CanopyAcousticDescriptor::build(7, Vec3::ONE, 123, &leaves, &[])
    }
}
