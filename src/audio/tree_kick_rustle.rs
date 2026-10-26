//! Finite leaf-contact excitation, separate from ambient wind and acoustic transport.
//! Clips are prepared off the realtime callback and before the next K event.
use super::mixer::AudioCategory;
use super::{CanopyAcousticDescriptor, CanopyDistributedEmitterAdapter, SpatialSoundManager};
mod synthesis;
use anyhow::{ensure, Result};
use glam::Vec3;
use petalsonic::ResidentClip;
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use synthesis::{TreeRustleControl, TreeRustleFactory, TreeRustleParams};
use uuid::Uuid;

const SAMPLE_RATE: u32 = 48_000;
const VARIANTS: usize = 4;
const MAX_VOICES: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct KickRustleTuning {
    pub duration: f32,
    pub brightness: f32,
    pub crackle: f32,
    pub body: f32,
    pub shake_frequency: f32,
}
impl KickRustleTuning {
    fn validate(self) -> Result<()> {
        ensure!(
            self.duration.is_finite()
                && (0.25..=3.).contains(&self.duration)
                && self.brightness.is_finite()
                && (0. ..=1.).contains(&self.brightness)
                && self.crackle.is_finite()
                && (0. ..=1.).contains(&self.crackle)
                && self.body.is_finite()
                && (0. ..=1.5).contains(&self.body)
                && self.shake_frequency.is_finite()
                && (1. ..=7.).contains(&self.shake_frequency),
            "invalid kick rustle tuning"
        );
        Ok(())
    }
}

