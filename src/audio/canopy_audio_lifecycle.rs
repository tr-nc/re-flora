use crate::audio::{CanopyAcousticDescriptor, CanopyAcousticSample, CanopyAcousticSampleId};
use glam::Vec3;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanopyAudioSourceKey {
    tree_id: u32,
    generation: u64,
    sample_id: CanopyAcousticSampleId,
}

impl CanopyAudioSourceKey {
    pub fn tree_id(self) -> u32 {
        self.tree_id
    }

    pub fn generation(self) -> u64 {
        self.generation
    }

    pub fn sample_id(self) -> CanopyAcousticSampleId {
        self.sample_id
    }
}

#[derive(Clone, Debug)]
pub struct ActiveCanopyAcousticSample {
    key: CanopyAudioSourceKey,
    tree_origin_world: Vec3,
    sample: CanopyAcousticSample,
    lifecycle_power: f32,
}

impl ActiveCanopyAcousticSample {
    pub fn key(&self) -> CanopyAudioSourceKey {
        self.key
    }

    pub fn tree_id(&self) -> u32 {
        self.key.tree_id
    }

    pub fn generation(&self) -> u64 {
        self.key.generation
    }

    pub fn sample(&self) -> &CanopyAcousticSample {
        &self.sample
    }

    pub fn world_position(&self) -> Vec3 {
        self.tree_origin_world + self.sample.position_tree_voxels() / 256.0
    }

    pub fn lifecycle_power(&self) -> f32 {
        self.lifecycle_power
    }

    pub fn effective_power(&self) -> f32 {
        self.sample.weight() * self.lifecycle_power
    }
}

#[derive(Clone, Debug, Default)]
pub struct CanopyAudioLifecycleSnapshot {
    samples: Vec<ActiveCanopyAcousticSample>,
}

impl CanopyAudioLifecycleSnapshot {
    pub fn samples(&self) -> &[ActiveCanopyAcousticSample] {
        &self.samples
    }

    pub fn total_power(&self) -> f32 {
        self.samples
            .iter()
            .map(ActiveCanopyAcousticSample::effective_power)
            .sum()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CanopyAudioLifecycleError {
    #[error("canopy audio time must be finite, got {0}")]
    NonFiniteTime(String),
    #[error(
        "tree {tree_id} canopy generation must increase: current {current}, incoming {incoming}"
    )]
    NonMonotonicGeneration {
        tree_id: u32,
        current: u64,
        incoming: u64,
    },
}

#[derive(Clone, Copy, Debug)]
struct PowerEnvelope {
    start_time_seconds: f32,
    duration_seconds: f32,
    start_power: f32,
    target_power: f32,
}

impl PowerEnvelope {
    fn new(
        start_time_seconds: f32,
        duration_seconds: f32,
        start_power: f32,
        target_power: f32,
    ) -> Self {
        Self {
            start_time_seconds,
            duration_seconds,
            start_power: start_power.clamp(0.0, 1.0),
            target_power: target_power.clamp(0.0, 1.0),
        }
    }

    fn power_at(self, time_seconds: f32) -> f32 {
        if self.duration_seconds <= f32::EPSILON {
            return self.target_power;
        }
        let progress =
            ((time_seconds - self.start_time_seconds) / self.duration_seconds).clamp(0.0, 1.0);
        self.start_power + (self.target_power - self.start_power) * progress
    }

    fn is_finished_at(self, time_seconds: f32) -> bool {
        time_seconds >= self.start_time_seconds + self.duration_seconds
    }
}

#[derive(Clone, Debug)]
struct CanopyGenerationLayer {
    descriptor: CanopyAcousticDescriptor,
    power: PowerEnvelope,
}

#[derive(Debug, Default)]
struct CanopyTreeLifecycle {
    layers: Vec<CanopyGenerationLayer>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CanopyTreeLifecycleDiagnostics {
    pub published_generation_count: u64,
    pub replacement_transition_count: u64,
    pub removal_transition_count: u64,
    pub superseded_transition_count: u64,
    pub retired_generation_count: u64,
}

pub struct CanopyAudioLifecycle {
    crossfade_seconds: f32,
    trees: HashMap<u32, CanopyTreeLifecycle>,
    latest_generation_by_tree: HashMap<u32, u64>,
    diagnostics_by_tree: HashMap<u32, CanopyTreeLifecycleDiagnostics>,
}

impl CanopyAudioLifecycle {
    pub fn new(crossfade_seconds: f32) -> Self {
        Self {
            crossfade_seconds: if crossfade_seconds.is_finite() {
                crossfade_seconds.max(0.0)
            } else {
                0.0
            },
            trees: HashMap::new(),
            latest_generation_by_tree: HashMap::new(),
            diagnostics_by_tree: HashMap::new(),
        }
    }

