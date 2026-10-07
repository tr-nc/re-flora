//! Soil tool presentation only. The bag and bounded ballistic grains never edit
//! terrain, fertilize it, own inventory, or participate in the deposition clock.
use super::*;
use super::{input::TerrainEditHover, player_tools::ContinuousTerrainToolAction};
use crate::{
    particles::{ParticleRenderKind, ParticleSnapshot},
    soil_bag::Instance,
};

const MAX_GRAINS: usize = 128;
const GRAINS_PER_SECOND: f32 = 90.;
const GRAVITY: f32 = 3.2;
const GRAIN_SIZE: f32 = 1. / 256.;
#[derive(Clone, Copy)]
struct Grain {
    position: Vec3,
    velocity: Vec3,
    age: f32,
}
#[derive(Default)]
pub(super) struct State {
    instance: Option<Instance>,
    pub allow_flow: bool,
    pub review: Option<super::soil_bag_review::Review>,
    grains: Vec<Grain>,
    credit: f32,
    serial: u32,
}
impl State {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            review: super::soil_bag_review::Review::from_env()?,
            ..Self::default()
        })
    }
    pub fn current_instance(&self) -> Option<Instance> {
        self.instance
    }
    pub fn note_added(&mut self, added: u32) {
        self.allow_flow = true;
        if let Some(review) = &mut self.review {
            review.added += added;
        }
    }
    fn advance(&mut self, dt: f32, mut collides: impl FnMut(Vec3, Vec3) -> bool) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.grains.retain_mut(|grain| {
            let old = grain.position;
            grain.position += grain.velocity * dt - Vec3::Y * (0.5 * GRAVITY * dt * dt);
            grain.velocity.y -= GRAVITY * dt;
            grain.age += dt;
            grain.age < 0.7 && !collides(old, grain.position)
        });
        let Some(pose) = self.instance.filter(|p| p.pouring && self.allow_flow) else {
            self.credit = 0.;
            return;
        };
        self.credit += dt * GRAINS_PER_SECOND;
        let requested = self.credit.floor() as usize;
        self.credit -= requested as f32;
        for _ in 0..requested.min(MAX_GRAINS - self.grains.len()) {
            self.serial = self.serial.wrapping_add(1);
            let mut rng = self.serial.wrapping_mul(747796405).wrapping_add(2891336453);
            let mut random = || {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                (rng & 65535) as f32 / 65535. - 0.5
            };
            let jitter = pose.rotation * Vec3::new(random() * 0.010, 0., random() * 0.014);
            self.grains.push(Grain {
                position: pose.mouth() + jitter,
                velocity: Vec3::new(random() * 0.018, -0.12, random() * 0.018),
                age: 0.,
            });
        }
    }
    fn append(&self, snapshots: &mut Vec<ParticleSnapshot>, color: Vec4) {
        snapshots.extend(self.grains.iter().map(|g| ParticleSnapshot {
            position_ws: g.position,
            velocity: g.velocity,
            color,
            size: GRAIN_SIZE,
            kind: ParticleRenderKind::TerrainVoxel,
            palette_index: 0,
            animation_phase_offset: 0.,
            animation_sample_time: None,
            butterfly_wingbeat: None,
            leaf_orientation: None,
            leaf_shape_seed: None,
            leaf_geometry: None,
        }));
    }
}
impl App {
    pub(super) fn update_soil_bag_instance(&mut self, hover: Option<TerrainEditHover>) {
        let pouring = self.player_tools.active_continuous_action()
            == Some(ContinuousTerrainToolAction::SoilDeposit);
        let instance = hover
            .filter(|_| self.player_tools.selected_tool() == PlayerTool::Soil)
            .map(|hit| {
                Instance::over_surface(hit.center, self.tracer.camera_pose().position, pouring)
            });
        // A fresh asynchronous hover may be temporarily absent after terrain
        // publication. Hide/suspend the cue without forgetting the last accepted
        // dose; a resolved miss/rejection still stops flow in try_soil_deposition.
        if !pouring
            || self.player_tools.selected_tool() != PlayerTool::Soil
            || !self.terrain_edit_pointer_available()
            || hover.is_some_and(|hit| !hit.is_editable)
        {
            self.soil_bag_preview.allow_flow = false;
        }
        self.soil_bag_preview.instance = instance;
        if let (Some(review), Some(pose)) = (&mut self.soil_bag_preview.review, instance) {
            review.hover_seen |= !pose.pouring;
            review.pour_seen |= pose.pouring;
        }
        self.tracer.set_soil_bag_instance(instance);
    }
    pub(super) fn append_soil_bag_grains(&mut self, dt: f32) {
        let mut state = std::mem::take(&mut self.soil_bag_preview);
        let mut contacts = 0;
        state.advance(dt, |from, to| {
            let segment = to - from;
            let distance = segment.length();
            let hit = distance > 1e-8
                && self
                    .query_terrain_ray_cpu(from, segment)
                    .is_some_and(|hit| hit.position.distance(from) <= distance + GRAIN_SIZE * 0.5);
            contacts += usize::from(hit);
            hit
        });
        if let Some(review) = &mut state.review {
            review.contacts += contacts;
            review.peak_grains = review.peak_grains.max(state.grains.len());
        }
        state.append(
            &mut self.particle_snapshots,
            Vec3::from(crate::soil_bag::asset().colors[3]).extend(1.),
        );
        self.soil_bag_preview = state;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soil_bag_grains_are_bounded_ballistic_fx_and_only_emit_after_successful_writes() {
        let pose = Instance::over_surface(Vec3::ZERO, Vec3::new(0., 1., 2.), true);
        let mut state = State {
            instance: Some(pose),
            ..State::default()
        };
        state.advance(0.05, |_, _| false);
        assert!(state.grains.is_empty());
        state.allow_flow = true;
        state.advance(0.05, |_, _| false);
        assert_eq!(state.grains.len(), 4);
        assert!(state
            .grains
            .iter()
            .all(|g| g.position.distance(pose.mouth()) < 0.01));
        state.allow_flow = false;
        let y = state.grains[0].position.y;
        state.advance(0.05, |_, _| false);
        assert!(state.grains[0].position.y < y);
        for _ in 0..100 {
            state.allow_flow = true;
            state.advance(0.1, |_, _| false);
            assert!(state.grains.len() <= MAX_GRAINS);
        }
        state.allow_flow = false;
        for _ in 0..8 {
            state.advance(0.1, |_, _| false);
        }
        assert!(state.grains.is_empty());
    }
    #[test]
    fn pending_hover_suspends_grains_without_inventing_a_source_or_resetting_accepted_flow() {
        let mut state = State {
            allow_flow: true,
            ..State::default()
        };
        state.advance(0.1, |_, _| false);
        assert!(state.grains.is_empty());
        assert_eq!(state.credit, 0.);
        state.instance = Some(Instance::over_surface(Vec3::ZERO, Vec3::Z, true));
        state.advance(0.05, |_, _| false);
        assert_eq!(state.grains.len(), 4);
    }

    #[test]
    fn soil_bag_grain_contacts_expire_without_creating_any_material() {
        let mut state = State {
            instance: Some(Instance::over_surface(Vec3::ZERO, Vec3::Z, true)),
            allow_flow: true,
            ..State::default()
        };
        state.advance(0.1, |_, _| false);
        assert!(!state.grains.is_empty());
        state.allow_flow = false;
        let mut checks = 0;
        state.advance(0.1, |_, _| {
            checks += 1;
            true
        });
        assert!(checks > 0);
        assert!(state.grains.is_empty());
        let mut snapshots = Vec::new();
        state.append(&mut snapshots, Vec4::ONE);
        assert!(snapshots.is_empty());
    }
}