// A mechanical-energy envelope, not a measured room/tree impulse response. The
// positive rebound modulation avoids an audible pure tone or perfectly periodic shaker.
pub(super) fn excitation_envelope(time: f32, duration: f32, frequency: f32) -> f32 {
    if time <= 0. || time >= duration {
        return 0.;
    }
    let attack = 1. - (-time / 0.009).exp();
    let decay = (-5. * time / duration).exp();
    let rebound = 0.7 + 0.3 * (std::f32::consts::TAU * frequency * time).cos().powi(2);
    let end_fade = ((duration - time) / 0.04).clamp(0., 1.);
    attack * decay * rebound * end_fade * end_fade
}
fn kick_params(tuning: KickRustleTuning) -> TreeRustleParams {
    TreeRustleParams {
        // Reduce the warm low/mid bed; prioritize friction/contact without
        // inventing a foot/trunk thump or modifying ambient tree sound.
        gustiness: 0.,
        leaf_density: 1.35,
        dryness: 0.28,
        branch: 0.,
        air: 0.02,
        leaf_body: tuning.body,
        brightness: 0.35 + 0.65 * tuning.brightness,
        crackle: tuning.crackle.sqrt(),
    }
}
fn render_variant(tuning: KickRustleTuning, variant: usize) -> Result<ResidentClip> {
    let control = Arc::new(TreeRustleControl::with_params(kick_params(tuning)));
    let factory = TreeRustleFactory::new(
        0x6b69_636b_7275_7374 ^ (variant as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        control,
    );
    let samples = factory.render_excited_pcm(SAMPLE_RATE, tuning.duration, tuning.shake_frequency);
    Ok(ResidentClip::from_mono_pcm(samples, SAMPLE_RATE)?)
}
/// A canonical canopy domain, independent of listener/camera/nearest-point selection.
struct CanopyKickEmission {
    origin_world: Vec3,
    extent: petalsonic::SourceExtent,
    sample_count: usize,
}
impl CanopyKickEmission {
    fn from_descriptor(descriptor: &CanopyAcousticDescriptor) -> Result<Self> {
        ensure!(
            !descriptor.samples().is_empty(),
            "kick canopy has no acoustic samples"
        );
        // Compute in tree-local coordinates: moving the tree cannot change weights/anchor.
        let mut center = glam::DVec3::ZERO;
        let mut power = 0_f64;
        for sample in descriptor.samples() {
            let weight = sample.weight() as f64;
            center += sample.position_tree_voxels().as_dvec3() * weight;
            power += weight;
        }
        ensure!(power.is_finite() && power > 0., "invalid kick canopy power");
        let anchor = (center / power).as_vec3();
        let origin_world = descriptor.tree_origin_world() + anchor / 256.;
        ensure!(origin_world.is_finite(), "invalid kick canopy origin");
        let extent = CanopyDistributedEmitterAdapter::source_extent_at_anchor(descriptor, anchor)?;
        Ok(Self {
            origin_world,
            extent,
            sample_count: descriptor.samples().len(),
        })
    }
}

struct Active {
    source: Uuid,
    deadline: Instant,
}
pub(crate) struct KickRustlePlayer {
    spatial: SpatialSoundManager,
    tuning: Option<KickRustleTuning>,
    clips: Vec<ResidentClip>,
    variant: usize,
    active: VecDeque<Active>,
}
impl KickRustlePlayer {
    pub fn new(spatial: SpatialSoundManager) -> Self {
        Self {
            spatial,
            tuning: None,
            clips: Vec::new(),
            variant: 0,
            active: VecDeque::new(),
        }
    }
    pub fn prepare(&mut self, tuning: KickRustleTuning) -> Result<()> {
        tuning.validate()?;
        if self.tuning == Some(tuning) {
            return Ok(());
        }
        let started = Instant::now();
        let clips = (0..VARIANTS)
            .map(|variant| render_variant(tuning, variant))
            .collect::<Result<Vec<_>>>()?;
        self.clips = clips;
        self.tuning = Some(tuning);
        log::info!("[TREE_KICK][RUSTLE_PREPARE] variants={VARIANTS} duration={} elapsed_us={} realtime_dsp=false", tuning.duration, started.elapsed().as_micros());
        Ok(())
    }
    pub fn play(&mut self, descriptor: &CanopyAcousticDescriptor, volume_db: f32) -> Result<()> {
        let emission = CanopyKickEmission::from_descriptor(descriptor)?;
        let position = emission.origin_world;
        ensure!(
            volume_db.is_finite() && (-40. ..=6.).contains(&volume_db),
            "invalid kick rustle placement/gain"
        );
        let tuning = self
            .tuning
            .ok_or_else(|| anyhow::anyhow!("kick rustle clips not prepared"))?;
        let now = Instant::now();
        self.retire(now);
        while self.active.len() >= MAX_VOICES {
            self.spatial
                .remove_source(self.active.pop_front().unwrap().source);
        }
        let variant = self.variant % self.clips.len();
        let source = self.spatial.add_spatial_resident_one_shot(
            AudioCategory::Leaves,
            self.clips[variant].clone(),
            volume_db,
            position,
            emission.extent,
            // Exact energy aggregation across samples, no ambient gain floor.
            // This policy is orthogonal to the WeightedSamples source domain.
            petalsonic::OcclusionProfile::PointExact,
        )?;
        self.variant = self.variant.wrapping_add(1);
        self.active.push_back(Active {
            source,
            deadline: now
                + Duration::from_secs_f32(tuning.duration + 0.25)
                + petalsonic::ACOUSTIC_START_MAX_DELAY,
        });
        log::info!("[TREE_KICK][RUSTLE] once=true wind_independent=true variant={variant} duration={} active={} max_voices={MAX_VOICES} position={position:?} canopy_samples={} generation={} power_normalized=true distributed=true", tuning.duration, self.active.len(), emission.sample_count, descriptor.generation());
        Ok(())
    }
    pub fn retire(&mut self, now: Instant) {
        let spatial = &self.spatial;
        // New short clips can finish before older long clips after live tuning.
        self.active.retain(|voice| {
            if voice.deadline <= now {
                spatial.remove_source(voice.source);
                false
            } else {
                true
            }
        });
    }
    #[cfg(test)]
    pub(super) fn latest_spatial_domain(
        &self,
    ) -> Option<(Vec3, petalsonic::SourceExtent, petalsonic::OcclusionProfile)> {
        self.active
            .back()
            .and_then(|a| self.spatial.registered_spatial_domain(a.source))
    }

    pub fn voice_count(&self) -> usize {
        self.active.len()
    }
    pub fn clear(&mut self) {
        for voice in self.active.drain(..) {
            self.spatial.remove_source(voice.source);
        }
    }
}
impl Drop for KickRustlePlayer {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn canopy(origin: Vec3) -> CanopyAcousticDescriptor {
        let leaves = [
            Vec3::new(-24., 50., 0.),
            Vec3::new(24., 90., 12.),
            Vec3::new(0., 75., -24.),
        ]
        .into_iter()
        .map(|position| crate::tree_gen::LeafPlacement {
            position,
            anchor: Vec3::ZERO,
        })
        .collect::<Vec<_>>();
        CanopyAcousticDescriptor::build(7, origin, 123, &leaves, &[])
    }
    #[test]
    fn kick_canopy_domain_is_stable_power_normalized_and_keeps_exact_sample_world_positions() {
        let descriptor = canopy(Vec3::new(1., 0.4, 1.));
        let a = CanopyKickEmission::from_descriptor(&descriptor).unwrap();
        let b = CanopyKickEmission::from_descriptor(&descriptor).unwrap();
        assert_eq!(a.origin_world, b.origin_world);
        assert_eq!(a.extent, b.extent);
        let samples = a.extent.weighted().unwrap().samples();
        assert_eq!(samples.len(), descriptor.samples().len());
        assert!(
            (samples
                .iter()
                .map(petalsonic::ExtentSample::power_weight)
                .sum::<f32>()
                - 1.)
                .abs()
                < 1e-6
        );
        for (mapped, original) in samples.iter().zip(descriptor.samples()) {
            assert_eq!(
                mapped.id(),
                petalsonic::ExtentSampleId(original.id().value())
            );
            let p = mapped.local_position();
            let world = a.origin_world + Vec3::new(p.x, p.y, p.z);
            assert!(world.distance(descriptor.sample_world_position(original)) < 1e-6);
            assert!((mapped.power_weight() - original.weight()).abs() < 1e-6);
        }
        // Canonical origin follows the canopy, never the trunk or listener-nearest sample.
        assert!(a.origin_world.y > descriptor.tree_origin_world().y + 0.15);
        let moved = CanopyKickEmission::from_descriptor(&canopy(
            descriptor.tree_origin_world() + Vec3::new(32., 0., -7.),
        ))
        .unwrap();
        assert_eq!(a.extent, moved.extent);
        assert!((moved.origin_world - a.origin_world - Vec3::new(32., 0., -7.)).length() < 2e-6);
    }
    #[test]
    fn sampleless_canopy_never_falls_back_to_a_trunk_sound() {
        let descriptor = CanopyAcousticDescriptor::build(1, Vec3::ZERO, 0, &[], &[]);
        assert!(CanopyKickEmission::from_descriptor(&descriptor).is_err());
    }
    #[test]
    fn kick_excitation_is_finite_fast_attack_decaying_and_exactly_silent_at_boundaries() {
        let envelope = |t| excitation_envelope(t, 1.1, 2.55);
        assert_eq!(envelope(0.), 0.);
        assert_eq!(envelope(1.1), 0.);
        assert_eq!(envelope(3.), 0.);
        assert!(envelope(0.02) > 0.6);
        assert!(envelope(0.5) < 0.11);
        assert!(envelope(1.09) < 0.001);
        for t in 0..1100 {
            assert!((0. ..=1.).contains(&envelope(t as f32 / 1000.)));
        }
    }
    #[test]
    fn clearer_kick_texture_has_more_high_band_contact_relative_to_the_low_bed() {
        let tuning = KickRustleTuning {
            duration: 1.1,
            brightness: 0.42,
            crackle: 0.18,
            body: 0.28,
            shake_frequency: 2.55,
        };
        let old = TreeRustleParams {
            gustiness: 0.,
            leaf_density: 1.35,
            dryness: 0.2,
            branch: 0.,
            air: 0.08,
            leaf_body: 1.14,
            brightness: 0.42,
            crackle: 0.18,
        };
        let ratio = |params, seed| {
            let pcm =
                TreeRustleFactory::new(seed, Arc::new(TreeRustleControl::with_params(params)))
                    .render_excited_pcm(SAMPLE_RATE, tuning.duration, tuning.shake_frequency);
            let mut low = 0_f64;
            let mut mid = 0_f64;
            let mut bass = 0_f64;
            let mut high = 0_f64;
            let a = 1. - (-std::f64::consts::TAU * 800. / SAMPLE_RATE as f64).exp();
            let b = 1. - (-std::f64::consts::TAU * 2500. / SAMPLE_RATE as f64).exp();
            for sample in pcm {
                let x = sample as f64;
                low += a * (x - low);
                mid += b * (x - mid);
                bass += low * low;
                high += (x - mid).powi(2);
            }
            high / bass
        };
        let mut previous = 0.;
        let mut candidate = 0.;
        for seed in [11, 22, 33, 44] {
            previous += ratio(old, seed);
            candidate += ratio(kick_params(tuning), seed);
        }
        eprintln!(
            "kick texture high/low proxy: previous={} candidate={}",
            previous / 4.,
            candidate / 4.
        );
        assert!(candidate > previous * 2.);
        assert!(KickRustleTuning {
            body: f32::NAN,
            ..tuning
        }
        .validate()
        .is_err());
        assert!(KickRustleTuning { body: 2., ..tuning }.validate().is_err());
    }
    #[test]
    fn invalid_kick_rustle_parameters_are_rejected_before_allocating_or_playing() {
        let tuning = KickRustleTuning {
            duration: 1.1,
            brightness: 0.42,
            crackle: 0.18,
            body: 0.28,
            shake_frequency: 2.55,
        };
        assert!(tuning.validate().is_ok());
        for duration in [0., f32::INFINITY, f32::NAN, 4.] {
            assert!(KickRustleTuning { duration, ..tuning }.validate().is_err());
        }
        assert!(KickRustleTuning {
            brightness: -1.,
            ..tuning
        }
        .validate()
        .is_err());
        assert!(KickRustleTuning {
            crackle: 2.,
            ..tuning
        }
        .validate()
        .is_err());
    }
}
