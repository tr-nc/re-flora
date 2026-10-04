//! CPU path adapter. Rest arc, not node index or current world height, owns bands.
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct StemPathPoint {
    pub position: Vec3,
    pub rest_arc: f32,
    pub tangent: Vec3,
    pub side: Vec3,
    pub parent: Option<usize>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StemBandInstance {
    pub a_radius: [f32; 4],
    pub b_radius: [f32; 4],
    pub tangent_a: [f32; 3],
    pub tangent_b: [f32; 3],
    pub side_a: [f32; 3],
    pub side_b: [f32; 3],
    pub color_srgb: [f32; 3],
    pub shading_position: [f32; 3],
    pub shading_normal: [f32; 3],
}

/// Points must be finite, parent-before-child, with increasing rest arc on edges.
/// Shading is anchored at the band start so every geometric subdivision of one
/// band samples the same light, including partially grown terminal bands.
pub fn path_bands(
    points: &[StemPathPoint],
    full_arc: f32,
    bands: u32,
    radius: f32,
    mode: u32,
    palette: [Vec3; 2],
) -> Vec<StemBandInstance> {
    assert!(full_arc.is_finite() && full_arc > 0.0 && bands > 0);
    assert!(radius.is_finite() && radius > 0.0);
    let cell = full_arc / bands as f32;
    let mut result = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let Some(parent) = point.parent else { continue };
        assert!(parent < index);
        let previous = points[parent];
        assert!(point.rest_arc > previous.rest_arc);
        let mut arc = previous.rest_arc;
        while arc < point.rest_arc {
            let band = ((arc / cell + 1e-5).floor() as u32).min(bands - 1);
            let end = if band + 1 == bands {
                point.rest_arc
            } else {
                point.rest_arc.min((band + 1) as f32 * cell)
            };
            let at = |s: f32| {
                let t =
                    ((s - previous.rest_arc) / (point.rest_arc - previous.rest_arc)).clamp(0., 1.);
                (
                    previous.position.lerp(point.position, t),
                    previous.tangent.lerp(point.tangent, t).normalize_or_zero(),
                    previous.side.lerp(point.side, t).normalize_or_zero(),
                )
            };
            let (a, tangent_a, side_a) = at(arc);
            let (b, tangent_b, side_b) = at(end);
            let shading_arc = band as f32 * cell;
            let (shading_position, shading_normal) = ancestor_sample(points, index, shading_arc);
            let taper = |s: f32| {
                if mode == 2 {
                    1. - 0.88 * (s / full_arc).clamp(0., 1.)
                } else {
                    1.
                }
            };
            let color = palette[0].lerp(palette[1], band as f32 / (bands - 1).max(1) as f32);
            result.push(StemBandInstance {
                a_radius: a.extend(radius * taper(arc)).to_array(),
                b_radius: b.extend(radius * taper(end)).to_array(),
                tangent_a: tangent_a.to_array(),
                tangent_b: tangent_b.to_array(),
                side_a: side_a.to_array(),
                side_b: side_b.to_array(),
                color_srgb: color.to_array(),
                shading_position: shading_position.to_array(),
                shading_normal: shading_normal.to_array(),
            });
            arc = end;
        }
    }
    result
}

fn ancestor_sample(points: &[StemPathPoint], mut index: usize, arc: f32) -> (Vec3, Vec3) {
    while let Some(parent) = points[index].parent {
        let a = points[parent];
        let b = points[index];
        if a.rest_arc <= arc {
            let t = ((arc - a.rest_arc) / (b.rest_arc - a.rest_arc)).clamp(0., 1.);
            return (
                a.position.lerp(b.position, t),
                a.tangent.lerp(b.tangent, t).normalize_or_zero(),
            );
        }
        index = parent;
    }
    (points[index].position, points[index].tangent)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(y: f32, parent: Option<usize>) -> StemPathPoint {
        StemPathPoint {
            position: Vec3::Y * y,
            rest_arc: y,
            tangent: Vec3::Y,
            side: Vec3::X,
            parent,
        }
    }
    #[test]
    fn subdivisions_share_band_color_light_and_rings() {
        let sparse = [point(0., None), point(4., Some(0))];
        let dense = [
            point(0., None),
            point(1., Some(0)),
            point(2., Some(1)),
            point(4., Some(2)),
        ];
        let palette = [Vec3::ZERO, Vec3::ONE];
        let a = path_bands(&sparse, 4., 2, 0.5, 1, palette);
        let b = path_bands(&dense, 4., 2, 0.5, 1, palette);
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].color_srgb, b[1].color_srgb);
        assert_eq!(b[0].shading_position, b[1].shading_position);
        assert_eq!(a[0].a_radius, b[0].a_radius);
        assert_eq!(a[0].b_radius, b[1].b_radius);
        assert_eq!(a[1], b[2]);
        assert_eq!(b[0].b_radius, b[1].a_radius);
    }
    #[test]
    fn growth_appends_without_recoloring_existing_material() {
        let palette = [Vec3::ZERO, Vec3::ONE];
        let a = path_bands(
            &[point(0., None), point(1., Some(0))],
            4.,
            4,
            0.5,
            2,
            palette,
        );
        let b = path_bands(
            &[point(0., None), point(1., Some(0)), point(3., Some(1))],
            4.,
            4,
            0.5,
            2,
            palette,
        );
        assert_eq!(a[0], b[0]);
        assert!(b.last().unwrap().b_radius[3] < a[0].a_radius[3]);
    }
    #[test]
    fn deformation_changes_positions_not_material_bands() {
        let points = [point(0., None), point(4., Some(0))];
        let mut bent = points;
        bent[1].position.x = 2.;
        bent[1].tangent = Vec3::new(0.5, 1., 0.).normalize();
        let a = path_bands(&points, 4., 4, 0.5, 1, [Vec3::ZERO, Vec3::ONE]);
        let b = path_bands(&bent, 4., 4, 0.5, 1, [Vec3::ZERO, Vec3::ONE]);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.color_srgb, b.color_srgb);
        }
        assert_ne!(a.last().unwrap().b_radius, b.last().unwrap().b_radius);
        assert_eq!(std::mem::size_of::<StemBandInstance>(), 29 * 4);
    }
}