    pub fn replace(
        &mut self,
        tree_id: u32,
        descriptor: CanopyAcousticDescriptor,
        time_seconds: f32,
    ) -> Result<(), CanopyAudioLifecycleError> {
        validate_time(time_seconds)?;
        self.prune(time_seconds);
        if let Some(&current) = self.latest_generation_by_tree.get(&tree_id) {
            if descriptor.generation() <= current {
                return Err(CanopyAudioLifecycleError::NonMonotonicGeneration {
                    tree_id,
                    current,
                    incoming: descriptor.generation(),
                });
            }
        }
        self.latest_generation_by_tree
            .insert(tree_id, descriptor.generation());
        let replacing_existing = self
            .trees
            .get(&tree_id)
            .is_some_and(|tree| !tree.layers.is_empty());
        let superseding_transition = self.trees.get(&tree_id).is_some_and(|tree| {
            tree.layers.iter().any(|layer| {
                !layer.power.is_finished_at(time_seconds)
                    && (layer.power.start_power - layer.power.target_power).abs() > f32::EPSILON
            })
        });
        let diagnostics = self.diagnostics_by_tree.entry(tree_id).or_default();
        diagnostics.published_generation_count += 1;
        diagnostics.replacement_transition_count += u64::from(replacing_existing);
        diagnostics.superseded_transition_count += u64::from(superseding_transition);
        let tree = self.trees.entry(tree_id).or_default();

        for layer in &mut tree.layers {
            let current_power = layer.power.power_at(time_seconds);
            layer.power =
                PowerEnvelope::new(time_seconds, self.crossfade_seconds, current_power, 0.0);
        }
        if !descriptor.samples().is_empty() {
            tree.layers.push(CanopyGenerationLayer {
                descriptor,
                power: PowerEnvelope::new(time_seconds, self.crossfade_seconds, 0.0, 1.0),
            });
        }
        Ok(())
    }

    pub fn remove(
        &mut self,
        tree_id: u32,
        time_seconds: f32,
    ) -> Result<(), CanopyAudioLifecycleError> {
        validate_time(time_seconds)?;
        self.prune(time_seconds);
        let Some(tree) = self.trees.get_mut(&tree_id) else {
            return Ok(());
        };
        if !tree.layers.is_empty() {
            self.diagnostics_by_tree
                .entry(tree_id)
                .or_default()
                .removal_transition_count += 1;
        }
        for layer in &mut tree.layers {
            let current_power = layer.power.power_at(time_seconds);
            layer.power =
                PowerEnvelope::new(time_seconds, self.crossfade_seconds, current_power, 0.0);
        }
        Ok(())
    }

    pub fn snapshot(
        &mut self,
        time_seconds: f32,
    ) -> Result<CanopyAudioLifecycleSnapshot, CanopyAudioLifecycleError> {
        validate_time(time_seconds)?;
        self.prune(time_seconds);
        let mut samples = Vec::new();
        for (&tree_id, tree) in &self.trees {
            for layer in &tree.layers {
                let lifecycle_power = layer.power.power_at(time_seconds);
                for sample in layer.descriptor.samples() {
                    samples.push(ActiveCanopyAcousticSample {
                        key: CanopyAudioSourceKey {
                            tree_id,
                            generation: layer.descriptor.generation(),
                            sample_id: sample.id(),
                        },
                        tree_origin_world: layer.descriptor.tree_origin_world(),
                        sample: sample.clone(),
                        lifecycle_power,
                    });
                }
            }
        }
        samples.sort_by_key(ActiveCanopyAcousticSample::key);
        Ok(CanopyAudioLifecycleSnapshot { samples })
    }

    pub fn registered_tree_count(&self) -> usize {
        self.trees.len()
    }

    pub fn tree_diagnostics(&self, tree_id: u32) -> Option<CanopyTreeLifecycleDiagnostics> {
        self.diagnostics_by_tree.get(&tree_id).copied()
    }

