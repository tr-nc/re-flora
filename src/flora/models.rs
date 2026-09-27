//! Original flower geometry published from the same recipe as the HTML preview.
//! This module owns authored geometry, not planting, wind, lighting or GPU lifetime.
use anyhow::{ensure, Result};
use glam::Vec3;
use serde::Deserialize;
use std::{ops::Range, sync::OnceLock};

pub const MODEL_COUNT: usize = 8;
pub const MAX_HEADS: usize = 3;
/// Ten terrain voxels per authoring unit: roughly 20–28 voxels tall.
pub const WORLD_SCALE: f32 = 10.0 / 256.0;

#[derive(Clone, Debug)]
pub struct Triangle {
    pub positions: [Vec3; 3],
    pub normal: Vec3,
    pub color: [u8; 3],
}
#[derive(Clone, Debug)]
pub struct Part {
    pub triangles: Range<u32>,
    pub center: Vec3,
    pub radius: f32,
}
pub struct Flower {
    pub id: String,
    pub triangles: Vec<Triangle>,
    pub whole: Part,
    pub heads: Vec<Part>,
    pub stem_triangles: u32,
}

#[derive(Deserialize)]
struct Published {
    flowers: Vec<PublishedFlower>,
}
#[derive(Deserialize)]
struct PublishedFlower {
    id: String,
    root: [f32; 3],
    center: [f32; 3],
    span: f32,
    heads: Vec<PublishedHead>,
    parts: Vec<PublishedPart>,
}
#[derive(Deserialize)]
struct PublishedHead {
    id: usize,
}
#[derive(Deserialize)]
struct PublishedPart {
    head: Option<usize>,
    positions: Vec<f32>,
    indices: Vec<usize>,
    color: [u8; 3],
}

pub fn flowers() -> &'static [Flower] {
    static FLOWERS: OnceLock<Vec<Flower>> = OnceLock::new();
    FLOWERS.get_or_init(|| {
        load(include_str!("../../assets/models/flowers.json"))
            .expect("validated published flower meshes")
    })
}

fn load(json: &str) -> Result<Vec<Flower>> {
    let published: Published = serde_json::from_str(json)?;
    ensure!(published.flowers.len() == MODEL_COUNT, "flower bank size");
    published
        .flowers
        .into_iter()
        .map(|source| {
            ensure!(
                !source.heads.is_empty() && source.heads.len() <= MAX_HEADS,
                "flower head count"
            );
            ensure!(
                source.heads.iter().enumerate().all(|(i, h)| i == h.id),
                "contiguous flower head IDs"
            );
            let root = Vec3::from_array(source.root);
            let mut triangles = Vec::new();
            let mut ranges = vec![0..0; source.heads.len()];
            let mut stem_triangles = 0;
            let mut previous_head = None;
            for part in source.parts {
                ensure!(
                    part.positions.len().is_multiple_of(3) && part.indices.len().is_multiple_of(3),
                    "triangle attributes"
                );
                ensure!(
                    part.positions.iter().all(|p| p.is_finite()),
                    "finite flower vertices"
                );
                ensure!(
                    part.indices.iter().all(|&i| i < part.positions.len() / 3),
                    "flower index range"
                );
                ensure!(
                    part.head.is_none_or(|head| head < ranges.len()),
                    "flower head index"
                );
                if let Some(head) = part.head {
                    if previous_head != Some(head) {
                        ensure!(ranges[head].is_empty(), "head parts must be contiguous");
                        ranges[head].start = triangles.len() as u32;
                    }
                } else {
                    ensure!(previous_head.is_none(), "stems must precede heads");
                }
                for index in part.indices.chunks_exact(3) {
                    let positions = [index[0], index[1], index[2]]
                        .map(|i| Vec3::from_slice(&part.positions[i * 3..i * 3 + 3]) - root);
                    let normal = (positions[1] - positions[0])
                        .cross(positions[2] - positions[0])
                        .normalize_or_zero();
                    ensure!(
                        normal.length_squared() > 0.9,
                        "nondegenerate flower triangle"
                    );
                    triangles.push(Triangle {
                        positions,
                        normal,
                        color: part.color,
                    });
                }
                if let Some(head) = part.head {
                    ranges[head].end = triangles.len() as u32;
                } else {
                    stem_triangles = triangles.len() as u32;
                }
                previous_head = part.head;
            }
            let heads = ranges
                .into_iter()
                .map(|range| {
                    ensure!(!range.is_empty(), "empty flower head");
                    let mut min = Vec3::splat(f32::INFINITY);
                    let mut max = Vec3::splat(f32::NEG_INFINITY);
                    for point in triangles[range.start as usize..range.end as usize]
                        .iter()
                        .flat_map(|t| t.positions)
                    {
                        min = min.min(point);
                        max = max.max(point);
                    }
                    Ok(Part {
                        triangles: range,
                        center: (min + max) * 0.5,
                        radius: (max - min).length() * 0.53,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Flower {
                id: source.id,
                whole: Part {
                    triangles: 0..triangles.len() as u32,
                    center: Vec3::from_array(source.center) - root,
                    radius: source.span * 0.5,
                },
                triangles,
                heads,
                stem_triangles,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_flowers_keep_the_approved_shapes_and_complete_head_ranges() {
        let models = flowers();
        let counts = [528, 552, 380, 648, 412, 460, 352, 250];
        for (model, count) in models.iter().zip(counts) {
            assert_eq!(model.triangles.len(), count, "{}", model.id);
            assert!(model.stem_triangles > 0);
            let mut end = model.stem_triangles;
            for head in &model.heads {
                assert_eq!(head.triangles.start, end);
                end = head.triangles.end;
                for point in model.triangles[head.triangles.start as usize..end as usize]
                    .iter()
                    .flat_map(|t| t.positions)
                {
                    assert!(point.distance(head.center) < head.radius);
                }
            }
            assert_eq!(end, count as u32);
            assert!(model
                .triangles
                .iter()
                .flat_map(|t| t.positions)
                .all(|p| p.is_finite() && p.y >= -0.02));
            assert!(model.whole.center.distance(Vec3::new(0., 1.4, 0.)) < 1e-6);
        }
    }
}
