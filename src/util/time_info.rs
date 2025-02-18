use std::time::Instant;

pub struct TimeInfo {
    time: Instant,
    dt: f32,
    display_update_interval: f32, // in seconds
    accumulator: f32,
    frame_count: u32,
    display_fps_value: f32,
}

impl Default for TimeInfo {
    fn default() -> Self {
        Self {
            time: Instant::now(),
            dt: 0.0,
            display_update_interval: 0.5, // default update interval: 500 ms
            accumulator: 0.0,
            frame_count: 0,
            display_fps_value: 0.0,
        }
    }
}

impl TimeInfo {
    /// Creates a new TimeInfo with a specified display FPS update interval in milliseconds.
    pub fn new(display_interval_ms: u64) -> Self {
        Self {
            time: Instant::now(),
            dt: 0.0,
            display_update_interval: display_interval_ms as f32 / 1000.0,
            accumulator: 0.0,
            frame_count: 0,
            display_fps_value: 0.0,
        }
    }

    /// Updates the time measurements.
    ///
    /// This method:
    /// - Computes the time difference (dt) since the last update.
    /// - Updates an accumulator and frame count which is used
    ///   to compute a smoothed display FPS over the configured interval.
    /// - When the accumulator reaches the display update interval,
    ///   the display FPS value is updated and the accumulation is reset.
    pub fn update(&mut self) {
        // Calculate delta time (in seconds) since the last update
        self.dt = self.time.elapsed().as_secs_f32();
        self.time = Instant::now();

        // Accumulate dt and count frames for display fps calculations
        self.accumulator += self.dt;
        self.frame_count += 1;

        // When the accumulated time exceeds the update_interval,
        // calculate an average FPS for display purposes.
        if self.accumulator >= self.display_update_interval {
            self.display_fps_value = self.frame_count as f32 / self.accumulator;
            // Reset for the next interval.
            self.accumulator = 0.0;
            self.frame_count = 0;
        }
    }

    /// Returns the delta time (dt) of the last frame.
    pub fn delta_time(&self) -> f32 {
        self.dt
    }

    /// Returns the instantaneous (raw) frames per second,
    /// calculated as the reciprocal of the last delta time.
    pub fn raw_fps(&self) -> f32 {
        if self.dt > 0.0 {
            1.0 / self.dt
        } else {
            0.0
        }
    }

    /// Returns the FPS calculated for display, which is updated
    /// every x milliseconds (as set in the constructor).
    pub fn display_fps(&self) -> f32 {
        self.display_fps_value
    }
}
