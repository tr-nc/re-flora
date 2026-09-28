//! Original flower geometry published from the same recipe as the HTML preview.
//! This module owns authored geometry, not planting, wind, lighting or GPU lifetime.
use anyhow::{ensure, Result};
use glam::Vec3;
use serde::Deserialize;
use std::{ops::Range, sync::OnceLock};

mod column;
pub use column::{Column, MAX_BEND_FRACTION};

pub const MAX_SHAPE_SCALE: f32 = 4.;

pub const MODEL_COUNT: usize = 8;
pub const MAX_HEADS: usize = 3;
/// Ten terrain voxels per authoring unit: roughly 20–28 voxels tall.
pub const WORLD_SCALE: f32 = 10.0 / 256.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub resolution: u32,
    pub views: u32,
    pub shape: Shape,
    /// Legacy saved overall scale: still multiplies the entire plant at runtime.
    pub size_scale: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            resolution: 32,
            views: 16,
            shape: Shape::default(),
            size_scale: 1.0,
        }
    }
}
impl Settings {
    pub fn normalized(self) -> Self {
        Self {
            resolution: self.resolution.clamp(8, 64),
            views: self.views.clamp(8, 512),
            shape: self.shape.normalized(),
            size_scale: if self.size_scale.is_finite() {
                self.size_scale.clamp(0.5, 2.0)
            } else {
                1.0
            },
            ..self
        }
    }
}

