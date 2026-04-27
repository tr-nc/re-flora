pub const WORLD_TICK_SECONDS_DEFAULT: f32 = 0.05;

pub fn clamp_world_tick_seconds(seconds: f32) -> f32 {
    seconds.max(1.0 / 240.0)
}
