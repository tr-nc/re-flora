//! Small bounded nocturnal population. Ecology chooses real habitat roots; the
//! swarm owns lifetimes, independent blink clocks and sample-held model poses.
use super::{ButterflyFrame, ParticleRenderKind, ParticleSnapshot};
use glam::{Vec3, Vec4};
pub(crate) const CAPACITY: usize = 32;
struct Firefly {
    anchor: Vec3,
    born: f32,
    phase: f32,
}
#[derive(Default)]
pub(crate) struct Fireflies {
    flies: Vec<Firefly>,
    serial: u32,
}
pub(crate) fn flash(time: f32, phase: f32) -> f32 {
    // Independent uniform phase offsets, with no phase/period correlation that
    // could accidentally synchronize the whole swarm into long dark intervals.
    let p = (time / 3.2 + phase).rem_euclid(1.);
    if p >= 0.24 {
        0.
    } else {
        (p / 0.24 * std::f32::consts::PI).sin().powi(2)
    }
}
fn position(anchor: Vec3, time: f32, phase: f32) -> Vec3 {
    let a = time * (0.7 + phase * 0.35) + phase * std::f32::consts::TAU;
    anchor
        + Vec3::new(
            a.sin() * 0.065,
            0.07 + (a * 0.73).sin() * 0.025,
            (a * 0.81).cos() * 0.06,
        )
}
impl Fireflies {
    pub fn len(&self) -> usize {
        self.flies.len()
    }
    pub fn has_capacity(&self) -> bool {
        self.flies.len() < CAPACITY
    }
    pub fn spawn(&mut self, anchor: Vec3, now: f32) -> bool {
        if !self.has_capacity() || !anchor.is_finite() || !now.is_finite() {
            return false;
        }
        self.serial = self.serial.wrapping_add(1);
        let hash = self.serial.wrapping_mul(0x9e3779b9);
        self.flies.push(Firefly {
            anchor,
            born: now,
            phase: (hash >> 8) as f32 / 16777216.,
        });
        true
    }
    pub fn update(&mut self, now: f32, enabled: bool) {
        self.flies.retain(|f| enabled && now - f.born < 45.);
    }
    pub fn append(&self, frame: ButterflyFrame, night: f32, out: &mut Vec<ParticleSnapshot>) {
        if night <= 0. {
            return;
        }
        let time = frame.time_seconds();
        for f in &self.flies {
            let age = (time - f.born).max(0.);
            let fade = (age / 1.5).min(1.) * ((45. - age) / 2.).clamp(0., 1.);
            if fade <= 0. {
                continue;
            }
            let p = position(f.anchor, time, f.phase);
            out.push(ParticleSnapshot {
                position_ws: p,
                velocity: (position(f.anchor, time + 0.01, f.phase) - p) * 100.,
                color: Vec4::new(0.18, 0.22, 0.04, night * fade),
                size: 0.018,
                kind: ParticleRenderKind::Firefly,
                palette_index: 0,
                animation_phase_offset: f.phase,
                animation_sample_time: Some(time),
                butterfly_wingbeat: None,
                butterfly_body_pose: None,
                leaf_orientation: None,
                leaf_shape_seed: None,
                leaf_geometry: None,
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn population_is_bounded_night_only_and_expires() {
        let mut swarm = Fireflies::default();
        for _ in 0..CAPACITY {
            assert!(swarm.spawn(Vec3::ONE, 0.));
        }
        assert!(!swarm.spawn(Vec3::ONE, 0.));
        let mut snapshots = Vec::new();
        swarm.append(ButterflyFrame::at(2., 24), 0., &mut snapshots);
        assert!(snapshots.is_empty());
        swarm.append(ButterflyFrame::at(2., 24), 1., &mut snapshots);
        assert_eq!(snapshots.len(), CAPACITY);
        assert!(snapshots
            .iter()
            .all(|p| p.kind == ParticleRenderKind::Firefly && p.position_ws.y > 1.));
        swarm.update(46., true);
        assert!(swarm.flies.is_empty());
        swarm.spawn(Vec3::ZERO, 47.);
        swarm.update(48., false);
        assert!(swarm.flies.is_empty());
    }
    #[test]
    fn blink_clocks_have_true_dark_intervals_and_distinct_phases() {
        let samples: Vec<_> = (0..600).map(|i| flash(i as f32 / 100., 0.1)).collect();
        assert!(samples.iter().any(|&p| p > 0.99));
        assert!(samples.iter().filter(|&&p| p == 0.).count() > 300);
        assert!(
            (0..100).any(|i| (flash(i as f32 / 20., 0.1) - flash(i as f32 / 20., 0.7)).abs() > 0.5)
        );
    }
    #[test]
    fn one_presentation_tick_holds_pose_and_blink_clock() {
        let mut swarm = Fireflies::default();
        swarm.spawn(Vec3::ZERO, 0.);
        let mut a = Vec::new();
        let mut b = Vec::new();
        swarm.append(ButterflyFrame::at(2.001, 24), 1., &mut a);
        swarm.append(ButterflyFrame::at(2.002, 24), 1., &mut b);
        assert_eq!(a[0].position_ws, b[0].position_ws);
        assert_eq!(a[0].animation_sample_time, b[0].animation_sample_time);
    }
}
