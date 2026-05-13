use super::pond::PondWaterSim;

impl PondWaterSim {
    /// Advance the pond by up to the configured fixed substeps.
    ///
    /// The actual MLS-MPM transfer loop is added in the next implementation
    /// step; keeping the fixed-step accumulator in place makes app integration
    /// deterministic from the start.
    pub fn update(&mut self, dt: f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        self.accumulator += dt.min(0.25);
        let substep_dt = self.config.substep_dt;
        while self.accumulator >= substep_dt {
            self.substep(substep_dt);
            self.accumulator -= substep_dt;
        }
    }

    pub fn substep(&mut self, _dt: f32) {}
}
