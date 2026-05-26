use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindResponseCurve {
    pub min_strength: f32,
    pub max_strength: f32,
    pub power: f32,
}

impl WindResponseCurve {
    pub fn factor(self, normalized_strength: f32) -> f32 {
        let normalized_strength = normalized_strength.clamp(0.0, 1.0);
        let (min_strength, max_strength) = if self.min_strength <= self.max_strength {
            (self.min_strength, self.max_strength)
        } else {
            (self.max_strength, self.min_strength)
        };
        let range = max_strength - min_strength;
        if range <= f32::EPSILON {
            return if normalized_strength >= max_strength {
                1.0
            } else {
                0.0
            };
        }

        let scaled = ((normalized_strength - min_strength) / range).clamp(0.0, 1.0);
        scaled.powf(self.power.max(0.001))
    }
}

const WIND_DIRECTION_SEED: i32 = 1729;
const WIND_STRENGTH_SEED: i32 = 2843;
const WIND_DIRECTION_FREQUENCY: f32 = 0.0025;
const WIND_STRENGTH_FREQUENCY: f32 = 0.00125;
const WIND_SAMPLE_SCALE: f32 = 256.0;
const WIND_SECOND_SAMPLE_OFFSET: Vec2 = Vec2::new(57.23, -113.87);
const WIND_STRENGTH_OFFSET: Vec2 = Vec2::new(-211.0, 83.0);
const WIND_DIRECTION_TIME_SCROLL: Vec2 = Vec2::new(0.2, -0.3);
const WIND_STRENGTH_TIME_SCROLL: Vec2 = Vec2::new(-0.3, 0.5);
const WIND_TIME_SCALE: f32 = 200.0;
const WIND_DIRECTION_DETAIL_STRENGTH: f32 = 0.35;
const TWO_PI: f32 = std::f32::consts::PI * 2.0;

fn wind_noise_state(seed: i32, frequency: f32) -> FastNoiseLite {
    let mut state = FastNoiseLite::with_seed(seed);
    state.set_noise_type(Some(NoiseType::OpenSimplex2));
    state.set_fractal_type(Some(FractalType::FBm));
    state.set_fractal_octaves(Some(4));
    state.set_frequency(Some(frequency));
    state.set_fractal_lacunarity(Some(2.0));
    state.set_fractal_gain(Some(0.5));
    state
}

pub struct Wind {
    direction_noise: FastNoiseLite,
    strength_noise: FastNoiseLite,
}

impl Default for Wind {
    fn default() -> Self {
        Self::new()
    }
}

impl Wind {
    pub fn new() -> Self {
        Self {
            direction_noise: wind_noise_state(WIND_DIRECTION_SEED, WIND_DIRECTION_FREQUENCY),
            strength_noise: wind_noise_state(WIND_STRENGTH_SEED, WIND_STRENGTH_FREQUENCY),
        }
    }

    pub fn sample_normalized(&self, world_pos: Vec3, time: f32) -> Vec3 {
        let sample_pos = Vec2::new(world_pos.x, world_pos.z) * WIND_SAMPLE_SCALE;
        let scroll_time = time * WIND_TIME_SCALE;
        let direction_time = WIND_DIRECTION_TIME_SCROLL * scroll_time;
        let strength_time = WIND_STRENGTH_TIME_SCROLL * scroll_time;

        let primary_direction = self.direction_noise.get_noise_2d(
            sample_pos.x + direction_time.x,
            sample_pos.y + direction_time.y,
        );
        let detail_direction = self.direction_noise.get_noise_2d(
            sample_pos.x + WIND_SECOND_SAMPLE_OFFSET.x + direction_time.x,
            sample_pos.y + WIND_SECOND_SAMPLE_OFFSET.y + direction_time.y,
        );

        let base_angle = (primary_direction * 0.5 + 0.5) * TWO_PI;
        let detail_angle = detail_direction * WIND_DIRECTION_DETAIL_STRENGTH;
        let angle = base_angle + detail_angle;
        let direction = Vec2::new(angle.cos(), angle.sin());

        let strength_noise = self.strength_noise.get_noise_2d(
            sample_pos.x + WIND_STRENGTH_OFFSET.x + strength_time.x,
            sample_pos.y + WIND_STRENGTH_OFFSET.y + strength_time.y,
        );
        let normalized_strength = strength_noise * 0.5 + 0.5;

        let wind_planar = direction * normalized_strength;
        Vec3::new(wind_planar.x, 0.0, wind_planar.y)
    }

    pub fn sample_response(
        &self,
        world_pos: Vec3,
        time: f32,
        response_curve: WindResponseCurve,
    ) -> f32 {
        response_curve.factor(self.sample_normalized(world_pos, time).length())
    }
}
