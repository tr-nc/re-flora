use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{Vec2, Vec3};

pub const MAX_WIND_SOURCES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindSource {
    pub direction_degrees: f32,
    pub speed: f32,
    pub sharpness: f32,
    pub strength: f32,
}

impl WindSource {
    pub const fn new(direction_degrees: f32, speed: f32, sharpness: f32, strength: f32) -> Self {
        Self {
            direction_degrees,
            speed,
            sharpness,
            strength,
        }
    }
}

impl Default for WindSource {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

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

const WIND_GUST_MASK_SEED: i32 = 3181;
const WIND_GUST_MASK_FREQUENCY: f32 = 0.008;
const WIND_SAMPLE_SCALE: f32 = 256.0;
const WIND_TIME_SCALE: f32 = 170.0;
const WIND_GUST_SMOOTH_MIN: f32 = 0.52;
const WIND_GUST_SMOOTH_MAX: f32 = 0.82;
const WIND_SOURCE_OFFSETS: [Vec2; MAX_WIND_SOURCES] = [
    Vec2::new(149.0, -67.0),
    Vec2::new(-211.0, 307.0),
    Vec2::new(421.0, 83.0),
    Vec2::new(-97.0, -449.0),
];

fn wind_noise_state(seed: i32) -> FastNoiseLite {
    let mut state = FastNoiseLite::with_seed(seed);
    state.set_noise_type(Some(NoiseType::OpenSimplex2));
    state.set_fractal_type(Some(FractalType::FBm));
    state.set_fractal_octaves(Some(3));
    state.set_frequency(Some(WIND_GUST_MASK_FREQUENCY));
    state.set_fractal_lacunarity(Some(2.0));
    state.set_fractal_gain(Some(0.5));
    state
}

fn wind_safe_smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let low = edge0.min(edge1);
    let high = edge0.max(edge1);
    if high - low <= f32::EPSILON {
        return if x >= high { 1.0 } else { 0.0 };
    }

    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct Wind {
    gust_noises: [FastNoiseLite; MAX_WIND_SOURCES],
}

impl Default for Wind {
    fn default() -> Self {
        Self::new()
    }
}

impl Wind {
    pub fn new() -> Self {
        Self {
            gust_noises: std::array::from_fn(|index| {
                wind_noise_state(WIND_GUST_MASK_SEED + index as i32 * 997)
            }),
        }
    }

    pub fn sample_sources(&self, world_pos: Vec3, time: f32, sources: &[WindSource]) -> Vec3 {
        let sample_pos = Vec2::new(world_pos.x, world_pos.z) * WIND_SAMPLE_SCALE;
        let scroll_time = time * WIND_TIME_SCALE;
        let mut wind_planar = Vec2::ZERO;

        for (source_index, source) in sources.iter().take(MAX_WIND_SOURCES).enumerate() {
            let source_speed = source.speed.max(0.0);
            let source_strength = source.strength.max(0.0);
            if source_strength <= f32::EPSILON {
                continue;
            }

            let angle = source.direction_degrees.to_radians();
            let wind_direction = Vec2::new(angle.cos(), angle.sin());
            let time_offset = -wind_direction * scroll_time * source_speed;
            let wind_factor =
                self.sample_source_gust(source_index, sample_pos, time_offset, source.sharpness);
            wind_planar += wind_direction * (wind_factor * source_strength);
        }

        Vec3::new(wind_planar.x, 0.0, wind_planar.y)
    }

    pub fn sample_response_from_sources(
        &self,
        world_pos: Vec3,
        time: f32,
        sources: &[WindSource],
        response_curve: WindResponseCurve,
    ) -> f32 {
        response_curve.factor(self.sample_sources(world_pos, time, sources).length())
    }

    pub fn sample_response(
        &self,
        world_pos: Vec3,
        time: f32,
        response_curve: WindResponseCurve,
    ) -> f32 {
        let default_source = WindSource::new(0.0, 1.0, 0.335, 1.0);
        self.sample_response_from_sources(world_pos, time, &[default_source], response_curve)
    }

    fn sample_source_gust(
        &self,
        source_index: usize,
        sample_pos: Vec2,
        time_offset: Vec2,
        sharpness: f32,
    ) -> f32 {
        let offset = WIND_SOURCE_OFFSETS[source_index];
        let noise = &self.gust_noises[source_index];
        let gust_noise = noise.get_noise_2d(
            sample_pos.x + offset.x + time_offset.x,
            sample_pos.y + offset.y + time_offset.y,
        );
        let gust_value = (gust_noise * 0.5 + 0.5).clamp(0.0, 1.0);
        let center = (WIND_GUST_SMOOTH_MIN + WIND_GUST_SMOOTH_MAX) * 0.5;
        let half_width =
            (WIND_GUST_SMOOTH_MAX - WIND_GUST_SMOOTH_MIN) * 0.5 * (1.0 - sharpness.clamp(0.0, 1.0));
        wind_safe_smoothstep(center - half_width, center + half_width, gust_value)
    }
}
