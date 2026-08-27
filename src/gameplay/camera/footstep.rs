use glam::Vec3;

pub(super) const WALK_STRIDE_INTERVAL_SECONDS: f32 = 0.35;
pub(super) const RUN_STRIDE_INTERVAL_SECONDS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Run,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootstepKind {
    Jump,
    Land,
    Stride(Gait),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootstepSide {
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FootstepSurface {
    #[default]
    Unknown,
    Dirt,
    Sand,
    Stone,
    Wood,
    Stucco,
    Glass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootstepEvent {
    pub event_seq: u64,
    pub kind: FootstepKind,
    pub side: FootstepSide,
    pub contact_world: Vec3,
    pub surface: FootstepSurface,
    pub speed_mps: f32,
    pub sim_time_seconds: f64,
}

#[derive(Debug)]
pub(super) struct FootstepEventJournal {
    next_event_seq: u64,
    next_stride_side: FootstepSide,
    pending: Vec<FootstepEvent>,
}

impl Default for FootstepEventJournal {
    fn default() -> Self {
        Self {
            next_event_seq: 0,
            next_stride_side: FootstepSide::Left,
            pending: Vec::new(),
        }
    }
}

impl FootstepEventJournal {
    pub(super) fn record(
        &mut self,
        kind: FootstepKind,
        contact_world: Vec3,
        speed_mps: f32,
        sim_time_seconds: f64,
    ) {
        let side = match kind {
            FootstepKind::Stride(_) => {
                let side = self.next_stride_side;
                self.next_stride_side = match side {
                    FootstepSide::Left => FootstepSide::Right,
                    FootstepSide::Right => FootstepSide::Left,
                    FootstepSide::Both => unreachable!("stride parity cannot be both feet"),
                };
                side
            }
            FootstepKind::Jump | FootstepKind::Land => FootstepSide::Both,
        };
        let event_seq = self.next_event_seq;
        self.next_event_seq = self
            .next_event_seq
            .checked_add(1)
            .expect("footstep event sequence overflowed");
        self.pending.push(FootstepEvent {
            event_seq,
            kind,
            side,
            contact_world,
            surface: FootstepSurface::Unknown,
            speed_mps,
            sim_time_seconds,
        });
    }

    pub(super) fn drain(&mut self) -> Vec<FootstepEvent> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::{FootstepEventJournal, FootstepKind, FootstepSide, FootstepSurface, Gait};
    use glam::Vec3;

    #[test]
    fn semantic_events_are_monotonic_and_drained_exactly_once() {
        let mut journal = FootstepEventJournal::default();

        journal.record(FootstepKind::Jump, Vec3::new(1.0, 2.0, 3.0), 0.75, 10.0);
        journal.record(FootstepKind::Land, Vec3::new(4.0, 5.0, 6.0), 1.25, 10.5);

        let events = journal.drain();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_seq, 0);
        assert_eq!(events[1].event_seq, 1);
        assert_eq!(events[0].kind, FootstepKind::Jump);
        assert_eq!(events[0].side, FootstepSide::Both);
        assert_eq!(events[0].contact_world, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(events[0].surface, FootstepSurface::Unknown);
        assert_eq!(events[0].speed_mps, 0.75);
        assert_eq!(events[0].sim_time_seconds, 10.0);
        assert!(journal.drain().is_empty());
    }

    #[test]
    fn stride_events_alternate_sides_without_jump_or_land_changing_parity() {
        let mut journal = FootstepEventJournal::default();

        journal.record(FootstepKind::Stride(Gait::Walk), Vec3::ZERO, 1.0, 1.0);
        journal.record(FootstepKind::Jump, Vec3::ZERO, 1.0, 1.1);
        journal.record(FootstepKind::Land, Vec3::ZERO, 1.0, 1.2);
        journal.record(FootstepKind::Stride(Gait::Run), Vec3::ZERO, 2.0, 1.3);

        let events = journal.drain();
        assert_eq!(events[0].side, FootstepSide::Left);
        assert_eq!(events[1].side, FootstepSide::Both);
        assert_eq!(events[2].side, FootstepSide::Both);
        assert_eq!(events[3].side, FootstepSide::Right);
    }
}
