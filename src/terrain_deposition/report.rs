//! Explicit artifact exporter for the HTML review; not part of ordinary tests/perf claims.
use super::*;
use serde_json::{json, Value};
use std::collections::VecDeque;

const DIM: UVec3 = UVec3::new(48, 48, 48);
const FLOOR: u32 = 2;
fn idx(x: u32, y: u32, z: u32) -> usize {
    ((z * DIM.y + y) * DIM.x + x) as usize
}

pub(super) fn layer_holes(heights: &[u32], dim: UVec3, layer: u32) -> usize {
    let width = dim.x as usize;
    let depth = dim.z as usize;
    let mut seen = vec![false; heights.len()];
    let mut queue = VecDeque::new();
    for z in 0..depth {
        for x in 0..width {
            let i = z * width + x;
            if (x == 0 || z == 0 || x + 1 == width || z + 1 == depth)
                && heights[i] < layer
                && !seen[i]
            {
                seen[i] = true;
                queue.push_back((x, z));
            }
        }
    }
    while let Some((x, z)) = queue.pop_front() {
        for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as i32 + dx;
            let nz = z as i32 + dz;
            if nx < 0 || nz < 0 || nx >= width as i32 || nz >= depth as i32 {
                continue;
            }
            let i = nz as usize * width + nx as usize;
            if heights[i] < layer && !seen[i] {
                seen[i] = true;
                queue.push_back((nx as usize, nz as usize));
            }
        }
    }
    seen.iter()
        .zip(heights)
        .filter(|(s, h)| !**s && **h < layer)
        .count()
}

fn sample(
    name: &str,
    radius: f32,
    clump_radius: u32,
    slope: f32,
    dose: u32,
    batches: u32,
) -> Value {
    let mut data = vec![0; (DIM.x * DIM.y * DIM.z) as usize];
    for z in 0..DIM.z {
        for x in 0..DIM.x {
            data[idx(x, 1, z)] = 7;
        }
    }
    let mut request = DepositionRequest {
        start: Vec3::new(24., 2., 24.),
        end: Vec3::new(24., 2., 24.),
        radius,
        volume: dose,
        clump_radius,
        repose_slope: slope,
        seed: 1,
    };
    let mut added = 0;
    for batch in 1..=batches {
        request.seed = if dose == 1 {
            batch.wrapping_mul(0x9e3779b9)
        } else {
            batch
        };
        added += deposit(&mut data, UVec3::ZERO, DIM, None, request).added;
    }
    let heights: Vec<u32> = (0..DIM.z)
        .flat_map(|z| {
            let data = &data;
            (0..DIM.x).map(move |x| {
                (FLOOR..DIM.y)
                    .rev()
                    .find(|&y| data[idx(x, y, z)] & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8)
                    .map_or(0, |y| y + 1 - FLOOR)
            })
        })
        .collect();
    let top = heights.iter().copied().max().unwrap();
    let interior_gaps = heights.iter().sum::<u32>() - added;
    let mut spikes = 0;
    for z in 0..DIM.z {
        for x in 0..DIM.x {
            let h = heights[(z * DIM.x + x) as usize];
            if h < 2 {
                continue;
            }
            let sx = (x > 0 && heights[(z * DIM.x + x - 1) as usize] >= h - 1)
                || (x + 1 < DIM.x && heights[(z * DIM.x + x + 1) as usize] >= h - 1);
            let sz = (z > 0 && heights[((z - 1) * DIM.x + x) as usize] >= h - 1)
                || (z + 1 < DIM.z && heights[((z + 1) * DIM.x + x) as usize] >= h - 1);
            spikes += u32::from(!sx || !sz);
        }
    }
    let sections: Vec<Value> = [1 + (top - 1) / 3, 1 + (top - 1) / 2]
        .into_iter()
        .map(|layer| {
            let widths: Vec<f32> = (0..16)
                .map(|direction| {
                    let angle = direction as f32 * std::f32::consts::PI / 16.;
                    let (sin, cos) = angle.sin_cos();
                    let mut min = f32::INFINITY;
                    let mut max = f32::NEG_INFINITY;
                    for z in 0..DIM.z {
                        for x in 0..DIM.x {
                            if heights[(z * DIM.x + x) as usize] >= layer {
                                let p = x as f32 * cos + z as f32 * sin;
                                min = min.min(p);
                                max = max.max(p);
                            }
                        }
                    }
                    max - min
                })
                .collect();
            let min = widths.iter().copied().fold(f32::INFINITY, f32::min);
            let max = widths.iter().copied().fold(0., f32::max);
            json!({ "layer": layer, "min_width": min, "max_width": max, "anisotropy": max / min })
        })
        .collect();
    let holes: Vec<usize> = (1..=top)
        .map(|layer| layer_holes(&heights, DIM, layer))
        .collect();
    assert_eq!(added, dose * batches);
    assert_eq!(interior_gaps, 0);
    assert_eq!(spikes, 0);
    assert!(
        holes.iter().all(|&h| h == 0),
        "soil layers contain interior holes"
    );
    json!({ "name": name, "dim": DIM.to_array(), "floor": FLOOR, "radius": radius, "clump_radius": clump_radius, "slope": slope, "dose": dose, "batches": batches, "added": added, "heights": heights, "peak_height": top, "interior_gaps": interior_gaps, "unsupported_spikes": spikes, "layer_holes": holes, "sections": sections })
}

#[test]
#[ignore = "writes deterministic voxel artifacts for a report; run explicitly"]
fn export_soil_report_samples() {
    let samples = vec![
        sample("precise_stationary", 0., 0, 1., 2048, 8),
        sample("same_input_as_old_shape_repro", 2., 1, 1., 2048, 8),
        sample("scattered_individual_steep", 8., 0, 2., 1, 4096),
    ];
    for s in &samples {
        eprintln!(
            "[SOIL_REPORT] {} added={} peak={} interior_gaps={} spikes={} layer_holes={}",
            s["name"],
            s["added"],
            s["peak_height"],
            s["interior_gaps"],
            s["unsupported_spikes"],
            s["layer_holes"]
        );
    }
    std::fs::create_dir_all("target").unwrap();
    std::fs::write(
        "target/soil-deposition-report.json",
        serde_json::to_string(&samples).unwrap(),
    )
    .unwrap();
}
