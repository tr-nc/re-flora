use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::Vec2;
use rand::Rng; // For Rng trait and gen_range method

#[derive(Debug, Clone, Copy)]
pub struct FractalSettings {
    pub fractal_type: Option<FractalType>,
    pub octaves: i32,
    pub lacunarity: f32,
    pub gain: f32,
    pub weighted_strength: f32,
    pub ping_pong_strength: f32,
}

impl Default for FractalSettings {
    fn default() -> Self {
        FractalSettings {
            fractal_type: Some(FractalType::FBm),
            octaves: 3,
            lacunarity: 2.0,
            gain: 0.5,
            weighted_strength: 0.0,
            ping_pong_strength: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacerDesc {
    pub seed: u64,
    pub noise_type: NoiseType,
    pub frequency: f32,
    pub fractal_settings: Option<FractalSettings>,
    pub threshold: f32,
}

impl PlacerDesc {
    pub fn new(seed: u64) -> Self {
        PlacerDesc {
            seed,
            noise_type: NoiseType::OpenSimplex2,
            frequency: 0.02,
            fractal_settings: Some(FractalSettings::default()),
            threshold: 0.75,
        }
    }
}

const JITTER_MULTIPLIER: f32 = 0.9;

pub fn generate_positions(map_dimensions: Vec2, grid_size: f32, desc: &PlacerDesc) -> Vec<Vec2> {
    if grid_size <= 0.0 {
        // eprintln!("Error: grid_size must be positive."); // Minimal comments
        return Vec::new();
    }

    let mut noise = FastNoiseLite::with_seed(desc.seed as i32);

    // Using your specified FastNoiseLite setter pattern:
    noise.set_noise_type(Some(desc.noise_type));
    noise.set_frequency(Some(desc.frequency)); // Using Some() as per your last instruction

    if let Some(fractal) = &desc.fractal_settings {
        noise.set_fractal_type(fractal.fractal_type); // fractal.fractal_type is already Option
        noise.set_fractal_octaves(Some(fractal.octaves)); // Using Some()
        noise.set_fractal_lacunarity(Some(fractal.lacunarity)); // Using Some()
        noise.set_fractal_gain(Some(fractal.gain)); // Using Some()
        noise.set_fractal_weighted_strength(Some(fractal.weighted_strength)); // Using Some()
        if fractal.fractal_type == Some(FractalType::PingPong) {
            noise.set_fractal_ping_pong_strength(Some(fractal.ping_pong_strength));
            // Using Some()
        }
    }

    let mut positions = Vec::new();
    // Standard way to get a thread-local RNG
    let mut rng = rand::rng();

    let num_cells_x = (map_dimensions.x / grid_size).ceil() as u32;
    let num_cells_z = (map_dimensions.y / grid_size).ceil() as u32;

    for ix in 0..num_cells_x {
        for iz in 0..num_cells_z {
            let cell_origin_x = ix as f32 * grid_size;
            let cell_origin_z = iz as f32 * grid_size;

            let effective_cell_end_x = (cell_origin_x + grid_size).min(map_dimensions.x);
            let effective_cell_end_z = (cell_origin_z + grid_size).min(map_dimensions.y);

            let actual_cell_width = effective_cell_end_x - cell_origin_x;
            let actual_cell_depth = effective_cell_end_z - cell_origin_z;

            if actual_cell_width <= 0.0 || actual_cell_depth <= 0.0 {
                continue;
            }

            let noise_val = noise.get_noise_2d(cell_origin_x, cell_origin_z);
            let normalized_noise = (noise_val + 1.0) / 2.0;

            if normalized_noise > desc.threshold {
                let base_x = cell_origin_x + actual_cell_width / 2.0;
                let base_z = cell_origin_z + actual_cell_depth / 2.0;

                let half_jitter_span_x = actual_cell_width * JITTER_MULTIPLIER / 2.0;
                let half_jitter_span_z = actual_cell_depth * JITTER_MULTIPLIER / 2.0;

                // Standard Rng::gen_range for random numbers in a range
                let offset_x = rng.random_range(-half_jitter_span_x..half_jitter_span_x);
                let offset_z = rng.random_range(-half_jitter_span_z..half_jitter_span_z);

                let final_x = base_x + offset_x;
                let final_z = base_z + offset_z;

                if final_x >= 0.0
                    && final_x < map_dimensions.x
                    && final_z >= 0.0
                    && final_z < map_dimensions.y
                {
                    positions.push(Vec2::new(final_x, final_z));
                }
            }
        }
    }
    positions
}
