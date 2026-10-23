//! Asset selection and phase ownership for the optional recorded canopy texture.
use super::{CanopyAudioGenerationKey, SpatialSoundManager};
use anyhow::Result;
use petalsonic::{LoopBlendControl, ResidentClip};
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use std::hash::{DefaultHasher, Hash, Hasher};

// Derivatives are RMS-matched to -30 dBFS. Replace the procedural +36 dB makeup
// with +12 dB, yielding a -18 dBFS full-wind reference before existing tree/mixer gains.
pub(super) const GAIN_ADJUST_DB: f32 =
    12.0 - super::tree_audio_manager::PROCEDURAL_RUSTLE_MAKEUP_GAIN_DB;

pub(super) struct RecordedLeafBank {
    breeze: Vec<ResidentClip>,
    strong: Vec<ResidentClip>,
    seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Selection {
    breeze: usize,
    strong: usize,
    breeze_phase: f32,
    strong_phase: f32,
}

pub(super) struct PreparedLeafLoop {
    pub clip: ResidentClip,
    pub control: LoopBlendControl,
    pub phase: f32,
}

impl RecordedLeafBank {
    pub fn load(manager: &SpatialSoundManager) -> Result<Self> {
        Ok(Self {
            breeze: super::runtime_assets::LEAF_BREEZE_CLIPS
                .iter()
                .map(|p| manager.cached_clip(p))
                .collect::<Result<_>>()?,
            strong: super::runtime_assets::LEAF_STRONG_CLIPS
                .iter()
                .map(|p| manager.cached_clip(p))
                .collect::<Result<_>>()?,
            seed: rand::random(),
        })
    }

    fn selection(&self, key: CanopyAudioGenerationKey) -> Selection {
        let mut hash = DefaultHasher::new();
        (self.seed, key).hash(&mut hash);
        let mut rng = SmallRng::seed_from_u64(hash.finish());
        Selection {
            breeze: rng.random_range(0..self.breeze.len()),
            strong: rng.random_range(0..self.strong.len()),
            breeze_phase: rng.random(),
            strong_phase: rng.random(),
        }
    }

    pub fn prepare(
        &self,
        key: CanopyAudioGenerationKey,
        time_seconds: f32,
        response: f32,
    ) -> Result<PreparedLeafLoop> {
        let selected = self.selection(key);
        let control = LoopBlendControl::default();
        control.set(response);
        let clip = self.breeze[selected.breeze].clone().with_loop_blend(
            &self.strong[selected.strong],
            selected.strong_phase - selected.breeze_phase,
            control.clone(),
        )?;
        let duration = clip.total_frames() as f32 / clip.sample_rate() as f32;
        let time = if time_seconds.is_finite() {
            time_seconds.max(0.0)
        } else {
            0.0
        };
        let phase = (selected.breeze_phase + time / duration).fract();
        log::debug!("[TREE_LEAF_AUDIO] tree={} generation={} breeze={} strong={} breeze_phase={:.6} strong_phase={:.6} loop_seconds={duration} one_voice=true", key.tree_id(), key.generation(), selected.breeze + 1, selected.strong + 1, selected.breeze_phase, selected.strong_phase);
        Ok(PreparedLeafLoop {
            clip,
            control,
            phase,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn bank() -> RecordedLeafBank {
        let clip = ResidentClip::from_mono_pcm(vec![0.01; 80], 10).unwrap();
        RecordedLeafBank {
            breeze: vec![clip.clone(); 4],
            strong: vec![clip; 4],
            seed: 91,
        }
    }

    #[test]
    fn sources_choose_both_groups_and_both_phases_independently_without_rerolling() {
        let bank = bank();
        let mut pairs = HashSet::new();
        let mut phases = HashSet::new();
        for tree in 0..256 {
            let key = CanopyAudioGenerationKey::new(tree, 2);
            let s = bank.selection(key);
            assert_eq!(s, bank.selection(key));
            assert_ne!(s, bank.selection(CanopyAudioGenerationKey::new(tree, 3)));
            pairs.insert((s.breeze, s.strong));
            phases.insert((s.breeze_phase.to_bits(), s.strong_phase.to_bits()));
            assert!((0.0..1.0).contains(&s.breeze_phase));
            assert!((0.0..1.0).contains(&s.strong_phase));
            assert_ne!(s.breeze_phase, s.strong_phase);
        }
        assert_eq!(pairs.len(), 16);
        assert_eq!(phases.len(), 256);
    }

    #[test]
    fn mode_reentry_keeps_selection_and_advances_phase_at_original_speed() {
        let bank = bank();
        let key = CanopyAudioGenerationKey::new(42, 1);
        let a = bank.prepare(key, 0.0, 0.2).unwrap();
        let b = bank.prepare(key, 2.0, 0.8).unwrap();
        let later = bank.prepare(key, 8.0, 1.0).unwrap();
        assert!(((b.phase - a.phase).rem_euclid(1.0) - 0.25).abs() < 1e-6);
        assert!((later.phase - a.phase).abs() < 1e-6);
        assert_eq!(a.control.position(), 0.2);
        assert_eq!(b.control.position(), 0.8);
        a.control.set(0.0);
        assert_eq!(b.control.position(), 0.8);
    }
}
