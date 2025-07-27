use std::time::{Duration, Instant};

pub struct TimeInfo {
    // the moment the TimeInfo struct was created. Used to calculate total elapsed time.
    start_instant: Instant,
    // the time of the last call to `update()`. Used to calculate delta time.
    last_update_instant: Instant,

    // time elapsed since the last frame, in seconds. This is the raw, unscaled delta time.
    dt: f32,
    // a factor to scale delta time. Useful for slow-motion (scale < 1.0) or fast-forward (scale > 1.0).
    time_scale: f32,

    // --- Fields for calculating and displaying smoothed FPS ---
    display_update_interval: f32, // in seconds
    fps_accumulator: f32,
    fps_frame_count: u32,
    display_fps_value: f32,

    // --- Overall statistics ---
    total_frame_count: u64,
}

impl Default for TimeInfo {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            start_instant: now,
            last_update_instant: now,
            dt: 0.0,
            time_scale: 1.0,
            display_update_interval: 0.5, // default update interval: 500 ms
            fps_accumulator: 0.0,
            fps_frame_count: 0,
            display_fps_value: 0.0,
            total_frame_count: 0,
        }
    }
}

impl TimeInfo {
    /// Creates a new TimeInfo with a specified display FPS update interval in milliseconds.
    #[allow(dead_code)]
    pub fn new(display_interval_ms: u64) -> Self {
        Self {
            display_update_interval: display_interval_ms as f32 / 1000.0,
            ..Default::default()
        }
    }

    /// Updates the time measurements. Should be called once per frame.
    ///
    /// This method:
    /// - Computes the delta time (dt) since the last update.
    /// - Updates an accumulator and frame count for smoothed FPS calculation.
    /// - Updates the total frame count.
    pub fn update(&mut self) {
        let now = Instant::now();
        // calculate delta time (in seconds) since the last update.
        self.dt = now.duration_since(self.last_update_instant).as_secs_f32();
        self.last_update_instant = now;

        // increment total frames rendered.
        self.total_frame_count += 1;

        // accumulate dt and count frames for display fps calculations.
        self.fps_accumulator += self.dt;
        self.fps_frame_count += 1;

        // when the accumulated time exceeds the update_interval,
        // calculate an average FPS for display purposes.
        if self.fps_accumulator >= self.display_update_interval {
            self.display_fps_value = self.fps_frame_count as f32 / self.fps_accumulator;
            // reset for the next interval.
            self.fps_accumulator = 0.0;
            self.fps_frame_count = 0;
        }
    }

    /// Returns the total time in seconds since the `TimeInfo` was created.
    pub fn time_since_start(&self) -> f32 {
        self.start_instant.elapsed().as_secs_f32()
    }

    /// Returns the total time as a `Duration` since the `TimeInfo` was created.
    #[allow(dead_code)]
    pub fn time_since_start_duration(&self) -> Duration {
        self.start_instant.elapsed()
    }

    /// Returns the delta time of the last frame, scaled by the `time_scale` factor.
    /// This is typically the value you want to use for game logic and physics updates.
    pub fn delta_time(&self) -> f32 {
        self.dt * self.time_scale
    }

    /// Returns the unscaled delta time of the last frame.
    #[allow(dead_code)]
    pub fn unscaled_delta_time(&self) -> f32 {
        self.dt
    }

    /// Returns the instantaneous frames per second, based on the last frame's unscaled delta time.
    /// This value can fluctuate wildly.
    #[allow(dead_code)]
    pub fn raw_fps(&self) -> f32 {
        if self.dt > 0.0 {
            1.0 / self.dt
        } else {
            0.0
        }
    }

    /// Returns a smoothed FPS value, updated at the configured `display_update_interval`.
    /// This is better for display purposes.
    pub fn display_fps(&self) -> f32 {
        self.display_fps_value
    }

    /// Returns the total number of frames that have passed since the start.
    #[allow(dead_code)]
    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }

    /// Gets the current time scale.
    #[allow(dead_code)]
    pub fn get_time_scale(&self) -> f32 {
        self.time_scale
    }

    /// Sets the time scale.
    #[allow(dead_code)]
    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = scale;
    }
}
