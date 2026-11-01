//! Low-wall layout over the SAME PathStroke as pavement: s / u / height.
//! Masonry blocks are voxel unions of two shared closed half-cell primitives.
//! There is no head gap at their internal split; alternating layers move the
//! actual head gaps, not just an overlay. No persistent spline or wall authority.
use super::path_band::{
    inset_quad_edges, CellSolid, PathCell, PathSample, PathSampling, PathSettings, VOXEL_SIZE,
};
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LowWallKind {
    Earth,
    Stone,
    Brick,
    Chinese,
}
impl LowWallKind {
    pub(super) fn from_choice(value: u32) -> Self {
        match value {
            1 => Self::Stone,
            2 => Self::Brick,
            3 => Self::Chinese,
            _ => Self::Earth,
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Earth => "Earth",
            Self::Stone => "Stone",
            Self::Brick => "Brick",
            Self::Chinese => "Chinese red wall",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct LowWallRecipe {
    pub kind: LowWallKind,
    pub width: f32,
    pub height: f32,
    pub pitch: f32,
    seed: u32,
}
fn bounded(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}
impl LowWallRecipe {
    pub(super) fn new(kind: LowWallKind, width: f32, height: f32, pitch: f32, seed: u32) -> Self {
        let width = bounded(width, 0.06, 0.05, 0.15);
        Self {
            kind,
            width,
            height: bounded(height, 0.12, 0.02, 0.3),
            // Same bounded cell aspect as paving: the rounded guide's minimum
            // radius is the band width, so do not step past a whole inner bend.
            pitch: bounded(pitch, 0.05, 0.025, 0.1).min(width),
            seed,
        }
    }
    pub(super) fn path_settings(self) -> PathSettings {
        PathSettings {
            width: self.width,
            pitch: self.pitch,
            sampling: PathSampling::Courses,
        }
    }
    pub(super) fn layers(self) -> usize {
        if self.kind == LowWallKind::Chinese {
            return 30; // Body + roof bed + 24 pan/cover panels + four ridge panels.
        }
        if self.kind == LowWallKind::Earth {
            1
        } else {
            let ratio = if self.kind == LowWallKind::Brick {
                0.6
            } else {
                0.85
            };
            (self.height / (self.pitch * ratio).max(6. * VOXEL_SIZE))
                .round()
                .max(1.) as usize
        }
    }
    pub(super) fn raw_cell(self, row: PathSample) -> PathCell {
        PathCell {
            center: row.center,
            forward: row.forward,
            size: Vec3::new(self.width, self.height, self.pitch),
            seed: self.seed,
            footprint: [
                row.entry.point(-self.width * 0.5),
                row.entry.point(self.width * 0.5),
                row.exit.point(self.width * 0.5),
                row.exit.point(-self.width * 0.5),
            ],
        }
    }
    fn head_gap(self, node: u32, layer: usize) -> f32 {
        if self.kind != LowWallKind::Stone {
            return 2.5 * VOXEL_SIZE;
        }
        let mut seed =
            self.seed ^ node.wrapping_mul(0x9e3779b9) ^ (layer as u32).wrapping_mul(0x85ebca6b);
        seed ^= seed >> 16;
        seed = seed.wrapping_mul(0x7feb352d);
        seed ^= seed >> 15;
        (2.5 + 0.35 * (seed & 1023) as f32 / 1023.) * VOXEL_SIZE
    }
    /// Whole vertical column or none: no selective missing layer in ordinary bends.
    pub(super) fn pieces(self, row: PathSample) -> Vec<WallSlice> {
        if !row.valid {
            return vec![];
        }
        let raw = self.raw_cell(row);
        // Also validates convexity, positive dimensions and finite band sections.
        if inset_quad_edges(raw.footprint, [0.; 4]).is_none() {
            return vec![];
        }
        if self.kind == LowWallKind::Chinese {
            return self.chinese_pieces(row, raw);
        }
        if self.kind == LowWallKind::Earth {
            return vec![WallSlice {
                cell: raw,
                layer: 0,
                lower: -VOXEL_SIZE,
                upper: self.height,
                top_scale: 0.82,
                top_offsets: [0.; 4],
            }];
        }
        let count = self.layers();
        let step = self.height / count as f32;
        let bed_gap = 2.5 * VOXEL_SIZE;
        let mut pieces = Vec::with_capacity(count);
        for layer in 0..count {
            // One logical block spans two half-cells. Neighbours use the same
            // physical plane and head-gap identity; the internal plane has no gap.
            let starts_head = (row.index as usize + layer).is_multiple_of(2);
            let edges = [
                if starts_head {
                    self.head_gap(row.index, layer) * 0.5
                } else {
                    0.
                },
                0.,
                if !starts_head {
                    self.head_gap(
                        if row.closes_loop {
                            0
                        } else {
                            row.index.wrapping_add(1)
                        },
                        layer,
                    ) * 0.5
                } else {
                    0.
                },
                0.,
            ];
            let Some(footprint) = inset_quad_edges(raw.footprint, edges) else {
                return vec![];
            };
            let lower = if layer == 0 {
                -VOXEL_SIZE
            } else {
                layer as f32 * step + bed_gap * 0.5
            };
            let upper = if layer + 1 == count {
                self.height
            } else {
                (layer + 1) as f32 * step - bed_gap * 0.5
            };
            if upper <= lower {
                return vec![];
            }
            pieces.push(WallSlice {
                cell: PathCell {
                    size: Vec3::new(self.width, upper - lower, self.pitch),
                    footprint,
                    ..raw
                },
                layer,
                lower,
                upper,
                top_scale: 1.,
                top_offsets: [0.; 4],
            });
        }
        pieces
    }

    fn chinese_pieces(self, row: PathSample, raw: PathCell) -> Vec<WallSlice> {
        let cap = (self.width * 0.32).min(self.height * 0.45);
        let body_top = self.height - cap;
        // t follows the wall; u runs downhill across the two roof slopes.
        let point = |t: f32, u: f32| {
            row.entry
                .point(u * self.width * 0.5)
                .lerp(row.exit.point(u * self.width * 0.5), t)
        };
        let footprint =
            |t0, t1, u0, u1| [point(t0, u0), point(t0, u1), point(t1, u1), point(t1, u0)];
        let relief = cap * 0.22;
        let mut pieces = vec![WallSlice {
            cell: PathCell {
                footprint: footprint(0., 1., -0.76, 0.76),
                ..raw
            },
            layer: 0,
            lower: -VOXEL_SIZE,
            upper: body_top,
            top_scale: 1.,
            top_offsets: [0.; 4],
        }];
        // The roof bed closes the joints: recessed tile seams must not become
        // holes looking through the wall cap to the ground behind it.
        pieces.push(WallSlice {
            cell: raw,
            layer: 1,
            lower: body_top - VOXEL_SIZE * 0.25,
            upper: body_top + cap * 0.20,
            top_scale: 1.,
            top_offsets: [0.; 4],
        });
        // Concave pans drain across the wall. Rounded cover rolls bridge the
        // joints between lanes, rather than engraving lines on a flat grey cap.
        let stations = [
            (0., 0.95),
            (0.10, 0.72),
            (0.25, 0.08),
            (0.50, 0.),
            (0.75, 0.08),
            (0.90, 0.72),
            (1., 0.95),
        ];
        let roof = |u: f32| body_top + cap * (0.26 + 0.49 * (1. - u.abs()).powf(0.85));
        for side in [-1., 1.] {
            for course in 0..2 {
                // The upper course overlaps the lower one, leaving a proud lip.
                let near = course as f32 * 0.45;
                let far = (near + 0.55).min(1.);
                let (u0, u1) = if side < 0. {
                    (-far, -near)
                } else {
                    (near, far)
                };
                let lip = if course == 0 { cap * 0.035 } else { 0. };
                for pair in stations.windows(2) {
                    let [(t0, roll0), (t1, roll1)] = [pair[0], pair[1]];
                    let height = |u, roll| roof(u) + relief * roll + lip;
                    pieces.push(WallSlice {
                        cell: PathCell {
                            footprint: footprint(t0, t1, u0, u1),
                            ..raw
                        },
                        layer: pieces.len(),
                        lower: body_top - VOXEL_SIZE * 0.25,
                        upper: self.height,
                        top_scale: 1.,
                        top_offsets: [
                            height(u0, roll0),
                            height(u1, roll0),
                            height(u1, roll1),
                            height(u0, roll1),
                        ]
                        .map(|h| h - self.height),
                    });
                }
            }
        }
        // Four facets read as a round ridge tile even at the native voxel scale.
        let ridge = [
            (-0.20, 0.80),
            (-0.12, 0.95),
            (0., 1.),
            (0.12, 0.95),
            (0.20, 0.80),
        ];
        for pair in ridge.windows(2) {
            let [(a, ha), (b, hb)] = [pair[0], pair[1]];
            pieces.push(WallSlice {
                cell: PathCell {
                    footprint: footprint(0., 1., a, b),
                    ..raw
                },
                layer: pieces.len(),
                lower: body_top + cap * 0.55,
                upper: self.height,
                top_scale: 1.,
                top_offsets: [ha, hb, hb, ha].map(|h| cap * (h - 1.)),
            });
        }
        pieces
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct WallSlice {
    pub cell: PathCell,
    pub layer: usize,
    pub lower: f32,
    pub upper: f32,
    pub top_scale: f32,
    top_offsets: [f32; 4],
}
impl WallSlice {
    pub(super) fn solid(self, ground: [f32; 4]) -> CellSolid {
        let mut solid = CellSolid::new(
            self.cell.footprint,
            ground,
            self.lower,
            self.upper,
            self.top_scale,
        );
        for i in 0..4 {
            solid.positions[i + 4].y += self.top_offsets[i];
        }
        solid
    }
    pub(super) fn material(self, kind: LowWallKind) -> u32 {
        use crate::builder::*;
        match kind {
            LowWallKind::Earth => VOXEL_TYPE_DIRT,
            LowWallKind::Stone => VOXEL_TYPE_ROCK,
            LowWallKind::Brick => VOXEL_TYPE_TERRACOTTA,
            LowWallKind::Chinese if self.layer == 0 => VOXEL_MATERIAL_IRON_RED,
            LowWallKind::Chinese => VOXEL_TYPE_ASPHALT,
        }
    }
    pub(super) fn preview(self) -> PathCell {
        let mut cell = self.cell;
        cell.center.y += self.lower;
        cell.size.y = self.upper - self.lower;
        for i in 0..4 {
            let middle = if i < 2 {
                (self.cell.footprint[0] + self.cell.footprint[1]) * 0.5
            } else {
                (self.cell.footprint[2] + self.cell.footprint[3]) * 0.5
            };
            cell.footprint[i] = middle + (self.cell.footprint[i] - middle) * self.top_scale;
        }
        cell
    }
}
#[cfg(test)]
mod tests {
    use super::super::path_band::{contains_polygon, overlaps_quads, PathStroke};
    use super::*;
    fn assert_solid(solid: CellSolid) {
        use std::collections::BTreeMap;
        let volume: f64 = solid
            .triangles()
            .map(|[a, b, c]| a.as_dvec3().dot(b.as_dvec3().cross(c.as_dvec3())) / 6.)
            .sum();
        assert!(volume > 1e-10, "volume={volume}");
        let mut edges = BTreeMap::<(usize, usize), Vec<(usize, usize)>>::new();
        for t in solid.indices.chunks_exact(3) {
            let points = t.iter().map(|&i| solid.positions[i]).collect::<Vec<_>>();
            assert!(
                (points[1] - points[0])
                    .cross(points[2] - points[0])
                    .length_squared()
                    > 1e-16
            );
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                edges.entry((a.min(b), a.max(b))).or_default().push((a, b));
            }
        }
        for edge in edges.values() {
            assert_eq!(edge.len(), 2);
            assert_eq!(edge[0], (edge[1].1, edge[1].0));
        }
    }
    #[test]
    fn low_walls_height_is_independent_of_guide_width_and_every_layer_is_closed() {
        for kind in [
            LowWallKind::Earth,
            LowWallKind::Stone,
            LowWallKind::Brick,
            LowWallKind::Chinese,
        ] {
            for width in [0.05, 0.06, 0.15] {
                for height in [0.02, 0.12, 0.3] {
                    for pitch in [0.025, 0.05, 0.1] {
                        let recipe = LowWallRecipe::new(kind, width, height, pitch, 42);
                        let row = PathSample::straight(Vec3::ONE, Vec3::X, 0., 0, recipe.pitch);
                        let pieces = recipe.pieces(row);
                        assert_eq!(pieces.len(), recipe.layers());
                        assert_eq!(pieces.last().unwrap().upper, height);
                        for piece in pieces {
                            assert_solid(piece.solid([1.; 4]));
                            assert_solid(piece.solid([1.02, 1.03, 1.06, 1.05]));
                            for p in &piece.cell.footprint {
                                assert!(contains_polygon(
                                    &recipe.raw_cell(row).footprint,
                                    *p,
                                    1e-6
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn low_walls_real_head_joints_stagger_upwards_without_an_internal_half_brick_gap() {
        for kind in [LowWallKind::Stone, LowWallKind::Brick] {
            let recipe = LowWallRecipe::new(kind, 0.06, 0.12, 0.05, 42);
            let a = PathSample::straight(Vec3::ONE, Vec3::X, 0., 0, recipe.pitch);
            let b = PathSample::straight(
                Vec3::ONE + Vec3::X * recipe.pitch,
                Vec3::X,
                recipe.pitch,
                1,
                recipe.pitch,
            );
            let a = recipe.pieces(a);
            let b = recipe.pieces(b);
            for layer in 0..recipe.layers() {
                let end = a[layer].cell.footprint[2];
                let start = b[layer].cell.footprint[1];
                let gap = (start - end).dot(glam::Vec2::X);
                if layer % 2 == 0 {
                    assert!(gap.abs() < 1e-6);
                } else {
                    assert!(gap >= 2.5 * VOXEL_SIZE - 1e-6);
                }
                assert_eq!(a[layer].upper, b[layer].upper);
                if layer > 0 {
                    assert!((a[layer].lower - a[layer - 1].upper - 2.5 * VOXEL_SIZE).abs() < 1e-6);
                }
            }
        }
    }
    #[test]
    fn low_walls_share_complete_curved_cells_and_no_earth_sampling_seams() {
        for kind in [
            LowWallKind::Earth,
            LowWallKind::Stone,
            LowWallKind::Brick,
            LowWallKind::Chinese,
        ] {
            for width in [0.05, 0.06, 0.15] {
                for pitch in [0.025, 0.05, 0.1] {
                    for sign in [-1., 1.] {
                        let recipe = LowWallRecipe::new(kind, width, 0.12, pitch, 42);
                        let mut path =
                            PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
                        for i in 1..=90 {
                            let t = std::f32::consts::FRAC_PI_2 * i as f32 / 90.;
                            assert!(path.push(
                                Vec3::ONE
                                    + Vec3::new(
                                        width * 1.5 * t.sin(),
                                        0.,
                                        sign * width * 1.5 * (1. - t.cos())
                                    )
                            ));
                        }
                        assert!(path.finish());
                        let mut previous: Option<PathSample> = None;
                        for row in path.rows {
                            let pieces = recipe.pieces(row);
                            assert_eq!(
                                pieces.len(),
                                recipe.layers(),
                                "{kind:?} width={width} pitch={pitch} sign={sign} row={row:?} raw={:?}",recipe.raw_cell(row).footprint
                            );
                            for piece in pieces {
                                assert_solid(piece.solid([1.; 4]));
                            }
                            if let Some(prev) = previous {
                                assert_eq!(prev.exit, row.entry);
                                assert!(!overlaps_quads(
                                    recipe.raw_cell(prev).footprint,
                                    recipe.raw_cell(row).footprint,
                                    -1e-6
                                ));
                                if kind == LowWallKind::Earth {
                                    let a = recipe.pieces(prev)[0].solid([1.; 4]);
                                    let b = recipe.pieces(row)[0].solid([1.; 4]);
                                    assert_eq!(a.positions[6], b.positions[5]);
                                    assert_eq!(a.positions[7], b.positions[4]);
                                }
                            }
                            previous = Some(row);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn chinese_wall_has_independent_red_plaster_pitched_tiles_and_real_eaves() {
        assert_eq!(LowWallKind::from_choice(3), LowWallKind::Chinese);
        let wall = LowWallRecipe::new(LowWallKind::Chinese, 0.1, 0.2, 0.05, 42);
        let row = PathSample::straight(Vec3::ONE, Vec3::X, 0., 0, wall.pitch);
        let pieces = wall.pieces(row);
        assert_eq!(pieces.len(), wall.layers());
        assert_eq!(
            pieces[0].material(wall.kind),
            crate::builder::VOXEL_MATERIAL_IRON_RED
        );
        assert!(pieces[1..]
            .iter()
            .all(|p| p.material(wall.kind) == crate::builder::VOXEL_TYPE_ASPHALT));
        let body = pieces[0].solid([1.; 4]);
        let bed = pieces[1].solid([1.; 4]);
        let eave = pieces[8].solid([1.; 4]);
        let ridge = pieces[27].solid([1.; 4]);
        assert!(bed.positions[4].y < eave.positions[4].y);
        assert!(eave.positions[4].y < ridge.positions[5].y);
        assert!((ridge.positions[5].y - 1.2).abs() < 1e-6);
        let pan = pieces[10].solid([1.; 4]);
        assert!(
            eave.positions[4].y > pan.positions[7].y,
            "rounded cover must stand above the concave pan at the eave"
        );
        assert!(body.positions[0].z < eave.positions[0].z);
        assert!(eave.positions[5].y > body.positions[5].y);
    }
    #[test]
    fn low_walls_invalid_and_outlandish_inputs_never_make_nan_or_folded_prisms() {
        let recipe = LowWallRecipe::new(
            LowWallKind::Brick,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            42,
        );
        assert_eq!(recipe.width, 0.06);
        assert_eq!(recipe.height, 0.12);
        assert_eq!(recipe.pitch, 0.05);
        let mut path = PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        assert!(path.push(Vec3::ONE + Vec3::X * 0.3));
        assert!(path.push(Vec3::ONE));
        assert!(path.finish());
        assert!(path.rows.iter().any(|r| !r.valid));
        for row in path.rows {
            if !row.valid {
                assert!(recipe.pieces(row).is_empty());
            }
            for piece in recipe.pieces(row) {
                assert_solid(piece.solid([1.; 4]));
            }
        }
    }
}
