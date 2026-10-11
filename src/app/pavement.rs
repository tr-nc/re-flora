//! Paving patterns over the shared path band; terrain/GPU work stays in the adapter.
pub(super) use super::path_band::{contains_polygon, PathSample, VOXEL_SIZE};
use super::path_band::{inset_quad, overlaps_quads, PathCell, PathSampling, PathSettings};
#[cfg(test)]
use super::path_band::{PathStroke, MAX_QUEUED_ROWS};
use glam::{Vec2, Vec3};

// The existing two-voxel minimum applies to the DEFORMED tile as well as
// nominal lane widths. A mathematically positive sliver is not a usable stone.
const MIN_PIECE_EXTENT: f32 = 2. * VOXEL_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PavementKind {
    Slabs,
    Brick,
    Dirt,
}
impl PavementKind {
    pub(super) fn from_choice(value: u32) -> Self {
        match value {
            1 => Self::Brick,
            2 => Self::Dirt,
            _ => Self::Slabs,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct PavementRecipe {
    pub kind: PavementKind,
    pub width: f32,
    pub pitch: f32,
    pub seed: u32,
}
impl PavementRecipe {
    pub(super) fn new(kind: PavementKind, width: f32, length: f32, seed: u32) -> Self {
        let width = finite_clamp(width, 0.12, 0.05, 0.3);
        Self {
            kind,
            width,
            pitch: if kind == PavementKind::Dirt {
                width * 0.25
            } else {
                finite_clamp(length, 0.05, 0.025, 0.1).min(width)
            },
            seed,
        }
    }
    pub(super) fn path_settings(self) -> PathSettings {
        PathSettings {
            width: self.width,
            pitch: self.pitch,
            sampling: if self.kind == PavementKind::Dirt {
                PathSampling::Surface
            } else {
                PathSampling::Courses
            },
        }
    }
    pub(super) fn pieces(self, row: PathSample) -> Vec<PathCell> {
        if self.kind == PavementKind::Dirt || !row.valid {
            return Vec::new();
        }
        let nominal_width = if self.kind == PavementKind::Brick {
            self.pitch * 2.
        } else {
            self.pitch
        };
        let lanes = (self.width / nominal_width).ceil().max(1.) as u32;
        let lane_width = self.width / lanes as f32;
        let gap = (2.5 * VOXEL_SIZE).min(lane_width.min(self.pitch) * 0.3);
        let stagger = if self.kind == PavementKind::Brick && row.index % 2 == 1 {
            lane_width * 0.5
        } else {
            0.
        };
        let mut pieces = Vec::new();
        for lane in 0..=lanes {
            let left =
                (-self.width * 0.5 + lane as f32 * lane_width - stagger).max(-self.width * 0.5);
            let right = (-self.width * 0.5 + (lane + 1) as f32 * lane_width - stagger)
                .min(self.width * 0.5);
            if right - left < gap + MIN_PIECE_EXTENT {
                continue;
            }
            let cell = [
                row.entry.point(left),
                row.entry.point(right),
                row.exit.point(right),
                row.exit.point(left),
            ];
            // Insets are physical distances in XZ, not a parameter-space gap
            // which would widen outside the bend and disappear on its inside.
            let Some(footprint) = inset_quad(cell, gap * 0.5) else {
                return Vec::new(); // explicit unusable course, never a folded mesh
            };
            if !resolved_footprint(footprint) {
                return Vec::new(); // reject the whole course, never lose an inner tile
            }
            let center = footprint.iter().copied().sum::<Vec2>() * 0.25;
            let width = right - left - gap;
            let length = self.pitch - gap;
            pieces.push(PathCell {
                center: Vec3::new(center.x, row.center.y, center.y),
                forward: row.forward,
                size: Vec3::new(width, (width.min(length) * 0.2).min(0.008), length),
                seed: hash(self.seed ^ row.index.wrapping_mul(0x9e3779b9) ^ lane),
                footprint,
            });
        }
        pieces
    }
}
fn resolved_footprint(points: [Vec2; 4]) -> bool {
    (0..4).all(|i| {
        let edge = points[(i + 1) % 4] - points[i];
        let length = edge.length();
        length >= MIN_PIECE_EXTENT - 1e-6
            && [2, 3].into_iter().all(|offset| {
                edge.perp_dot(points[(i + offset) % 4] - points[i]) / length
                    >= MIN_PIECE_EXTENT - 1e-6
            })
    })
}

fn finite_clamp(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}
fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^ (value >> 16)
}
#[cfg(test)]
fn xz(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z)
}

#[derive(Default)]
pub(super) struct PavementCoverage {
    pieces: Vec<PathCell>,
}
impl PavementCoverage {
    pub(super) fn published_pieces(&self) -> &[PathCell] {
        &self.pieces
    }
    pub(super) fn accepts(&self, candidate: PathCell) -> bool {
        !self.pieces.iter().any(|&piece| overlaps(piece, candidate))
    }
    pub(super) fn record(&mut self, piece: PathCell) {
        self.pieces.push(piece);
    }
}
fn overlaps(a: PathCell, b: PathCell) -> bool {
    overlaps_quads(a.footprint, b.footprint, VOXEL_SIZE * 1.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe(kind: PavementKind) -> PavementRecipe {
        PavementRecipe::new(kind, 0.16, 0.05, 42)
    }
    fn sample_path(points: &[Vec3], kind: PavementKind) -> Vec<PathSample> {
        let mut stroke = PathStroke::new(recipe(kind).path_settings(), points[0], Vec3::Z).unwrap();
        for &point in &points[1..] {
            assert!(stroke.push(point));
        }
        stroke.finish();
        stroke.rows.into_iter().collect()
    }

    #[test]
    fn sparse_and_dense_mouse_events_make_the_same_courses_and_seeds() {
        let dense: Vec<_> = (0..101)
            .map(|i| Vec3::new(i as f32 * 0.01, 1., 0.))
            .collect();
        let sparse = sample_path(&[dense[0], dense[100]], PavementKind::Slabs);
        let dense = sample_path(&dense, PavementKind::Slabs);
        assert_eq!(sparse.len(), dense.len());
        for (a, b) in sparse.iter().zip(&dense) {
            assert!(a.center.distance(b.center) < 1e-5);
            assert!(a.forward.distance(Vec3::X) < 1e-5);
            assert!((a.distance - b.distance).abs() < 1e-5);
            assert_eq!(
                recipe(PavementKind::Slabs).pieces(*a)[0].seed,
                recipe(PavementKind::Slabs).pieces(*b)[0].seed
            );
        }
    }
    #[test]
    fn standing_still_does_not_generate_courses_and_click_is_one_patch() {
        let mut stroke = PathStroke::new(
            recipe(PavementKind::Slabs).path_settings(),
            Vec3::ONE,
            Vec3::X,
        )
        .unwrap();
        for _ in 0..100 {
            assert!(stroke.push(Vec3::ONE));
        }
        assert!(stroke.rows.is_empty());
        stroke.finish();
        assert_eq!(stroke.rows.len(), 1);
        assert_eq!(stroke.rows[0].forward, Vec3::X);
    }
    #[test]
    fn path_coordinates_rotate_with_vertical_diagonal_and_reversed_strokes() {
        for direction in [
            Vec3::X,
            Vec3::Z,
            Vec3::NEG_Z,
            (Vec3::X + Vec3::Z).normalize(),
        ] {
            let rows = sample_path(&[Vec3::ZERO, direction], PavementKind::Brick);
            for row in rows {
                assert!(row.forward.distance(direction) < 1e-5);
                for piece in recipe(PavementKind::Brick).pieces(row) {
                    assert!(piece.forward.distance(direction) < 1e-5);
                    assert!((piece.center - row.center).dot(direction).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn running_bond_is_deliberate_and_clipped_to_a_constant_corridor() {
        let recipe = recipe(PavementKind::Brick);
        let row = PathSample::straight(Vec3::ZERO, Vec3::Z, 0., 0, recipe.pitch);
        let even = recipe.pieces(row);
        let odd = recipe.pieces(PathSample { index: 1, ..row });
        assert_eq!(even.len(), 2);
        assert_eq!(odd.len(), 3);
        assert!(odd[0].size.x < even[0].size.x);
        for piece in even.iter().chain(&odd) {
            assert!(piece.center.x.abs() + piece.size.x * 0.5 < recipe.width * 0.5);
        }
    }
    #[test]
    fn bends_use_centered_tangents_and_uturns_stay_finite() {
        let rows = sample_path(
            &[Vec3::ZERO, Vec3::X * 0.5, Vec3::new(0.5, 0., 0.5)],
            PavementKind::Slabs,
        );
        assert!(rows.iter().all(|r| r.valid));
        assert!(rows.iter().any(|r| r.forward.x > 0.3 && r.forward.z > 0.3));
        for row in sample_path(&[Vec3::ZERO, Vec3::X, Vec3::ZERO], PavementKind::Slabs) {
            assert!(row.forward.is_finite());
            assert!((row.forward.length() - 1.).abs() < 1e-5);
        }
    }
    #[test]
    fn dirt_is_isotropic_and_reaches_the_exact_endpoint() {
        let end = Vec3::new(0.213, 0.1, 0.);
        let rows = sample_path(&[Vec3::ZERO, end], PavementKind::Dirt);
        assert_eq!(rows.last().unwrap().center, end);
        assert!(recipe(PavementKind::Dirt).pieces(rows[0]).is_empty());
    }
    #[test]
    fn coverage_allows_regular_joints_but_rejects_retracing_and_inner_turn_collisions() {
        let recipe = recipe(PavementKind::Slabs);
        let rows = sample_path(&[Vec3::ZERO, Vec3::Z * 0.2], PavementKind::Slabs);
        let mut coverage = PavementCoverage::default();
        let pieces: Vec<_> = rows.iter().flat_map(|r| recipe.pieces(*r)).collect();
        for piece in &pieces {
            assert!(coverage.accepts(*piece));
            coverage.record(*piece);
            assert!(!coverage.accepts(*piece));
        }
        let crossing =
            PathCell::straight(pieces[0].center, Vec3::X, pieces[0].size, pieces[0].seed);
        assert!(!coverage.accepts(crossing));
    }
    #[test]
    fn pavement_bends_keep_every_planned_tile_in_complete_courses() {
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for sign in [-1., 1.] {
                let recipe = PavementRecipe::new(kind, 0.12, 0.05, 42);
                let mut stroke =
                    PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
                for i in 1..=180 {
                    let angle = std::f32::consts::FRAC_PI_2 * i as f32 / 180.;
                    assert!(stroke.push(
                        Vec3::ONE
                            + Vec3::new(0.18 * angle.sin(), 0., sign * 0.18 * (1. - angle.cos()),)
                    ));
                }
                stroke.finish();
                let mut coverage = PavementCoverage::default();
                let mut total = 0;
                for row in stroke.rows {
                    let pieces = recipe.pieces(row);
                    assert!(
                        !pieces.is_empty(),
                        "a normal bend must not discard a whole course"
                    );
                    for piece in pieces {
                        assert!(
                            coverage.accepts(piece),
                            "{kind:?} sign={sign} row={} has a missing tile",
                            row.index
                        );
                        coverage.record(piece);
                        total += 1;
                    }
                }
                assert!(
                    total >= 15,
                    "the complete bend must actually produce paving"
                );
            }
        }
    }

    #[test]
    fn streaming_bends_share_exact_sections_and_keep_physical_joint_width() {
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            let recipe = PavementRecipe::new(kind, 0.12, 0.05, 42);
            let mut coverage = PavementCoverage::default();
            let mut stroke = PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
            let mut previous = None;
            let mut count = 0;
            let mut inspect = |row: PathSample, coverage: &mut PavementCoverage| {
                if let Some(previous) = previous {
                    assert_eq!(row.entry, previous);
                }
                previous = Some(row.exit);
                let pieces = recipe.pieces(row);
                assert!(!pieces.is_empty());
                let gap = 2.5 * VOXEL_SIZE;
                for piece in pieces {
                    assert!(
                        coverage.accepts(piece),
                        "streamed course {} collides",
                        row.index
                    );
                    for (section, corners) in [(row.entry, [0, 1]), (row.exit, [2, 3])] {
                        let axis = Vec2::new(-section.across.z, section.across.x);
                        for corner in corners {
                            let distance = (piece.footprint[corner] - xz(section.center))
                                .dot(axis)
                                .abs();
                            assert!((distance - gap * 0.5).abs() < 2e-6, "joint={distance}");
                        }
                    }
                    coverage.record(piece);
                    count += 1;
                }
            };
            // Longer quarter-circle publishes multiple times BEFORE release.
            for i in 1..=180 {
                let t = std::f32::consts::FRAC_PI_2 * i as f32 / 180.;
                assert!(stroke.push(Vec3::ONE + Vec3::new(0.4 * t.sin(), 0., 0.4 * (1. - t.cos()))));
                assert!(stroke.prepare());
                while let Some(row) = stroke.rows.pop_front() {
                    inspect(row, &mut coverage);
                }
            }
            assert!(
                !coverage.published_pieces().is_empty(),
                "not just a release-time layout"
            );
            assert!(!stroke.pending_rows().is_empty());
            assert!(stroke.finish());
            while let Some(row) = stroke.rows.pop_front() {
                inspect(row, &mut coverage);
            }
            assert!(stroke.pending_rows().is_empty());
            assert!(count >= 30);
        }
    }
    #[test]
    fn normal_bends_and_s_curves_keep_complete_courses_across_saved_extremes() {
        for kind in [PavementKind::Slabs, PavementKind::Brick] {
            for width in [0.05, 0.12, 0.3] {
                for pitch in [0.025, 0.05, 0.1] {
                    let recipe = PavementRecipe::new(kind, width, pitch, 1729);
                    let mut coverage = PavementCoverage::default();
                    let mut stroke =
                        PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
                    let radius = width * 1.5;
                    let mut at = Vec3::ONE;
                    for sign in [1., -1.] {
                        let origin = at;
                        for i in 1..=45 {
                            let t = std::f32::consts::FRAC_PI_2 * i as f32 / 45.;
                            let offset = if sign > 0. {
                                Vec3::new(radius * t.sin(), 0., radius * (1. - t.cos()))
                            } else {
                                Vec3::new(radius * (1. - t.cos()), 0., radius * t.sin())
                            };
                            at = origin + offset;
                            assert!(stroke.push(at));
                            assert!(stroke.prepare());
                            while let Some(row) = stroke.rows.pop_front() {
                                let pieces = recipe.pieces(row);
                                assert!(
                                    !pieces.is_empty(),
                                    "{kind:?} w={width} p={pitch} row={}",
                                    row.index
                                );
                                for piece in pieces {
                                    assert!(
                                        coverage.accepts(piece),
                                        "{kind:?} w={width} p={pitch} row={}",
                                        row.index
                                    );
                                    coverage.record(piece);
                                }
                            }
                        }
                    }
                    assert!(stroke.finish());
                    while let Some(row) = stroke.rows.pop_front() {
                        let pieces = recipe.pieces(row);
                        assert!(!pieces.is_empty(), "{kind:?} w={width} p={pitch} tail");
                        for piece in pieces {
                            assert!(coverage.accepts(piece), "{kind:?} w={width} p={pitch} tail");
                            coverage.record(piece);
                        }
                    }
                    let minimum = (std::f32::consts::PI * radius / recipe.pitch).floor() as usize;
                    assert!(coverage.published_pieces().len() >= minimum);
                }
            }
        }
    }
    #[test]
    fn compression_is_checked_on_real_inset_tiles_not_the_planning_radius() {
        let recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42);
        for (radius, count) in [(0.12, 3), (0.10, 3), (0.09, 3), (0.07, 0), (0.06, 0)] {
            let angle: f32 = recipe.pitch / radius;
            let mut row = PathSample::straight(Vec3::ZERO, Vec3::X, 0., 0, recipe.pitch);
            row.entry.center = Vec3::ZERO;
            row.exit.center = Vec3::new(radius * angle.sin(), 0., radius * (1. - angle.cos()));
            row.exit.across = Vec3::new(angle.sin(), 0., -angle.cos());
            let pieces = recipe.pieces(row);
            assert_eq!(pieces.len(), count, "radius={radius}");
            assert!(pieces.iter().all(|p| resolved_footprint(p.footprint)));
        }
    }

    #[test]
    fn impossible_corner_is_explicit_and_never_produces_a_folded_cell() {
        let recipe = PavementRecipe::new(PavementKind::Slabs, 0.12, 0.05, 42);
        let mut stroke = PathStroke::new(recipe.path_settings(), Vec3::ONE, Vec3::X).unwrap();
        assert!(stroke.push(Vec3::ONE + Vec3::X * 0.3));
        assert!(stroke.push(Vec3::ONE));
        assert!(stroke.finish());
        assert!(stroke.rows.iter().any(|r| !r.valid));
        for row in stroke.rows {
            let pieces = recipe.pieces(row);
            if !row.valid {
                assert!(pieces.is_empty());
            }
            for piece in pieces {
                assert!(piece.footprint.iter().all(|p| p.is_finite()));
                for i in 0..4 {
                    assert!(
                        (piece.footprint[i] - piece.footprint[(i + 3) % 4])
                            .perp_dot(piece.footprint[(i + 1) % 4] - piece.footprint[i])
                            > 0.
                    );
                }
            }
        }
    }
    #[test]
    fn invalid_input_breaks_at_the_seam_and_queue_is_bounded() {
        let mut stroke = PathStroke::new(
            recipe(PavementKind::Brick).path_settings(),
            Vec3::ZERO,
            Vec3::Z,
        )
        .unwrap();
        assert!(!stroke.push(Vec3::splat(f32::NAN)));
        assert!(!stroke.push(Vec3::X * 100.));
        assert!(stroke.rows.len() <= MAX_QUEUED_ROWS);
        assert!(PathStroke::new(
            recipe(PavementKind::Brick).path_settings(),
            Vec3::NAN,
            Vec3::Y
        )
        .is_none());
        let recipe = PavementRecipe::new(PavementKind::Slabs, f32::INFINITY, f32::NAN, 0);
        assert_eq!(recipe.width, 0.12);
        assert_eq!(recipe.pitch, 0.05);
    }
}
