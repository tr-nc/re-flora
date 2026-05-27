use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindSource {
    pub direction_degrees: f32,
    pub speed: f32,
    pub pattern_scale: f32,
    pub pattern_frequency: f32,
    pub octaves: u32,
    pub lacunarity: f32,
    pub gain: f32,
}

impl WindSource {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        direction_degrees: f32,
        speed: f32,
        pattern_scale: f32,
        pattern_frequency: f32,
        octaves: u32,
        lacunarity: f32,
        gain: f32,
    ) -> Self {
        Self {
            direction_degrees,
            speed,
            pattern_scale,
            pattern_frequency,
            octaves,
            lacunarity,
            gain,
        }
    }
}

impl Default for WindSource {
    fn default() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0, 3, 2.0, 1.0)
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

const WIND_FBM_SEED: i32 = 3181;
const WIND_FBM_FREQUENCY: f32 = 0.008;
const WIND_SAMPLE_SCALE: f32 = 256.0;
const WIND_TIME_SCALE: f32 = 170.0;
const WIND_FBM_PERSISTENCE: f32 = 0.5;

fn wind_noise_state(seed: i32) -> FastNoiseLite {
    let mut state = FastNoiseLite::with_seed(seed);
    state.set_noise_type(Some(NoiseType::OpenSimplex2));
    state.set_fractal_type(Some(FractalType::None));
    state.set_frequency(Some(1.0));
    state
}

fn wind_source_seed(source_index: usize) -> i32 {
    WIND_FBM_SEED.wrapping_add((source_index as i32).wrapping_mul(997))
}

fn wind_source_offset(source_index: usize) -> Vec2 {
    let seed = source_index as u32;
    let x = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let y = seed.wrapping_mul(22_695_477).wrapping_add(1_109_515_789);
    Vec2::new(((x & 1023) as f32) - 512.0, ((y & 1023) as f32) - 512.0)
}

pub struct Wind;

impl Default for Wind {
    fn default() -> Self {
        Self::new()
    }
}

impl Wind {
    pub fn new() -> Self {
        Self
    }

    pub fn sample_sources(&self, world_pos: Vec3, time: f32, sources: &[WindSource]) -> Vec3 {
        let sample_pos = Vec2::new(world_pos.x, world_pos.z) * WIND_SAMPLE_SCALE;
        let scroll_time = time * WIND_TIME_SCALE;
        let mut wind_planar = Vec2::ZERO;

        for (source_index, source) in sources.iter().enumerate() {
            let source_speed = source.speed.max(0.0);
            let source_gain = source.gain.max(0.0);
            if source_gain <= f32::EPSILON {
                continue;
            }

            let angle = source.direction_degrees.to_radians();
            let wind_direction = Vec2::new(angle.cos(), angle.sin());
            let time_offset = -wind_direction * scroll_time * source_speed;
            let wind_factor = self.sample_source_fbm(source_index, sample_pos, time_offset, source);
            wind_planar += wind_direction * wind_factor;
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
        let default_source = WindSource::new(0.0, 1.0, 1.0, 1.0, 3, 2.0, 1.0);
        self.sample_response_from_sources(world_pos, time, &[default_source], response_curve)
    }

    fn sample_source_fbm(
        &self,
        source_index: usize,
        sample_pos: Vec2,
        time_offset: Vec2,
        source: &WindSource,
    ) -> f32 {
        let offset = wind_source_offset(source_index);
        let noise = wind_noise_state(wind_source_seed(source_index));
        let pattern_scale = source.pattern_scale.max(0.05);
        let mut frequency = WIND_FBM_FREQUENCY * source.pattern_frequency.max(0.05);
        let lacunarity = source.lacunarity.max(1.0);
        let gain = source.gain.max(0.0);
        let octaves = source.octaves.clamp(1, 8);
        let p = (sample_pos + offset + time_offset) / pattern_scale;
        let mut amplitude = 1.0;
        let mut noise_sum = 0.0;

        for _ in 0..octaves {
            noise_sum += noise.get_noise_2d(p.x * frequency, p.y * frequency) * amplitude;
            frequency *= lacunarity;
            amplitude *= WIND_FBM_PERSISTENCE;
        }

        noise_sum * gain
    }
}
