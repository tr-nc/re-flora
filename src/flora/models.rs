//! Original flower geometry published from the same recipe as the HTML preview.
//! This module owns authored geometry, not planting, wind, lighting or GPU lifetime.
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::{ops::Range, sync::OnceLock};

mod column;
mod stem_experiment;
pub use column::{Column, MAX_BEND_FRACTION};
pub use stem_experiment::StemExperiment;

pub const MAX_SHAPE_SCALE: f32 = 4.;

// The published authoring catalog owns model order, display names, stem
// layers and count. Build-time validation generates these consts from flowers.json.
include!(concat!(env!("OUT_DIR"), "/flower_registry.rs"));
pub const MAX_HEADS: usize = 3;
pub const HEAD_PALETTE_SIZE: usize = 4;

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
            views: 256,
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
    /// Mean of the per-plant height multiplier, before integer layer rounding.
    pub height_scale: f32,
    pub height_variance: f32,
    pub voxel_scale: f32,
}
impl Default for Shape {
    fn default() -> Self {
        Self {
            head_scale: 1.,
            height_scale: 1.,
            height_variance: 0.,
            voxel_scale: 1.,
        }
    }
}
impl Shape {
    pub const MAX: Self = Self {
        head_scale: MAX_SHAPE_SCALE,
        height_scale: MAX_SHAPE_SCALE,
        height_variance: 1.,
        voxel_scale: 4.,
    };
    pub fn max_layers(self, base: u32) -> u32 {
        let s = self.normalized();
        (base as f32 * (s.height_scale + 3. * s.height_variance.sqrt()))
            .round()
            .max(1.) as u32
    }
    /// CPU oracle for the shader's rounded, bounded per-instance count.
    #[cfg(test)]
    pub fn layers_for_normal(self, base: u32, normal: f32) -> u32 {
        let s = self.normalized();
        (base as f32 * (s.height_scale + s.height_variance.sqrt() * normal))
            .round()
            .clamp(1., s.max_layers(base) as f32) as u32
    }
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
            height_variance: if self.height_variance.is_finite() {
                self.height_variance.clamp(0., 1.)
            } else {
                0.
            },
            voxel_scale: if self.voxel_scale.is_finite() {
                self.voxel_scale.clamp(0.2, Self::MAX.voxel_scale)
            } else {
                1.
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Triangle {
    pub positions: [Vec3; 3],
    pub normal: Vec3,
    pub color: [u8; 3],
    /// Shared head atlas coordinates; stems use zero UVs and their own colors.
    pub uvs: [Vec2; 3],
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
    pub cache_family: String,
    pub palette: [[u8; 3]; HEAD_PALETTE_SIZE],
    pub color_texture: ColorTexture,
    /// Outward head normal; the analytic stem stops at this attachment plane.
    pub socket_normal: Vec3,
    pub triangles: Vec<Triangle>,
    pub whole: Part,
    pub heads: Vec<Part>,
    pub stem_triangles: u32,
    pub column: Column,
    /// Base layer count, edge, multiplier mean, standard deviation. Immutable with source geometry.
    pub distribution: [f32; 4],
}

impl Flower {
    /// Build a bounded layer bank; each GPU instance selects its own prefix and
    /// transports the complete head. No per-instance surface baking.
    pub fn transformed(&self, shape: Shape) -> Self {
        let shape = shape.normalized();
        let mut result = self.clone();
        if shape == Shape::default() {
            return result;
        }
        result.column = self.column.with_shape(shape);
        result.distribution = [
            self.column.count() as f32,
            result.column.edge,
            shape.height_scale,
            shape.height_variance.sqrt(),
        ];
        result.triangles = result.column.triangles();
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
        result.whole.center = Vec3::new(0., result.column.tip().y * 0.5, 0.);
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

/// Resolved sRGB8 atlas generated by the same palette-weight decoder as the web.
#[derive(Clone, Debug, Deserialize)]
pub struct ColorTexture {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

#[derive(Deserialize)]
struct Published {
    flowers: Vec<PublishedFlower>,
}
#[derive(Deserialize)]
struct PublishedFlower {
    id: String,
    display_name: String,
    stem_layers: u32,
    cache_family: String,
    palette: [[u8; 3]; HEAD_PALETTE_SIZE],
    color_texture: ColorTexture,
    socket_normal: [f32; 3],
    heads: Vec<PublishedHead>,
    parts: Vec<PublishedPart>,
}
#[derive(Deserialize)]
struct PublishedHead {
    id: usize,
    anchor: [f32; 3],
}
#[derive(Deserialize)]
struct PublishedPart {
    head: usize,
    material: String,
    positions: Vec<f32>,
    uvs: Vec<f32>,
    indices: Vec<usize>,
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
        .enumerate()
        .map(|(model, source)| {
            let (id, name, layers) = FLOWER_REGISTRY[model];
            ensure!(
                source.id == id && source.display_name == name && source.stem_layers == layers,
                "flower catalog differs from published registry"
            );
            ensure!(source.heads.len() == 1, "flower head count");
            ensure!(
                source.heads.iter().enumerate().all(|(i, h)| i == h.id),
                "contiguous flower head IDs"
            );
            ensure!(
                source
                    .heads
                    .iter()
                    .all(|h| Vec3::from_array(h.anchor) == Vec3::ZERO),
                "attachment-local flower model"
            );
            let socket_normal = Vec3::from_array(source.socket_normal);
            ensure!(
                socket_normal.is_finite() && (socket_normal.length() - 1.).abs() < 1e-5,
                "invalid flower stem socket normal"
            );
            // Game-owned assembly: authored assets contain only attachment-local heads.
            let column = Column::for_layers(source.stem_layers)?;
            let tip = column.tip();
            let mut triangles = column.triangles();
            let stem_triangles = triangles.len() as u32;
            let mut ranges = vec![0..0; source.heads.len()];
            let palette = source.palette;
            let texture = &source.color_texture;
            ensure!(
                (32..=512).contains(&texture.width)
                    && (32..=512).contains(&texture.height)
                    && texture.rgb.len() == texture.width * texture.height * 3,
                "invalid flower color atlas"
            );
            for part in source.parts {
                ensure!(part.material == "palette", "shared palette flower material");
                ensure!(
                    part.uvs.len() == part.positions.len() / 3 * 2
                        && part
                            .uvs
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                    "invalid flower atlas coordinates"
                );
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
                ensure!(part.head < ranges.len(), "flower head index");
                if ranges[part.head].is_empty() {
                    ranges[part.head].start = triangles.len() as u32;
                }
                for index in part.indices.chunks_exact(3) {
                    let anchor = tip;
                    let positions = [index[0], index[1], index[2]]
                        .map(|i| Vec3::from_slice(&part.positions[i * 3..i * 3 + 3]) + tip);
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
                        color: palette[0],
                        uvs: [index[0], index[1], index[2]]
                            .map(|i| Vec2::from_slice(&part.uvs[i * 2..i * 2 + 2])),
                        anchor,
                    });
                }
                ranges[part.head].end = triangles.len() as u32;
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
                        anchor: Vec3::from_array(head.anchor) + tip,
                        center: (min + max) * 0.5,
                        radius: (max - min).length() * 0.53,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let center = Vec3::new(0., tip.y * 0.5, 0.);
            let radius = triangles
                .iter()
                .flat_map(|t| t.positions)
                .map(|p| p.distance(center))
                .fold(0f32, f32::max)
                * 1.06;
            Ok(Flower {
                id: source.id,
                cache_family: source.cache_family,
                palette,
                color_texture: source.color_texture,
                socket_normal,
                whole: Part {
                    triangles: 0..triangles.len() as u32,
                    anchor: Vec3::ZERO,
                    center,
                    radius,
                },
                triangles,
                heads,
                stem_triangles,
                distribution: [column.count() as f32, column.edge, 1., 0.],
                column,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_atlas_or_uv_data_is_rejected_before_gpu_upload() {
        let original = include_str!("../../assets/models/flowers.json");
        for field in ["texture", "uv", "material", "socket"] {
            let mut data: serde_json::Value = serde_json::from_str(original).unwrap();
            let flower = &mut data["flowers"][0];
            match field {
                "texture" => flower["color_texture"]["width"] = 0.into(),
                "uv" => flower["parts"][0]["uvs"][0] = 2.into(),
                "socket" => flower["socket_normal"][1] = 2.into(),
                _ => flower["parts"][0]["material"] = "petalColor".into(),
            }
            assert!(load(&data.to_string()).is_err(), "{field}");
        }
        assert_eq!(flowers()[1].id, "forget-me-not");
        assert_ne!(flowers()[1].cache_family, flowers()[3].cache_family);
    }

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
    fn authored_attachments_and_independent_transforms_cover_all_flowers() {
        let published: Published =
            serde_json::from_str(include_str!("../../assets/models/flowers.json")).unwrap();
        for (flower, authored) in flowers().iter().zip(&published.flowers) {
            for (head, published) in flower.heads.iter().zip(&authored.heads) {
                assert_eq!(
                    head.anchor,
                    Vec3::from_array(published.anchor) + flower.column.tip()
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
                        ..Shape::default()
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
                            assert_eq!(a.uvs, b.uvs, "shape transforms preserve authored UVs");
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
                height_scale: f32::INFINITY,
                height_variance: f32::NAN,
                voxel_scale: f32::INFINITY,
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
            assert_eq!(
                model.whole.center,
                Vec3::new(0., model.column.tip().y * 0.5, 0.)
            );
            assert_eq!(model.stem_triangles, model.column.count() * 12);
        }
    }
}
