//! Opt-in soil evaporation checks through the real atlas and frame scheduler.
use crate::builder::{PlainBuilder, VOXEL_MOISTURE_MASK, VOXEL_TYPE_DIRT, VOXEL_TYPE_SAND};
use anyhow::{ensure, Result};
use glam::{UVec3, Vec3};

#[derive(serde::Serialize)]
struct Sample {
    frame: u32,
    speed: f32,
    mean: f64,
    histogram: [u32; 4],
}

pub(super) struct DryingReview {
    offset: UVec3,
    dim: UVec3,
    initial: Vec<u8>,
    paused: Option<Vec<u8>>,
    samples: Vec<Sample>,
}

fn is_soil(byte: u8) -> bool {
    matches!(u32::from(byte & 15), VOXEL_TYPE_DIRT | VOXEL_TYPE_SAND)
}

impl DryingReview {
    pub fn new(builder: &mut PlainBuilder, base: Vec3, initial_moisture: u8) -> Result<Self> {
        let center = (base * 256.0).round().as_uvec3();
        let offset = UVec3::new(center.x - 80, center.y - 1, center.z - 80);
        let dim = UVec3::new(161, 1, 161);
        let mut initial = builder.read_chunk_atlas_region(offset, dim)?;
        for byte in &mut initial {
            let moisture = if is_soil(*byte) { initial_moisture } else { 0 };
            *byte = (*byte & !VOXEL_MOISTURE_MASK) | (moisture << 4);
        }
        builder.write_chunk_atlas_region(offset, dim, &initial)?;
        Ok(Self {
            offset,
            dim,
            initial,
            paused: None,
            samples: Vec::new(),
        })
    }

    pub fn checkpoint(
        &mut self,
        builder: &mut PlainBuilder,
        mode: &str,
        frame: u32,
        speed: f32,
    ) -> Result<()> {
        if !matches!(frame, 60 | 120 | 180) {
            return Ok(());
        }
        let data = builder.read_chunk_atlas_region(self.offset, self.dim)?;
        let mut histogram = [0u32; 4];
        for (&byte, &initial) in data.iter().zip(&self.initial) {
            ensure!(
                byte & !VOXEL_MOISTURE_MASK == initial & !VOXEL_MOISTURE_MASK,
                "drying modified voxel type or other state bits"
            );
            if is_soil(byte) {
                histogram[usize::from((byte >> 4) & 3)] += 1;
            } else {
                ensure!(
                    byte & VOXEL_MOISTURE_MASK == 0,
                    "drying modified non-soil moisture"
                );
            }
        }
        if mode == "dry-zero" {
            ensure!(data == self.initial, "zero drying speed changed moisture");
        }
        if mode == "dry-controls" {
            if frame == 60 {
                self.paused = Some(data.clone());
            }
            if frame == 120 {
                ensure!(
                    Some(&data) == self.paused.as_ref(),
                    "runtime pause did not freeze evaporation"
                );
            }
        }
        let count = histogram.iter().sum::<u32>();
        ensure!(count > 1000, "drying needs a real soil receiver population");
        let total = histogram
            .iter()
            .enumerate()
            .map(|(i, n)| i as u32 * n)
            .sum::<u32>();
        let mean = f64::from(total) / f64::from(count);
        log::info!("[SOIL_DRYING_REVIEW] mode={mode} frame={frame} speed={speed:.2} receivers={count} mean={mean:.6} histogram={histogram:?} saved=false");
        self.samples.push(Sample {
            frame,
            speed,
            mean,
            histogram,
        });
        if frame == 180 {
            if mode == "dry-watering" {
                ensure!(mean > 0.0, "paused evaporation disabled irrigation");
            } else if mode != "dry-zero" {
                ensure!(mean < 3.0, "drying produced no real moisture loss");
            }
            if let Ok(path) = std::env::var("RE_FLORA_SOIL_DRY_STATS") {
                std::fs::write(
                    path,
                    serde_json::to_vec(&serde_json::json!({
                        "mode":mode,"samples":self.samples,"receivers":count,
                        "offset":self.offset.to_array(),"dimensions":self.dim.to_array(),"voxels":data
                    }))?,
                )?;
            }
        }
        Ok(())
    }
}
