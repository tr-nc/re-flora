pub struct StrideCycle {
    phase: f32,
}

pub struct StrideSample {
    pub phase: f32,
    pub just_step: bool,
}

const RESTING_PHASE: f32 = 0.85;

impl StrideCycle {
    pub fn new() -> Self {
        Self {
            phase: RESTING_PHASE,
        }
    }

    pub fn reset(&mut self) {
        self.phase = RESTING_PHASE;
    }

    pub fn restart_after_step(&mut self) {
        self.phase = 0.0;
    }

    pub fn update(
        &mut self,
        dt: f32,
        is_active: bool,
        is_running: bool,
        walk_interval: f32,
        run_interval: f32,
    ) -> StrideSample {
        if !is_active {
            self.phase = RESTING_PHASE;
            return StrideSample {
                phase: RESTING_PHASE,
                just_step: false,
            };
        }

        let interval = if is_running {
            run_interval
        } else {
            walk_interval
        };

        let phase_step = dt / interval.max(f32::EPSILON);
        self.phase += phase_step;

        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            return StrideSample {
                phase: self.phase,
                just_step: true,
            };
        }

        StrideSample {
            phase: self.phase,
            just_step: false,
        }
    }
}