/// Authored-space controls only. Overall size/growth and per-attachment wind
/// transport never invalidate geometry. A head keeps its shape when its
/// attachment moves vertically; its complete authored range scales about that
/// attachment, including calyx and center (not just petals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub head_scale: f32,
    pub height_scale: f32,
}
impl Default for Shape {
    fn default() -> Self {
        Self {
            head_scale: 1.,
            height_scale: 1.,
        }
    }
}
impl Shape {
    pub fn normalized(self) -> Self {
        let scale = |v: f32| {
            if v.is_finite() {
                v.clamp(0.25, MAX_SHAPE_SCALE)
            } else {
                1.
            }
        };
        Self {
            head_scale: scale(self.head_scale),
            height_scale: scale(self.height_scale),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Triangle {
    pub positions: [Vec3; 3],
    pub normal: Vec3,
    pub color: [u8; 3],
    /// Rigid cell/leaf/head attachment; wind translates it without tilting cells.
    pub anchor: Vec3,
}
#[derive(Clone, Debug)]
pub struct Part {
    pub triangles: Range<u32>,
    /// Root-relative authored attachment, not the center of its bounding sphere.
    pub anchor: Vec3,
    pub center: Vec3,
    pub radius: f32,
}
#[derive(Clone)]
pub struct Flower {
    pub id: String,
    pub triangles: Vec<Triangle>,
    pub whole: Part,
    pub heads: Vec<Part>,
    pub stem_triangles: u32,
    pub stem_voxel_triangles: u32,
    pub column: Column,
}

impl Flower {
    /// Rebuild complete voxel layers for height edits; transport leaves and the
    /// complete terminal head with their attachments. Never runs per instance.
    pub fn transformed(&self, shape: Shape) -> Self {
        let shape = shape.normalized();
        let mut result = self.clone();
        if shape == Shape::default() {
            return result;
        }
        result.column = self.column.scaled_height(shape.height_scale);
        result.triangles = result.column.triangles();
        result.stem_voxel_triangles = result.triangles.len() as u32;
        for old in &self.triangles[self.stem_voxel_triangles as usize..self.stem_triangles as usize]
        {
            let mut triangle = old.clone();
            triangle.anchor = result.column.attachment(self.column, old.anchor);
            triangle.positions = old.positions.map(|p| p + triangle.anchor - old.anchor);
            result.triangles.push(triangle);
        }
        result.stem_triangles = result.triangles.len() as u32;
        for (authored, part) in self.heads.iter().zip(&mut result.heads) {
            part.triangles.start = result.triangles.len() as u32;
            part.anchor = result.column.tip();
            for old in
                &self.triangles[authored.triangles.start as usize..authored.triangles.end as usize]
            {
                let mut triangle = old.clone();
                triangle.anchor = part.anchor;
                triangle.positions = old
                    .positions
                    .map(|p| part.anchor + (p - authored.anchor) * shape.head_scale);
                result.triangles.push(triangle);
            }
            part.triangles.end = result.triangles.len() as u32;
            part.center = part.anchor + (authored.center - authored.anchor) * shape.head_scale;
            part.radius = authored.radius * shape.head_scale;
        }
        // Refit whole-plant culling around the transported authored center.
        // Height now advances in complete-cell increments, while head scale
        // remains continuous and does not change the selected pixel budget.
        result.whole.triangles = 0..result.triangles.len() as u32;
        result.whole.center = self.whole.center * Vec3::new(1., shape.height_scale, 1.);
        let vertex_radius = |triangles: &[Triangle], center: Vec3| {
            triangles
                .iter()
                .flat_map(|t| t.positions)
                .map(|p| p.distance(center))
                .fold(0., f32::max)
        };
        result.whole.radius = self.whole.radius
            * (vertex_radius(&result.triangles, result.whole.center)
                / vertex_radius(&self.triangles, self.whole.center));
        result
    }
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
    column: PublishedColumn,
}
#[derive(Deserialize)]
struct PublishedColumn {
    #[serde(rename = "cellSize")]
    edge: f32,
    tip: [f32; 3],
}
#[derive(Deserialize)]
struct PublishedHead {
    id: usize,
    anchor: [f32; 3],
}
#[derive(Deserialize)]
struct PublishedPart {
    head: Option<usize>,
    material: String,
    anchors: Vec<[f32; 3]>,
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
            ensure!(source.heads.len() == 1, "flower head count");
            ensure!(
                source.heads.iter().enumerate().all(|(i, h)| i == h.id),
                "contiguous flower head IDs"
            );
            let root = Vec3::from_array(source.root);
            ensure!(
                root.is_finite()
                    && source
                        .heads
                        .iter()
                        .all(|h| Vec3::from_array(h.anchor).is_finite()),
                "finite flower attachments"
            );
            let tip = Vec3::from_array(source.column.tip) - root;
            ensure!(
                source.column.edge.is_finite()
                    && source.column.edge > 0.
                    && tip.is_finite()
                    && tip.y > 0.,
                "valid stem column"
            );
            let mut column = Column {
                edge: source.column.edge,
                height: tip.y,
                bend: tip.x,
                color: [0; 3],
            };
            let mut stem_voxel_triangles = 0;
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
                ensure!(
                    part.anchors.len() == part.indices.len() / 3,
                    "triangle attachment count"
                );
                for (index, anchor) in part.indices.chunks_exact(3).zip(&part.anchors) {
                    let anchor = Vec3::from_array(*anchor) - root;
                    ensure!(anchor.is_finite(), "finite triangle attachment");
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
                        anchor,
                    });
                }
                if let Some(head) = part.head {
                    ranges[head].end = triangles.len() as u32;
                } else {
                    stem_triangles = triangles.len() as u32;
                }
                if part.head.is_none() && part.material == "stemColor" {
                    ensure!(stem_voxel_triangles == 0, "one unbranched voxel stem");
                    stem_voxel_triangles = triangles.len() as u32;
                    column.color = part.color;
                }
                previous_head = part.head;
            }
            let heads = ranges
                .into_iter()
                .zip(&source.heads)
                .map(|(range, head)| {
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
                        anchor: Vec3::from_array(head.anchor) - root,
                        center: (min + max) * 0.5,
                        radius: (max - min).length() * 0.53,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Flower {
                id: source.id,
                whole: Part {
                    triangles: 0..triangles.len() as u32,
                    anchor: Vec3::ZERO,
                    center: Vec3::from_array(source.center) - root,
                    radius: source.span * 0.5,
                },
                triangles,
                heads,
                stem_triangles,
                stem_voxel_triangles,
                column,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_bound_gpu_work_and_reject_nonfinite_scale() {
        let settings = Settings {
            resolution: 0,
            size_scale: f32::NAN,
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.resolution, 8);
        assert_eq!(settings.size_scale, 1.0);
        assert_eq!(
            Settings {
                resolution: u32::MAX,
                size_scale: 10.0,
                ..settings
            }
            .normalized(),
            Settings {
                resolution: 64,
                size_scale: 2.0,
                ..settings
            }
        );
    }

    #[test]
    fn head_size_preserves_continuous_framing_around_saved_defaults() {
        for flower in flowers() {
            for value in [0.9999f32, 1.0001] {
                for shape in [Shape {
                    head_scale: value,
                    ..Shape::default()
                }] {
                    let changed = flower.transformed(shape);
                    for (before, after) in std::iter::once(&flower.whole)
                        .chain(&flower.heads)
                        .zip(std::iter::once(&changed.whole).chain(&changed.heads))
                    {
                        let tolerance =
                            (before.center.length() + before.radius) * (value - 1.).abs() * 4.
                                + 1e-5;
                        assert!(
                            before.center.distance(after.center) < tolerance,
                            "{}: tiny {shape:?} edit jumped the framing center",
                            flower.id
                        );
                        assert!(
                            (before.radius - after.radius).abs() < tolerance,
                            "{}: tiny {shape:?} edit jumped the pixel scale",
                            flower.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn authored_attachments_and_independent_transforms_cover_all_eight_flowers() {
        let published: Published =
            serde_json::from_str(include_str!("../../assets/models/flowers.json")).unwrap();
        for (flower, authored) in flowers().iter().zip(&published.flowers) {
            for (head, published) in flower.heads.iter().zip(&authored.heads) {
                assert_eq!(
                    head.anchor,
                    Vec3::from_array(published.anchor) - Vec3::from_array(authored.root)
                );
            }
            let unchanged = flower.transformed(Shape::default());
            assert_eq!(unchanged.whole.center, flower.whole.center);
            assert_eq!(unchanged.whole.radius, flower.whole.radius);
            for head_scale in [0.25, 1., 2., 4.] {
                for height_scale in [0.25, 1., 2., 4.] {
                    let shape = Shape {
                        head_scale,
                        height_scale,
                    };
                    let transformed = flower.transformed(shape);
                    for (index, new) in transformed.triangles.iter().enumerate() {
                        let geometric_normal = (new.positions[1] - new.positions[0])
                            .cross(new.positions[2] - new.positions[0])
                            .normalize();
                        assert!(
                            geometric_normal.dot(new.normal) > 0.9999,
                            "{} normal {index}",
                            flower.id
                        );
                    }
                    assert_eq!(transformed.heads.len(), 1);
                    assert_eq!(transformed.column.edge, flower.column.edge);
                    for (old, new) in flower.heads.iter().zip(&transformed.heads) {
                        assert_eq!(new.triangles.len(), old.triangles.len());
                        assert!(new.anchor.distance(transformed.column.tip()) < 1e-6);
                        // Height moves the entire head without stretching its offsets.
                        for (a, b) in flower.triangles
                            [old.triangles.start as usize..old.triangles.end as usize]
                            .iter()
                            .zip(
                                &transformed.triangles
                                    [new.triangles.start as usize..new.triangles.end as usize],
                            )
                        {
                            for (p, q) in a.positions.into_iter().zip(b.positions) {
                                assert!(
                                    ((q - new.anchor) - (p - old.anchor) * head_scale).length()
                                        < 3e-6
                                );
                            }
                        }
                    }
                    for part in std::iter::once(&transformed.whole).chain(&transformed.heads) {
                        assert!(part.radius.is_finite() && part.radius > 0.);
                        assert!(transformed.triangles
                            [part.triangles.start as usize..part.triangles.end as usize]
                            .iter()
                            .flat_map(|t| t.positions)
                            .all(|p| p.distance(part.center) < part.radius));
                    }
                }
            }
        }
        assert_eq!(
            Shape {
                head_scale: f32::NAN,
                height_scale: f32::INFINITY
            }
            .normalized(),
            Shape::default()
        );
    }

    #[test]
    fn shared_flowers_keep_the_approved_shapes_and_complete_head_ranges() {
        let models = flowers();
        for model in models {
            let count = model.triangles.len();
            assert!(count > 100 && count < 4000, "{}", model.id);
            assert_eq!(model.heads.len(), 1);
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
                .all(|p| p.is_finite()
                    && p.y >= -0.02
                    && p.distance(model.whole.center) < model.whole.radius));
            assert!(model.whole.center.distance(Vec3::new(0., 1.4, 0.)) < 1e-6);
        }
    }
}
