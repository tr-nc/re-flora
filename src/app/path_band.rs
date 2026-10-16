//! Shared distance-sampled path band. No material, renderer or world ownership.
//! Consumers turn these shared course sections into paving or vertically layered walls.
use glam::{Vec2, Vec3};
use std::{collections::VecDeque, sync::Arc};
mod adaptive;
mod closure;
mod curve;
mod intent;
#[cfg(test)]
mod recovery_tests;
use curve::CurveGuide;
pub(super) use intent::PathIntent;
pub(super) const VOXEL_SIZE: f32 = 1. / 256.;
pub(super) const MAX_QUEUED_ROWS: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathSampling {
    Courses,
    Surface,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct PathSettings {
    pub width: f32,
    pub pitch: f32,
    pub sampling: PathSampling,
}
fn xz(point: Vec3) -> Vec2 {
    Vec2::new(point.x, point.z)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CourseSection {
    pub center: Vec3,
    pub across: Vec3,
    continuity: u32,
}
impl CourseSection {
    fn new(center: Vec3, forward: Vec3, continuity: u32) -> Self {
        Self {
            center,
            across: Vec3::new(forward.z, 0., -forward.x),
            continuity,
        }
    }
    pub(super) fn point(self, offset: f32) -> Vec2 {
        xz(self.center + self.across * offset)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PathSample {
    pub center: Vec3,
    pub forward: Vec3,
    pub distance: f32,
    /// Actual terminal station; merged courses do not have nominal pitch length.
    pub end_distance: f32,
    pub index: u32,
    pub entry: CourseSection,
    pub exit: CourseSection,
    /// Final boundary shares node zero, including masonry head-gap identity.
    pub closes_loop: bool,
    pub valid: bool,
}
impl PathSample {
    pub(super) fn straight(
        center: Vec3,
        forward: Vec3,
        distance: f32,
        index: u32,
        pitch: f32,
    ) -> Self {
        Self {
            center,
            forward,
            distance,
            end_distance: distance + pitch * 0.5,
            index,
            closes_loop: false,
            valid: true,
            entry: CourseSection::new(center - forward * pitch * 0.5, forward, 0),
            exit: CourseSection::new(center + forward * pitch * 0.5, forward, 0),
        }
    }
}
pub(super) fn contains_polygon(polygon: &[Vec2], point: Vec2, padding: f32) -> bool {
    (0..polygon.len()).all(|i| {
        let edge = polygon[(i + 1) % polygon.len()] - polygon[i];
        edge.perp_dot(point - polygon[i]) >= -padding * edge.length() - 1e-8
    })
}
pub(super) fn inset_quad(points: [Vec2; 4], inset: f32) -> Option<[Vec2; 4]> {
    inset_quad_edges(points, [inset; 4])
}
/// Start, right, end, left physical insets. A zero internal head inset lets
/// adjacent closed cell primitives bake to one continuous masonry block.
pub(super) fn inset_quad_edges(points: [Vec2; 4], insets: [f32; 4]) -> Option<[Vec2; 4]> {
    let mut output = [Vec2::ZERO; 4];
    for i in 0..4 {
        let previous = points[i] - points[(i + 3) % 4];
        let next = points[(i + 1) % 4] - points[i];
        if !points[i].is_finite() || previous.perp_dot(next) <= 1e-9 {
            return None;
        }
        let a = Vec2::new(-previous.y, previous.x).normalize();
        let b = Vec2::new(-next.y, next.x).normalize();
        let det = a.perp_dot(b);
        let da = insets[(i + 3) % 4];
        let db = insets[i];
        output[i] = points[i] + Vec2::new(da * b.y - a.y * db, a.x * db - da * b.x) / det;
    }
    if output
        .iter()
        .any(|p| !p.is_finite() || !contains_polygon(&points, *p, 0.))
    {
        return None;
    }
    for i in 0..4 {
        if (output[i] - output[(i + 3) % 4]).perp_dot(output[(i + 1) % 4] - output[i]) <= 1e-9 {
            return None;
        }
    }
    Some(output)
}

#[derive(Clone)]
pub(super) struct PathStroke {
    pub config: PathSettings,
    input: StrokeInput,
    pub rows: VecDeque<PathSample>,
    course_fit: Option<Arc<dyn Fn(PathSample) -> bool + Send + Sync>>,
    smoothing_scale: Option<f32>,
}
#[derive(Clone)]
enum StrokeInput {
    Tiles(TileInput),
    Surface(SurfaceInput),
}
#[derive(Clone)]
struct TileInput {
    guide: CurveGuide,
    heading: Vec3,
    since_prepare: f64,
    next_distance: f64,
    distance_base: f64,
    next_index: u32,
    previous_exit: Option<CourseSection>,
    dirty: bool,
    finished: bool,
}
#[derive(Clone)]
struct SurfaceInput {
    last_input: Vec3,
    previous_grid: Option<Vec3>,
    pending_grid: Vec3,
    distance: f64,
    until_next: f64,
    heading: Vec3,
    next_index: u32,
    resumed: bool,
}
impl PathStroke {
    pub(super) fn new(config: PathSettings, start: Vec3, heading: Vec3) -> Option<Self> {
        if !start.is_finite()
            || !config.width.is_finite()
            || !config.pitch.is_finite()
            || config.width <= 0.
            || config.pitch <= 0.
        {
            return None;
        }
        let heading = Vec3::new(heading.x, 0., heading.z).normalize_or_zero();
        let heading = if heading == Vec3::ZERO {
            Vec3::Z
        } else {
            heading
        };
        let input = if config.sampling == PathSampling::Surface {
            StrokeInput::Surface(SurfaceInput {
                last_input: start,
                previous_grid: None,
                pending_grid: start,
                distance: 0.,
                until_next: f64::from(config.pitch),
                heading,
                next_index: 0,
                resumed: false,
            })
        } else {
            StrokeInput::Tiles(TileInput {
                guide: CurveGuide::new(start),
                heading,
                since_prepare: 0.,
                next_distance: 0.,
                distance_base: 0.,
                next_index: 0,
                previous_exit: None,
                dirty: true,
                finished: false,
            })
        };
        Some(Self {
            config,
            input,
            rows: VecDeque::new(),
            course_fit: None,
            smoothing_scale: None,
        })
    }
    /// Consumer owns material legality; the band owns partitioning and curve containment.
    pub(super) fn adaptive_courses(
        mut self,
        fit: impl Fn(PathSample) -> bool + Send + Sync + 'static,
        smoothing_scale: Option<f32>,
    ) -> Self {
        self.course_fit = Some(Arc::new(fit));
        self.smoothing_scale = smoothing_scale;
        self
    }
    fn curve(&self, input: &TileInput) -> curve::RoundedPath {
        match self.smoothing_scale {
            Some(scale) => input
                .guide
                .smoothed(self.config.width * scale, input.heading),
            None => input.guide.rounded(self.config.width, input.heading),
        }
    }
    fn mutable_support(&self) -> f64 {
        f64::from(self.config.width)
            * self
                .smoothing_scale
                .map_or(2., |scale| 4. * f64::from(scale))
    }
    pub(super) fn push(&mut self, point: Vec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        match &mut self.input {
            StrokeInput::Tiles(input) => {
                if input.finished {
                    return false;
                }
                let length = f64::from(xz(point - input.guide.last()).length());
                if !length.is_finite()
                    || ((input.since_prepare + length) / f64::from(self.config.pitch)).ceil()
                        > MAX_QUEUED_ROWS.saturating_sub(self.rows.len()) as f64
                {
                    return false;
                }
                if !input.guide.push(point) {
                    return false;
                }
                input.since_prepare += length;
                input.dirty |= length > 1e-8;
                true
            }
            StrokeInput::Surface(input) => {
                let start = input.last_input;
                let length = f64::from(xz(point - start).length());
                if !length.is_finite() {
                    return false;
                }
                if length <= 1e-8 {
                    return true;
                }
                let mut consumed = 0.;
                while consumed + input.until_next <= length + 1e-7 {
                    if self.rows.len() >= MAX_QUEUED_ROWS {
                        return false;
                    }
                    consumed += input.until_next;
                    let next = start.lerp(point, (consumed / length).min(1.) as f32);
                    if input.resumed {
                        input.distance += f64::from(self.config.pitch);
                    }
                    input.emit(next, self.config.pitch, &mut self.rows);
                    input.previous_grid = Some(input.pending_grid);
                    input.pending_grid = next;
                    if !input.resumed {
                        input.distance += f64::from(self.config.pitch);
                    }
                    input.until_next = f64::from(self.config.pitch);
                }
                input.until_next -= (length - consumed).max(0.);
                input.last_input = point;
                true
            }
        }
    }
    /// A proposal may change any UNPUBLISHED course, including queued ones.
    /// Seek from the world's actual checkpoint, never a generated queue tail.
    pub(super) fn replan_unpublished(&mut self, accepted: Option<PathSample>) {
        let StrokeInput::Tiles(input) = &mut self.input else {
            return; // Surface sampling has no mutable curve tail.
        };
        self.rows.clear();
        input.next_distance = accepted.map_or(0., |r| {
            if self.course_fit.is_some() {
                f64::from(r.end_distance) - input.distance_base + f64::from(self.config.pitch) * 0.5
            } else {
                f64::from(r.distance) - input.distance_base + f64::from(self.config.pitch)
            }
        });
        input.next_index = accepted.map_or(0, |r| r.index.wrapping_add(1));
        input.previous_exit = accepted.map(|r| r.exit);
        input.since_prepare = 0.;
        input.dirty = true;
    }

    /// Compile only stable sections, preserving exact published exit facts.
    /// The short unbaked tail can still round a newly drawn corner.
    pub(super) fn prepare(&mut self) -> bool {
        if self.course_fit.is_some() {
            return self.prepare_adaptive();
        }
        let StrokeInput::Tiles(input) = &mut self.input else {
            return true;
        };
        if !input.dirty {
            return true;
        }
        let curve = input.guide.rounded(self.config.width, input.heading);
        let limit = if input.finished {
            curve.length
        } else {
            curve.length - 2. * f64::from(self.config.width) - f64::from(self.config.pitch) * 0.5
        };
        while input.next_distance <= limit + 1e-7 {
            if self.rows.len() >= MAX_QUEUED_ROWS {
                return false;
            }
            let half = f64::from(self.config.pitch) * 0.5;
            let middle = curve.section(input.next_distance);
            let entry = input
                .previous_exit
                .unwrap_or_else(|| curve.section(input.next_distance - half));
            let exit = curve.section(input.next_distance + half);
            let forward = Vec3::new(-middle.across.z, 0., middle.across.x);
            self.rows.push_back(PathSample {
                center: middle.center,
                forward,
                distance: (input.distance_base + input.next_distance) as f32,
                end_distance: (input.distance_base + input.next_distance + half) as f32,
                index: input.next_index,
                closes_loop: false,
                valid: entry.continuity == exit.continuity
                    && course_fits(entry, exit, self.config.width),
                entry,
                exit,
            });
            input.previous_exit = Some(exit);
            input.next_index = input.next_index.wrapping_add(1);
            input.next_distance += f64::from(self.config.pitch);
        }
        input.since_prepare = 0.;
        input.dirty = false;
        true
    }
    /// Queued and unbaked cells for a consumer's transient preview; never world state.
    pub(super) fn pending_rows(&self) -> Vec<PathSample> {
        let StrokeInput::Tiles(input) = &self.input else {
            return Vec::new();
        };
        if input.finished {
            return self.rows.iter().copied().collect();
        }
        let curve = self.curve(input);
        let mut rows: Vec<_> = self.rows.iter().copied().collect();
        if self.course_fit.is_some() {
            rows.extend(self.adaptive_tail(input, &curve));
            return rows;
        }
        let mut distance = input.next_distance;
        let mut index = input.next_index;
        let mut previous = input.previous_exit;
        for _ in 0..MAX_QUEUED_ROWS {
            if distance > curve.length + 1e-7 {
                break;
            }
            let half = f64::from(self.config.pitch) * 0.5;
            let middle = curve.section(distance);
            let entry = previous.unwrap_or_else(|| curve.section(distance - half));
            let exit = curve.section(distance + half);
            rows.push(PathSample {
                center: middle.center,
                forward: Vec3::new(-middle.across.z, 0., middle.across.x),
                distance: (input.distance_base + distance) as f32,
                end_distance: (input.distance_base + distance + half) as f32,
                index,
                entry,
                exit,
                closes_loop: false,
                valid: entry.continuity == exit.continuity
                    && course_fits(entry, exit, self.config.width),
            });
            previous = Some(exit);
            distance += f64::from(self.config.pitch);
            index = index.wrapping_add(1);
        }
        rows
    }
    /// Replace ONLY the unbaked compiler tail. Global course IDs/distances and
    /// the exact accepted exit survive; this is not a new user stroke.
    pub(super) fn replace_tail(&mut self, accepted: Option<PathSample>, points: &[Vec3]) -> bool {
        if points.is_empty() || points.iter().any(|p| !p.is_finite()) {
            return false;
        }
        let (anchor, heading) = accepted.map_or(
            (
                points[0],
                match &self.input {
                    StrokeInput::Tiles(i) => i.heading,
                    StrokeInput::Surface(i) => i.heading,
                },
            ),
            |r| {
                if self.config.sampling == PathSampling::Courses {
                    (
                        r.exit.center,
                        Vec3::new(-r.exit.across.z, 0., r.exit.across.x),
                    )
                } else {
                    (r.center, r.forward)
                }
            },
        );
        self.rows.clear();
        self.input = if self.config.sampling == PathSampling::Courses {
            StrokeInput::Tiles(TileInput {
                guide: CurveGuide::new(anchor),
                heading,
                since_prepare: 0.,
                next_distance: if accepted.is_some() {
                    f64::from(self.config.pitch) * 0.5
                } else {
                    0.
                },
                distance_base: accepted.map_or(0., |r| {
                    if self.course_fit.is_some() {
                        f64::from(r.end_distance)
                    } else {
                        // Preserve the original uniform wall course arithmetic.
                        f64::from(r.distance) + f64::from(self.config.pitch) * 0.5
                    }
                }),
                next_index: accepted.map_or(0, |r| r.index.wrapping_add(1)),
                previous_exit: accepted.map(|r| r.exit),
                dirty: true,
                finished: false,
            })
        } else {
            StrokeInput::Surface(SurfaceInput {
                last_input: anchor,
                previous_grid: accepted.map(|r| r.center),
                pending_grid: anchor,
                distance: accepted.map_or(0., |r| f64::from(r.distance)),
                until_next: f64::from(self.config.pitch),
                heading,
                next_index: accepted.map_or(0, |r| r.index.wrapping_add(1)),
                resumed: accepted.is_some(),
            })
        };
        for point in &points[1..] {
            if !self.push(*point) {
                return false;
            }
        }
        true
    }

    pub(super) fn finish(&mut self) -> bool {
        match &mut self.input {
            StrokeInput::Tiles(input) => {
                if input.finished {
                    return true;
                }
                input.finished = true;
                input.dirty = true;
                self.prepare()
            }
            StrokeInput::Surface(input) => {
                if !input.resumed {
                    input.emit(input.last_input, self.config.pitch, &mut self.rows);
                }
                if xz(input.last_input - input.pending_grid).length() > 1e-5 {
                    input.pending_grid = input.last_input;
                    input.distance += f64::from(self.config.pitch) - input.until_next;
                    input.emit(input.last_input, self.config.pitch, &mut self.rows);
                }
                true
            }
        }
    }
}
// Validate the actual strip, including its immutable entry after publication.
// Radius and angle are planning choices; strict convexity is a geometric fact.
fn course_fits(entry: CourseSection, exit: CourseSection, width: f32) -> bool {
    let half = width * 0.5;
    inset_quad(
        [
            entry.point(-half),
            entry.point(half),
            exit.point(half),
            exit.point(-half),
        ],
        0.,
    )
    .is_some()
}
impl SurfaceInput {
    fn emit(&mut self, next: Vec3, pitch: f32, rows: &mut VecDeque<PathSample>) {
        let delta = next - self.previous_grid.unwrap_or(self.pending_grid);
        let forward = Vec3::new(delta.x, 0., delta.z).normalize_or_zero();
        if forward != Vec3::ZERO {
            self.heading = forward;
        }
        rows.push_back(PathSample::straight(
            if self.resumed {
                next
            } else {
                self.pending_grid
            },
            self.heading,
            self.distance as f32,
            self.next_index,
            pitch,
        ));
        self.next_index = self.next_index.wrapping_add(1);
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PathCell {
    pub center: Vec3,
    pub forward: Vec3,
    pub size: Vec3,
    pub seed: u32,
    /// CCW XZ planform, already inset for joints. Mesh, support and coverage share it.
    pub footprint: [Vec2; 4],
}
impl PathCell {
    #[cfg(test)]
    pub(super) fn straight(center: Vec3, forward: Vec3, size: Vec3, seed: u32) -> Self {
        let across = Vec3::new(forward.z, 0., -forward.x);
        Self {
            center,
            forward,
            size,
            seed,
            footprint: [
                xz(center - across * size.x * 0.5 - forward * size.z * 0.5),
                xz(center + across * size.x * 0.5 - forward * size.z * 0.5),
                xz(center + across * size.x * 0.5 + forward * size.z * 0.5),
                xz(center - across * size.x * 0.5 + forward * size.z * 0.5),
            ],
        }
    }
    pub(super) fn point(self, u: f32, v: f32) -> Vec2 {
        self.footprint[0]
            .lerp(self.footprint[1], u)
            .lerp(self.footprint[3].lerp(self.footprint[2], u), v)
    }
}

pub(super) fn overlaps_quads(a: [Vec2; 4], b: [Vec2; 4], clearance: f32) -> bool {
    [a, b].iter().all(|polygon| {
        (0..4).all(|i| {
            let edge = polygon[(i + 1) % 4] - polygon[i];
            let axis = Vec2::new(-edge.y, edge.x).normalize();
            let interval = |foot: &[Vec2; 4]| {
                foot.iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), p| {
                        let dot = p.dot(axis);
                        (min.min(dot), max.max(dot))
                    })
            };
            let (amin, amax) = interval(&a);
            let (bmin, bmax) = interval(&b);
            amin < bmax + clearance && bmin < amax + clearance
        })
    })
}
/// Indexed closed prism over a cell. XZ stays in the authoritative footprint.
/// A top across-scale tapers earth without shortening the shared head sections.
pub(super) struct CellSolid {
    pub positions: [Vec3; 8],
    pub indices: [usize; 36],
}
impl CellSolid {
    pub(super) fn new(
        foot: [Vec2; 4],
        ground: [f32; 4],
        lower: f32,
        upper: f32,
        top_scale: f32,
    ) -> Self {
        let mut positions = [Vec3::ZERO; 8];
        for i in 0..4 {
            let midpoint = if i < 2 {
                (foot[0] + foot[1]) * 0.5
            } else {
                (foot[2] + foot[3]) * 0.5
            };
            let top = midpoint + (foot[i] - midpoint) * top_scale;
            positions[i] = Vec3::new(foot[i].x, ground[i] + lower, foot[i].y);
            positions[i + 4] = Vec3::new(top.x, ground[i] + upper, top.y);
        }
        Self {
            positions,
            indices: [
                0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2,
                7, 3, 3, 7, 4, 3, 4, 0,
            ],
        }
    }
    pub(super) fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices.chunks_exact(3).map(|t| {
            [
                self.positions[t[0]],
                self.positions[t[1]],
                self.positions[t[2]],
            ]
        })
    }
}