    fn prune(&mut self, time_seconds: f32) {
        for (&tree_id, tree) in &mut self.trees {
            let previous_layer_count = tree.layers.len();
            tree.layers.retain(|layer| {
                !(layer.power.target_power <= f32::EPSILON
                    && layer.power.is_finished_at(time_seconds))
            });
            let retired_count = previous_layer_count - tree.layers.len();
            if retired_count > 0 {
                self.diagnostics_by_tree
                    .entry(tree_id)
                    .or_default()
                    .retired_generation_count += retired_count as u64;
            }
        }
        self.trees.retain(|_, tree| !tree.layers.is_empty());
    }
}

fn validate_time(time_seconds: f32) -> Result<(), CanopyAudioLifecycleError> {
    if time_seconds.is_finite() {
        Ok(())
    } else {
        Err(CanopyAudioLifecycleError::NonFiniteTime(
            time_seconds.to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::CanopyAcousticDescriptor, tree_gen::LeafPlacement};
    use glam::Vec3;

    fn descriptor(generation: u64, x: f32) -> CanopyAcousticDescriptor {
        CanopyAcousticDescriptor::build(
            generation,
            Vec3::new(1.0, 0.0, 1.0),
            77,
            &[LeafPlacement {
                position: Vec3::new(x, 4.0, 0.0),
                anchor: Vec3::ZERO,
            }],
            &[],
        )
    }

    fn assert_power(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1.0e-6,
            "expected power {expected}, got {actual}"
        );
    }

    #[test]
    fn replacement_crossfades_generations_and_removal_returns_registry_to_baseline() {
        let mut lifecycle = CanopyAudioLifecycle::new(1.0);

        lifecycle.replace(7, descriptor(1, -4.0), 0.0).unwrap();
        assert_power(lifecycle.snapshot(0.0).unwrap().total_power(), 0.0);
        assert_power(lifecycle.snapshot(1.0).unwrap().total_power(), 1.0);

        lifecycle.replace(7, descriptor(2, 4.0), 1.0).unwrap();
        let transition_start = lifecycle.snapshot(1.0).unwrap();
        assert_eq!(transition_start.samples().len(), 2);
        assert_power(transition_start.total_power(), 1.0);

        let midpoint = lifecycle.snapshot(1.5).unwrap();
        assert_eq!(
            midpoint
                .samples()
                .iter()
                .map(ActiveCanopyAcousticSample::generation)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(midpoint
            .samples()
            .iter()
            .all(|sample| (sample.lifecycle_power() - 0.5).abs() < 1.0e-6));
        assert_power(midpoint.total_power(), 1.0);

        let completed = lifecycle.snapshot(2.0).unwrap();
        assert_eq!(completed.samples().len(), 1);
        assert_eq!(completed.samples()[0].generation(), 2);
        assert_power(completed.total_power(), 1.0);

        lifecycle.remove(7, 2.0).unwrap();
        assert_power(lifecycle.snapshot(2.5).unwrap().total_power(), 0.5);
        assert!(lifecycle.snapshot(3.0).unwrap().samples().is_empty());
        assert_eq!(lifecycle.registered_tree_count(), 0);
        assert_eq!(
            lifecycle.replace(7, descriptor(2, 4.0), 3.0),
            Err(CanopyAudioLifecycleError::NonMonotonicGeneration {
                tree_id: 7,
                current: 2,
                incoming: 2,
            })
        );
    }

    #[test]
    fn rapid_replacement_stays_power_bounded_and_counts_superseded_transition() {
        let mut lifecycle = CanopyAudioLifecycle::new(1.0);
        lifecycle.replace(9, descriptor(1, -4.0), 0.0).unwrap();
        lifecycle.snapshot(1.0).unwrap();
        lifecycle.replace(9, descriptor(2, 0.0), 1.0).unwrap();
        lifecycle.snapshot(1.25).unwrap();
        lifecycle.replace(9, descriptor(3, 4.0), 1.25).unwrap();

        for time in [1.25, 1.5, 2.25] {
            assert!(lifecycle.snapshot(time).unwrap().total_power() <= 1.0 + 1.0e-6);
        }
        assert_eq!(
            lifecycle.tree_diagnostics(9),
            Some(CanopyTreeLifecycleDiagnostics {
                published_generation_count: 3,
                replacement_transition_count: 2,
                removal_transition_count: 0,
                superseded_transition_count: 1,
                retired_generation_count: 2,
            })
        );
    }
}
