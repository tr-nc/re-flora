#![allow(dead_code)]

use glam::Vec2;
use noise::{Fbm, NoiseFn, OpenSimplex, Perlin, Seedable};
use rand::{rng, Rng};

/// The base algorithm for generating noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseType {
    OpenSimplex,
    Perlin,
}

/// Settings for configuring FBM (Fractal Brownian Motion) noise.
///
/// FBM adds detail by combining multiple versions of a base noise source
/// at different frequencies and amplitudes.
#[derive(Debug, Clone, Copy)]
pub struct FractalSettings {
    /// The number of noise functions to combine. Higher numbers add more detail.
    pub octaves: usize,
    /// The frequency of the noise. Higher values produce more frequent features.
    pub frequency: f64,
    /// A multiplier that determines how quickly the frequency increases for each successive octave.
    pub lacunarity: f64,
    /// A multiplier that determines how quickly the amplitude diminishes for each successive octave.
    pub persistence: f64,
}

impl Default for FractalSettings {
    fn default() -> Self {
        FractalSettings {
            octaves: 3,
            frequency: 0.02,
            lacunarity: 2.0,
            persistence: 0.5,
        }
    }
}

/// A descriptor that defines the complete noise configuration for object placement.
#[derive(Debug, Clone)]
pub struct PlacerDesc {
    /// The seed for the random number generator.
    pub seed: u32,
    pub noise_type: NoiseType,
    /// Frequency for non-fractal noise. For fractal noise, frequency is set in `FractalSettings`.
    pub frequency: f64,
    /// If `Some`, FBM fractal noise will be used. If `None`, simple noise will be used.
    pub fractal_settings: Option<FractalSettings>,
    /// Noise values above this threshold (0.0 to 1.0) will result in placing an object.
    pub threshold: f64,
}

impl PlacerDesc {
    pub fn new(seed: u32) -> Self {
        PlacerDesc {
            seed,
            noise_type: NoiseType::OpenSimplex,
            frequency: 0.02,
            fractal_settings: Some(FractalSettings::default()),
            threshold: 0.75,
        }
    }
}

/// Constructs a boxed, dynamically-dispatchable noise function based on the descriptor.
fn build_noise_function(desc: &PlacerDesc) -> Box<dyn NoiseFn<f64, 2>> {
    match desc.noise_type {
        NoiseType::OpenSimplex => build_specific_noise_fn::<OpenSimplex>(desc),
        NoiseType::Perlin => build_specific_noise_fn::<Perlin>(desc),
    }
}

/// Generic helper to construct a noise pipeline for a specific noise source type `T`.
fn build_specific_noise_fn<T>(desc: &PlacerDesc) -> Box<dyn NoiseFn<f64, 2>>
where
    T: Default + Seedable + NoiseFn<f64, 2> + Send + Sync + 'static,
    Fbm<T>: NoiseFn<f64, 2>,
{
    let seed = desc.seed;

    if let Some(fractal) = desc.fractal_settings {
        // if fractal settings are provided, configure and return an Fbm noise function.
        let mut noise = Fbm::<T>::new(seed);
        noise.octaves = fractal.octaves;
        noise.frequency = fractal.frequency;
        noise.lacunarity = fractal.lacunarity;
        noise.persistence = fractal.persistence;
        Box::new(noise)
    } else {
        Box::new(Perlin::new(seed))
    }
}

const JITTER_MULTIPLIER: f32 = 0.9;

/// Generates a list of 2D positions based on procedural noise.
///
/// The function divides the map into a grid. For each grid cell, it samples a noise
/// value. If the value is above a threshold, it places an object at a
/// randomly jittered position within that cell.
pub fn generate_positions(
    map_dimensions: Vec2,
    map_offset: Vec2,
    grid_size: f32,
    desc: &PlacerDesc,
) -> Vec<Vec2> {
    if grid_size <= 0.0 {
        return Vec::new();
    }

    let noise_fn = build_noise_function(desc);
    let mut positions = Vec::new();
    let mut rng = rng();

    let num_cells_x = (map_dimensions.x / grid_size).ceil() as u32;
    let num_cells_y = (map_dimensions.y / grid_size).ceil() as u32;

    for ix in 0..num_cells_x {
        for iy in 0..num_cells_y {
            let cell_origin_x = ix as f32 * grid_size;
            let cell_origin_y = iy as f32 * grid_size;

            // get the 2D noise value, casting coordinates to f64 for the noise library.
            let noise_val = noise_fn.get([cell_origin_x as f64, cell_origin_y as f64]);

            // normalize the noise value from [-1.0, 1.0] to [0.0, 1.0].
            let normalized_noise = (noise_val + 1.0) / 2.0;

            if normalized_noise > desc.threshold {
                // determine the actual cell dimensions, clamping to map boundaries.
                let effective_cell_end_x = (cell_origin_x + grid_size).min(map_dimensions.x);
                let effective_cell_end_y = (cell_origin_y + grid_size).min(map_dimensions.y);
                let actual_cell_width = effective_cell_end_x - cell_origin_x;
                let actual_cell_height = effective_cell_end_y - cell_origin_y;

                // calculate the center of the cell.
                let base_x = cell_origin_x + actual_cell_width / 2.0;
                let base_y = cell_origin_y + actual_cell_height / 2.0;

                // calculate a random offset (jitter) within the cell.
                let half_jitter_span_x = actual_cell_width * JITTER_MULTIPLIER / 2.0;
                let half_jitter_span_y = actual_cell_height * JITTER_MULTIPLIER / 2.0;

                let offset_x = rng.random_range(-half_jitter_span_x..half_jitter_span_x);
                let offset_y = rng.random_range(-half_jitter_span_y..half_jitter_span_y);

                let final_x = base_x + offset_x;
                let final_y = base_y + offset_y;

                // ensure the final position is within map bounds before adding it.
                if final_x >= 0.0
                    && final_x < map_dimensions.x
                    && final_y >= 0.0
                    && final_y < map_dimensions.y
                {
                    positions.push(Vec2::new(final_x, final_y));
                }
            }
        }
    }
    positions.iter_mut().for_each(|p| *p += map_offset);
    positions
}
