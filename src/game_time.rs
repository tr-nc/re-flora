pub const WORLD_TICK_SECONDS_DEFAULT: f32 = 0.05;
const SUN_POSITION_UPDATE_INTERVAL_TICKS: u32 = 1;

pub fn clamp_world_tick_seconds(seconds: f32) -> f32 {
    seconds.max(1.0 / 240.0)
}

#[derive(Clone, Copy, Debug)]
pub struct WorldClock {
    flora_tick: u32,
    tick_accumulator: f32,
    live_time_of_day: f32,
    sun_position_tick_accumulator: u32,
}

impl WorldClock {
    pub fn new(initial_flora_tick: u32, initial_time_of_day: f32) -> Self {
        Self {
            flora_tick: initial_flora_tick,
            tick_accumulator: 0.0,
            live_time_of_day: initial_time_of_day,
            sun_position_tick_accumulator: 0,
        }
    }

    pub fn flora_tick(&self) -> u32 {
        self.flora_tick
    }

    pub fn live_time_of_day(&self) -> f32 {
        self.live_time_of_day
    }

    pub fn set_live_time_of_day(&mut self, time_of_day: f32) {
        self.live_time_of_day = time_of_day;
    }

    /// Advances the canonical world tick and returns the number of ticks elapsed this frame.
    /// Pausing preserves the fractional remainder so resuming does not lose partial progress.
    pub fn advance_simulation(
        &mut self,
        frame_delta_time: f32,
        world_tick_seconds: f32,
        running: bool,
    ) -> u32 {
        if running {
            self.tick_accumulator +=
                frame_delta_time / clamp_world_tick_seconds(world_tick_seconds);
        }

        let mut elapsed_ticks = 0;
        while self.tick_accumulator >= 1.0 {
            self.flora_tick = self.flora_tick.wrapping_add(1);
            self.tick_accumulator -= 1.0;
            elapsed_ticks += 1;
        }
        elapsed_ticks
    }

    /// Advances live solar time on its intentionally throttled cadence.
    ///
    /// The persisted/manual start value remains caller-owned. This clock owns only the live value
    /// derived from it, so automatic cycling cannot silently rewrite configuration.
    pub fn advance_daynight(
        &mut self,
        elapsed_world_ticks: u32,
        world_tick_seconds: f32,
        day_cycle_minutes: f32,
        enabled: bool,
    ) -> bool {
        if !enabled || elapsed_world_ticks == 0 {
            return false;
        }

        self.sun_position_tick_accumulator += elapsed_world_ticks;
        let elapsed_sun_ticks = (self.sun_position_tick_accumulator
            / SUN_POSITION_UPDATE_INTERVAL_TICKS)
            * SUN_POSITION_UPDATE_INTERVAL_TICKS;
        self.sun_position_tick_accumulator %= SUN_POSITION_UPDATE_INTERVAL_TICKS;
        if elapsed_sun_ticks == 0 {
            return false;
        }

        self.live_time_of_day = advance_time_of_day(
            self.live_time_of_day,
            elapsed_sun_ticks,
            world_tick_seconds,
            day_cycle_minutes,
        );
        true
    }
}

fn advance_time_of_day(
    current_time_of_day: f32,
    elapsed_ticks: u32,
    world_tick_seconds: f32,
    day_cycle_minutes: f32,
) -> f32 {
    let time_speed = 1.0 / (day_cycle_minutes * 60.0);
    (current_time_of_day + elapsed_ticks as f32 * world_tick_seconds * time_speed) % 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_daynight_advances_live_time_without_mutating_the_start_value() {
        let persisted_start = 0.25;
        let mut clock = WorldClock::new(30, persisted_start);

        assert!(clock.advance_daynight(600, 0.05, 1.0, true));

        assert_eq!(persisted_start, 0.25);
        assert!((clock.live_time_of_day() - 0.75).abs() < 1.0e-6);
    }

    #[test]
    fn paused_simulation_preserves_fractional_tick_progress() {
        let mut clock = WorldClock::new(30, 0.25);

        assert_eq!(clock.advance_simulation(0.025, 0.05, true), 0);
        assert_eq!(clock.advance_simulation(1.0, 0.05, false), 0);
        assert_eq!(clock.advance_simulation(0.025, 0.05, true), 1);
        assert_eq!(clock.flora_tick(), 31);
    }

    #[test]
    fn world_tick_wraps_without_disrupting_elapsed_tick_count() {
        let mut clock = WorldClock::new(u32::MAX, 0.25);

        assert_eq!(clock.advance_simulation(0.05, 0.05, true), 1);
        assert_eq!(clock.flora_tick(), 0);
    }

    #[test]
    fn disabled_daynight_cycle_does_not_accumulate_deferred_solar_time() {
        let mut clock = WorldClock::new(30, 0.25);

        assert!(!clock.advance_daynight(600, 0.05, 1.0, false));
        assert!(clock.advance_daynight(1, 0.05, 1.0, true));

        let expected = 0.25 + 0.05 / 60.0;
        assert!((clock.live_time_of_day() - expected).abs() < 1.0e-6);
    }
}
