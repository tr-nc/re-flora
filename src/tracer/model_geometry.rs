//! Immutable native triangles shared by model draws, with no sampled surface
//! cache, view bins, tiles or per-model depth reconstruction.
use crate::{
    flora::models::{self, Shape},
    model_assets,
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Vec2, Vec3};
use std::sync::LazyLock;

pub const ANIMATION_FRAMES: u32 = 32;
pub const BUTTERFLY_SOURCE_BASE: u32 = 65;
pub const FLOWER_SOURCE_BASE: u32 = BUTTERFLY_SOURCE_BASE + ANIMATION_FRAMES;
fn representatives() -> &'static [usize] {
    static IDS: LazyLock<Vec<usize>> = LazyLock::new(|| {
        models::flowers()
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                (!models::flowers()[..i]
                    .iter()
                    .any(|p| p.cache_family == f.cache_family))
                .then_some(i)
            })
            .collect()
    });
    &IDS
}
fn flower_source(model: usize, part: usize) -> u32 {
    let flowers = models::flowers();
    let representative = representatives()
        .iter()
        .position(|&i| flowers[i].cache_family == flowers[model].cache_family)
        .unwrap();
    FLOWER_SOURCE_BASE
        + representatives()[..representative]
            .iter()
            .map(|&i| 1 + flowers[i].heads.len() as u32)
            .sum::<u32>()
        + part as u32
}
pub fn animation_frame(phase: f32) -> u32 {
    ((phase.rem_euclid(1.) * ANIMATION_FRAMES as f32).round() as u32) % ANIMATION_FRAMES
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Triangle {
    pub a: [f32; 4],
    pub e1: [f32; 4],
    pub e2: [f32; 4],
    pub normals: [[f32; 4]; 3],
    pub uv01: [f32; 4],
    pub uv2: [f32; 4],
}
fn triangle(p: [Vec3; 3], normals: [Vec3; 3], uv: [Vec2; 3], material: u32) -> Triangle {
    Triangle {
        a: p[0].extend(0.).to_array(),
        e1: (p[1] - p[0]).extend(0.).to_array(),
        e2: (p[2] - p[0]).extend(0.).to_array(),
        normals: normals.map(|n| n.extend(0.).to_array()),
        uv01: [uv[0].x, uv[0].y, uv[1].x, uv[1].y],
        uv2: [uv[2].x, uv[2].y, material as f32, 0.],
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct FlowerPart {
    pub range: [u32; 4],
    center_radius: [f32; 4],
    stem: [f32; 4],
    distribution: [f32; 4],
    socket: [f32; 4],
}
pub(super) struct Source {
    pub triangles: Vec<Triangle>,
    pub ranges: Vec<[u32; 4]>,
    pub palette: Vec<[f32; 4]>,
    pub flower_parts: Vec<FlowerPart>,
    flower_root_radius: f32,
    flower_spawn_height: f32,
}
impl Source {
    pub fn flower_culling_padding(&self, scale: f32, overshoot: f32) -> (Vec3, Vec3) {
        let radius = self.flower_root_radius * scale;
        (
            Vec3::splat(radius) + Vec3::Y * (self.flower_spawn_height * scale + 1. / 256.),
            Vec3::splat(radius) + Vec3::Y * ((overshoot.max(0.) + 1.) / 256.),
        )
    }
}
pub(super) fn source(shape: Shape) -> Source {
    let mut triangles = Vec::new();
    let mut ranges = Vec::new();
    let mut palette = Vec::new();
    let leaves = model_assets::leaf_variants();
    let transforms = leaves.transforms(0., 0);
    for variant in 0..64 {
        let first = triangles.len() as u32;
        for t in leaves.triangles.iter().filter(|t| t.node == variant) {
            let m = transforms[t.node];
            let normal = Mat3::from_mat4(m).inverse().transpose();
            triangles.push(triangle(
                t.positions.map(|p| m.transform_point3(p)),
                t.normals.map(|n| (normal * n).normalize()),
                t.uvs,
                0,
            ));
        }
        ranges.push([first, triangles.len() as u32 - first, 0, variant as u32]);
    }
    let apple = super::apple_preview::mesh();
    let first = triangles.len() as u32;
    for ids in apple.indices.chunks_exact(3) {
        let ids = [ids[0] as usize, ids[1] as usize, ids[2] as usize];
        triangles.push(triangle(
            ids.map(|i| apple.positions[i] * 0.5),
            ids.map(|i| apple.normals[i]),
            [Vec2::ZERO; 3],
            apple.materials[ids[0]] as u32,
        ));
    }
    ranges.push([first, triangles.len() as u32 - first, 1, 0]);
    let butterfly = model_assets::butterfly();
    for frame in 0..ANIMATION_FRAMES {
        let transforms = butterfly.transforms(frame as f32 / ANIMATION_FRAMES as f32, 0);
        let root = transforms[butterfly.node("Flight pose")].w_axis.truncate();
        let first = triangles.len() as u32;
        for t in &butterfly.triangles {
            let m = transforms[t.node];
            // Use the actual authored animated surface normals, not a billboard normal.
            let nm = Mat3::from_mat4(m).inverse().transpose();
            triangles.push(triangle(
                t.positions.map(|p| m.transform_point3(p) - root),
                t.normals.map(|n| (nm * n).normalize()),
                t.uvs,
                0,
            ));
        }
        ranges.push([first, triangles.len() as u32 - first, 2, frame]);
    }
    let mut flower_parts = Vec::new();
    let mut flower_root_radius = 4f32;
    let mut flower_spawn_height = 0f32;
    for (model, authored) in models::flowers().iter().enumerate() {
        let flower = authored.transformed(shape);
        palette.extend(authored.palette.iter().map(|rgb| {
            [
                f32::from(rgb[0]) / 255.,
                f32::from(rgb[1]) / 255.,
                f32::from(rgb[2]) / 255.,
                0.,
            ]
        }));
        let shared = (flower_source(model, 0) as usize) < ranges.len();
        let first = if shared {
            ranges[flower_source(model, 0) as usize][0]
        } else {
            triangles.len() as u32
        };
        for part in std::iter::once(&flower.whole).chain(&flower.heads) {
            flower_root_radius = flower_root_radius.max(
                flower.column.tip().length()
                    + (part.center - flower.column.tip()).length()
                    + part.radius * std::f32::consts::SQRT_2
                    + flower.column.tip().y * models::MAX_BEND_FRACTION,
            );
        }
        flower_spawn_height = flower_spawn_height.max(flower.whole.center.y + flower.whole.radius);
        for index in 0..=models::MAX_HEADS {
            let part = if index == 0 {
                Some(&flower.whole)
            } else {
                flower.heads.get(index - 1)
            };
            flower_parts.push(part.map_or(FlowerPart::zeroed(), |part| FlowerPart {
                range: [
                    first + part.triangles.start,
                    part.triangles.end - part.triangles.start,
                    flower_source(model, index),
                    flower.heads.len() as u32,
                ],
                center_radius: part.center.extend(part.radius).to_array(),
                distribution: flower.distribution,
                socket: flower.socket_normal.extend(0.).to_array(),
                stem: [
                    flower.column.tip().x,
                    flower.column.tip().y,
                    models::MAX_BEND_FRACTION,
                    flower.column.count() as f32,
                ],
            }));
        }
        for (index, t) in flower.triangles.iter().enumerate() {
            let mut gpu = triangle(t.positions, [t.normal; 3], t.uvs, 0);
            gpu.normals[0][3] = t.anchor.y;
            if shared {
                assert_eq!(
                    bytemuck::bytes_of(&gpu),
                    bytemuck::bytes_of(&triangles[first as usize + index])
                );
            } else {
                triangles.push(gpu);
            }
        }
        if !shared {
            for (index, part) in std::iter::once(&flower.whole)
                .chain(&flower.heads)
                .enumerate()
            {
                assert_eq!(ranges.len() as u32, flower_source(model, index));
                ranges.push([
                    first + part.triangles.start,
                    part.triangles.end - part.triangles.start,
                    3,
                    ranges.len() as u32 - FLOWER_SOURCE_BASE,
                ]);
            }
        }
    }
    // The authored palette texture remains ordinary material data, not a
    // geometry/depth atlas. Preserve the existing sRGB-to-linear decoder.
    for (model, flower) in models::flowers().iter().enumerate() {
        let texture = &flower.color_texture;
        let base = model * models::HEAD_PALETTE_SIZE;
        palette[base][3] = palette.len() as f32;
        palette[base + 1][3] = texture.width as f32;
        palette[base + 2][3] = texture.height as f32;
        palette.extend(texture.rgb.chunks_exact(3).map(|rgb| {
            let linear = |v: u8| {
                let v = f32::from(v) / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            [linear(rgb[0]), linear(rgb[1]), linear(rgb[2]), 0.]
        }));
    }
    Source {
        triangles,
        ranges,
        palette,
        flower_parts,
        flower_root_radius,
        flower_spawn_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ranges_and_parts_fit_the_triangle_buffer() {
        let geometry = source(Shape::default());
        assert_eq!(std::mem::size_of::<Triangle>(), 128);
        assert_eq!(std::mem::size_of::<FlowerPart>(), 80);
        for r in &geometry.ranges {
            assert!(r[1] > 0);
            assert!((r[0] + r[1]) as usize <= geometry.triangles.len());
        }
        for p in &geometry.flower_parts {
            assert!((p.range[0] + p.range[1]) as usize <= geometry.triangles.len());
        }
        assert_eq!(geometry.ranges[64][1], 780);
        assert_eq!(geometry.flower_parts.len(), models::flowers().len() * 4);
    }
    #[test]
    fn animation_source_ids_remain_bounded_and_loop() {
        assert_eq!(animation_frame(0.), animation_frame(1.));
        for frame in 0..32 {
            assert_eq!(animation_frame(frame as f32 / 32.), frame);
        }
    }
}
