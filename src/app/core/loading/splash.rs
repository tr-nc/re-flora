//! Approved B layout: a fine grid, a four-cell title and held ±10° flower poses.
//! Colored pixel quads are authored in model space and transformed as a whole.
use egui::{epaint::Mesh, Color32, Context, FontId, LayerId, Pos2, Rect, Vec2};
use std::time::Instant;

const CELL: f32 = 96.0;
// 25% more cells across the viewport: each cell is 20% smaller.
const GRID_DENSITY: f32 = 1.25;
const POSES: [f32; 4] = [-10.0, 0.0, 10.0, 0.0];
const STEP_SECONDS: f64 = 1.2;
const SWAY_PHASES: usize = 16;
const FLOWER_FADE_START_SECONDS: f64 = 0.4;
const FLOWER_FADE_DURATION_SECONDS: f64 = 0.7;
const FLOWER_STAGGER_SECONDS: f64 = 0.3;
const TRANSITION_END_SECONDS: f64 =
    FLOWER_FADE_START_SECONDS + FLOWER_FADE_DURATION_SECONDS + FLOWER_STAGGER_SECONDS;

#[derive(Clone, Copy)]
struct Palette {
    name: &'static str,
    // Background, grid, cream, yellow, flower center.
    colors: [u32; 5],
}

const PALETTES: [Palette; 3] = [
    Palette {
        name: "forest",
        colors: [0x233f32, 0x2b4937, 0xeee2b8, 0xd9b764, 0xa38846],
    },
    Palette {
        name: "moss",
        colors: [0x374833, 0x40513a, 0xe5ddbb, 0xc9ac65, 0x9b824a],
    },
    Palette {
        name: "pond",
        colors: [0x203e3c, 0x294845, 0xe0e4d1, 0xbebc75, 0x8c9259],
    },
];

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Keep the approved 10×6 composition square-scaled at the chosen density,
/// extending the grid at the edges instead of independently moving the title.
struct Layout {
    cell: f32,
    origin: Pos2,
    title: Rect,
}

impl Layout {
    fn new(viewport: Rect) -> Self {
        let cell =
            ((viewport.width() / 10.0).min(viewport.height() / 6.0) / GRID_DENSITY).max(0.01);
        let origin = viewport.center() - Vec2::new(5.0, 3.0) * cell;
        let title = Rect::from_min_size(
            origin + Vec2::new(3.0, 2.0) * cell,
            Vec2::new(4.0, 1.0) * cell,
        );
        Self {
            cell,
            origin,
            title,
        }
    }

    fn tile(&self, column: i32, row: i32) -> Rect {
        Rect::from_min_size(
            self.origin + Vec2::new(column as f32, row as f32) * self.cell,
            Vec2::splat(self.cell),
        )
    }
}

fn reserved(column: i32, row: i32) -> bool {
    (3..7).contains(&column) && row == 2
}

pub(crate) struct Splash {
    palette: Palette,
    flowers: [[Mesh; 4]; 2],
    started: Option<Instant>,
}

impl Default for Splash {
    fn default() -> Self {
        // Independent cosmetic randomness, selected once per loading owner;
        // never reseed or consume the world's procedural-generation RNG.
        Self::new(PALETTES[rand::random_range(0..PALETTES.len())])
    }
}

impl Splash {
    fn new(palette: Palette) -> Self {
        let flowers = std::array::from_fn(|kind| {
            let local = flower_mesh(kind, palette);
            std::array::from_fn(|frame| {
                let mut mesh = local.clone();
                mesh.rotate(
                    egui::emath::Rot2::from_angle(POSES[frame].to_radians()),
                    Pos2::ZERO,
                );
                mesh
            })
        });
        Self {
            palette,
            flowers,
            started: None,
        }
    }

    pub(super) fn show(&mut self, ctx: &Context, progress: f32) {
        self.paint(ctx, progress, LayerId::background(), Opacity::at(0.0));
    }

    fn paint(&mut self, ctx: &Context, progress: f32, layer: LayerId, opacity: Opacity) {
        if self.started.is_none() {
            log::info!("[LOADING][SPLASH] layout=B palette={} local_pixels=16 title_cells=4x1 motion=sway poses=-10,0,10,0 step_seconds=1.2 phases=16 phase_step_seconds=0.075", self.palette.name);
        }
        let seconds = self
            .started
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64();
        let viewport = ctx.viewport_rect();
        let layout = Layout::new(viewport);
        let scale = layout.cell / CELL;
        let [background, grid, cream, yellow, _] = self.palette.colors.map(rgb);
        let mut painter = ctx.layer_painter(layer).with_clip_rect(viewport);
        painter.set_opacity(opacity.background);
        painter.rect_filled(viewport, 0.0, background);
        let first = (viewport.min - layout.origin) / layout.cell;
        let last = (viewport.max - layout.origin) / layout.cell;
        let mut field = Mesh::default();
        for row in first.y.floor() as i32..last.y.ceil() as i32 {
            for column in first.x.floor() as i32..last.x.ceil() as i32 {
                if reserved(column, row) {
                    continue;
                }
                let tile = layout.tile(column, row);
                painter.rect_stroke(
                    tile.shrink(0.5 * scale),
                    0.0,
                    egui::Stroke::new(scale, grid),
                    egui::StrokeKind::Middle,
                );
                if (column + row).rem_euclid(3) == 0 {
                    // Quiet-cell marks share the grid's dark color and fade.
                    // Do not leave them floating over the scene with late flowers.
                    for (offset, size) in [
                        (Vec2::new(46.0, 47.0), Vec2::new(4.0, 2.0)),
                        (Vec2::new(48.0, 44.0), Vec2::new(2.0, 3.0)),
                    ] {
                        field.add_colored_rect(
                            Rect::from_min_size(tile.min + offset * scale, size * scale),
                            grid.gamma_multiply(opacity.background),
                        );
                    }
                    continue;
                }
                let kind = (column + row).rem_euclid(2) as usize;
                let group = sway_group(column, row);
                append_at(
                    &mut field,
                    &self.flowers[kind][sway_frame(group, seconds)],
                    tile.center(),
                    3.0 * scale,
                    opacity.flower(column, row),
                );
            }
        }
        // The title is part of the grid: no white backing and no interior lines.
        painter.rect_stroke(
            layout.title.shrink(0.5 * scale),
            0.0,
            egui::Stroke::new(scale, grid),
            egui::StrokeKind::Middle,
        );
        // Field colors already include their own premultiplied alpha (grid fade
        // for quiet marks, staggered fade for flowers); do not fade them again.
        painter.set_opacity(1.0);
        painter.add(egui::Shape::mesh(field));
        painter.set_opacity(opacity.foreground);
        let title = painter.layout_no_wrap(
            "re: flora".to_owned(),
            FontId::proportional(64.0 * scale),
            cream,
        );
        let (title_pos, underline) = title_and_underline(layout.title, title.mesh_bounds, scale);
        painter.galley(title_pos, title, cream);
        painter.rect_filled(underline, 0.0, grid);
        painter.rect_filled(
            Rect::from_min_size(
                underline.min,
                Vec2::new(
                    underline.width() * progress.clamp(0.0, 1.0),
                    underline.height(),
                ),
            ),
            0.0,
            yellow,
        );
    }
}

/// Background/grid and title/underline fade together during [0, 1] seconds.
/// Only flowers are delayed, with independently staggered starts.
/// No position or scale changes.
#[derive(Clone, Copy, Debug)]
struct Opacity {
    background: f32,
    foreground: f32,
    flower_fade: Option<(u64, f64)>,
}

fn fade(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0) as f32;
    1.0 - t * t * (3.0 - 2.0 * t)
}

impl Opacity {
    fn at(seconds: f64) -> Self {
        Self {
            background: fade(seconds),
            foreground: fade(seconds),
            flower_fade: None,
        }
    }

    fn flower(self, column: i32, row: i32) -> f32 {
        match self.flower_fade {
            Some((seed, seconds)) => fade(
                (seconds - FLOWER_FADE_START_SECONDS - flower_delay(seed, column, row))
                    / FLOWER_FADE_DURATION_SECONDS,
            ),
            None => self.foreground,
        }
    }
}

/// Hash a fixed per-transition seed and logical cell coordinates. This keeps a
/// flower's timing stable across repaints/resizes, including negative edge cells.
fn flower_delay(seed: u64, column: i32, row: i32) -> f64 {
    let cell = ((column as u32 as u64) << 32) | row as u32 as u64;
    let mut hash = (seed ^ cell).wrapping_add(0x9e3779b97f4a7c15);
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
    hash ^= hash >> 31;
    (hash >> 11) as f64 / (1u64 << 53) as f64 * FLOWER_STAGGER_SECONDS
}

/// Moves the same splash above the real game render after loading completes.
/// Start the clock only after the first game frame has successfully presented,
/// so expensive first-frame GPU initialization cannot consume the whole fade.
pub(crate) struct Transition {
    splash: Splash,
    flower_seed: u64,
    first_presented: Option<Instant>,
    clear_frame_drawn: bool,
}

impl Transition {
    pub(crate) fn new(splash: Splash) -> Self {
        Self {
            splash,
            flower_seed: rand::random(),
            first_presented: None,
            clear_frame_drawn: false,
        }
    }

    pub(crate) fn show(&mut self, ctx: &Context, visible: bool) {
        let seconds = self
            .first_presented
            .map_or(0.0, |start| start.elapsed().as_secs_f64());
        let opacity = Opacity {
            flower_fade: Some((self.flower_seed, seconds)),
            ..Opacity::at(seconds)
        };
        self.clear_frame_drawn = seconds >= TRANSITION_END_SECONDS;
        // Automated scene captures must keep receiving the scene, not an intro.
        if visible && !self.clear_frame_drawn {
            self.splash.paint(
                ctx,
                1.0,
                LayerId::new(egui::Order::Tooltip, egui::Id::new("splash_transition")),
                opacity,
            );
        }
    }

    pub(crate) fn needs_first_frame_completion(&self) -> bool {
        self.first_presented.is_none()
    }

    /// The first callback requires its GPU frame fence to have completed.
    /// Return true only after a fully transparent overlay frame was presented.
    pub(crate) fn presented(&mut self) -> bool {
        if self.first_presented.is_none() {
            self.first_presented = Some(Instant::now());
            log::info!(
                "[LOADING][SPLASH_TRANSITION] started background=0..1s title=0..1s flowers={}..{}s fade_duration={}s random_delay=0..{}s",
                FLOWER_FADE_START_SECONDS,
                TRANSITION_END_SECONDS,
                FLOWER_FADE_DURATION_SECONDS,
                FLOWER_STAGGER_SECONDS,
            );
        }
        if self.clear_frame_drawn {
            log::info!("[LOADING][SPLASH_TRANSITION] complete");
        }
        self.clear_frame_drawn
    }
}

/// Center the wordmark and its progress underline as one unit. Use visible ink
/// bounds, not font advance width, so both ends align with the actual letters.
fn title_and_underline(region: Rect, ink: Rect, scale: f32) -> (Pos2, Rect) {
    let gap = 10.0 * scale;
    let height = 3.0 * scale;
    let top = region.center() - Vec2::new(ink.width(), ink.height() + gap + height) * 0.5;
    let text_pos = top - ink.min.to_vec2();
    let underline = Rect::from_min_size(
        top + Vec2::new(0.0, ink.height() + gap),
        Vec2::new(ink.width(), height),
    );
    (text_pos, underline)
}

// Spread pose changes evenly across one hold period, independently of the
// four cached poses. Logical cell coordinates keep phases stable on repaint.
fn sway_group(column: i32, row: i32) -> usize {
    (column + row * 7).rem_euclid(SWAY_PHASES as i32) as usize
}

fn sway_frame(group: usize, seconds: f64) -> usize {
    let phase = group as f64 * STEP_SECONDS / SWAY_PHASES as f64;
    ((seconds + phase) / STEP_SECONDS).floor() as usize % POSES.len()
}

fn append_at(target: &mut Mesh, source: &Mesh, center: Pos2, scale: f32, alpha: f32) {
    let offset = target.vertices.len() as u32;
    target
        .indices
        .extend(source.indices.iter().map(|index| offset + index));
    target
        .vertices
        .extend(source.vertices.iter().map(|vertex| egui::epaint::Vertex {
            pos: center + vertex.pos.to_vec2() * scale,
            color: vertex.color.gamma_multiply(alpha),
            ..*vertex
        }));
}

/// Port of the approved study's 16px silhouettes, not borrowed reference assets.
/// Flat unit-square pixels avoid texture filtering or angle-dependent resampling.
fn flower_mesh(kind: usize, palette: Palette) -> Mesh {
    let (petals, length, breadth, radius) = if kind == 0 {
        (5, 2.9, 2.2, 3.9)
    } else {
        (6, 2.8, 1.85, 3.8)
    };
    let [_, _, cream, yellow, center] = palette.colors.map(rgb);
    let mut pixels = [None; 16 * 16];
    for i in 0..petals {
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / petals as f64;
        let (sin, cos) = angle.sin_cos();
        for y in 0..16 {
            for x in 0..16 {
                let dx = x as f64 - 7.5;
                let dy = y as f64 - 7.5;
                let u = dx * cos + dy * sin;
                let v = -dx * sin + dy * cos;
                if ((u - radius) / length).powi(2) + (v / breadth).powi(2) <= 1.0 {
                    pixels[y * 16 + x] = Some(if kind == 0 { yellow } else { cream });
                }
            }
        }
    }
    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f64 - 7.5;
            let dy = y as f64 - 7.5;
            if dx * dx + dy * dy <= 3.0 {
                pixels[y * 16 + x] = Some(if dx + dy < 0.0 { yellow } else { center });
            }
        }
    }
    let mut mesh = Mesh::default();
    for (index, color) in pixels.into_iter().enumerate() {
        if let Some(color) = color {
            mesh.add_colored_rect(
                Rect::from_min_size(
                    egui::pos2((index % 16) as f32 - 8.0, (index / 16) as f32 - 8.0),
                    Vec2::splat(1.0),
                ),
                color,
            );
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flower_fades_are_stable_varied_and_finish_after_the_title() {
        let mut delays = Vec::new();
        for row in -3..6 {
            for column in -3..10 {
                let delay = flower_delay(42, column, row);
                assert_eq!(delay, flower_delay(42, column, row));
                assert!((0.0..FLOWER_STAGGER_SECONDS).contains(&delay));
                delays.push(delay);
                let alpha = |seconds| {
                    Opacity {
                        flower_fade: Some((42, seconds)),
                        ..Opacity::at(seconds)
                    }
                    .flower(column, row)
                };
                assert_eq!(alpha(FLOWER_FADE_START_SECONDS), 1.0);
                assert!(
                    (alpha(FLOWER_FADE_START_SECONDS + delay + FLOWER_FADE_DURATION_SECONDS * 0.5)
                        - 0.5)
                        .abs()
                        < 0.00001
                );
                assert!(alpha(1.0) > 0.0, "flowers still outlive the title fade");
                assert_eq!(alpha(TRANSITION_END_SECONDS), 0.0);
                assert_eq!(alpha(1.4), 0.0, "flower fade must wrap up promptly");
                let mut previous = 1.0;
                for frame in 0..=160 {
                    let current = alpha(frame as f64 / 60.0);
                    assert!(current <= previous);
                    previous = current;
                }
            }
        }
        let low = delays.iter().copied().fold(f64::INFINITY, f64::min);
        let high = delays.iter().copied().fold(0.0, f64::max);
        assert!(high - low > FLOWER_STAGGER_SECONDS * 0.66);
        assert_ne!(flower_delay(42, 0, 0), flower_delay(43, 0, 0));
    }

    #[test]
    fn quiet_cell_marks_fade_with_the_grid_not_the_flowers() {
        let ctx = Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 576.0));
        let layout = Layout::new(viewport);
        let mark_pos = layout.tile(0, 0).min + Vec2::new(46.0, 47.0) * (layout.cell / CELL);
        let mut splash = Splash::new(PALETTES[0]);
        for seconds in [0.0, 0.5, 1.0, 1.5] {
            let opacity = Opacity {
                flower_fade: Some((42, seconds)),
                ..Opacity::at(seconds)
            };
            let input = egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                splash.paint(ui.ctx(), 1.0, LayerId::background(), opacity);
            });
            let mark = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => mesh
                        .vertices
                        .iter()
                        .find(|vertex| vertex.pos.distance(mark_pos) < 0.001),
                    _ => None,
                })
                .expect("quiet cell mark must be present in the painted field");
            assert_eq!(
                mark.color,
                rgb(PALETTES[0].colors[1]).gamma_multiply(opacity.background),
                "dark grid decorations must not linger over the revealed scene at {seconds}s"
            );
        }
    }

    #[test]
    fn flower_alpha_is_baked_without_modifying_cached_local_pixels() {
        let flower = flower_mesh(0, PALETTES[0]);
        let mut field = Mesh::default();
        append_at(&mut field, &flower, Pos2::ZERO, 1.0, 0.5);
        for (source, faded) in flower.vertices.iter().zip(&field.vertices) {
            assert_eq!(source.color.a(), 255);
            assert_eq!(faded.color, source.color.gamma_multiply(0.5));
            assert_eq!(source.pos, faded.pos);
        }
    }

    #[test]
    fn transition_fades_background_and_title_together() {
        for (time, background, foreground) in [
            (0.0, 1.0, 1.0),
            (0.5, 0.5, 0.5),
            (1.0, 0.0, 0.0),
            (1.5, 0.0, 0.0),
            (2.0, 0.0, 0.0),
            (10.0, 0.0, 0.0),
        ] {
            let opacity = Opacity::at(time);
            assert!((opacity.background - background).abs() < 0.00001);
            assert!((opacity.foreground - foreground).abs() < 0.00001);
        }
        let mut previous = Opacity::at(0.0);
        for frame in 1..=120 {
            let current = Opacity::at(frame as f64 / 60.0);
            assert!(current.background <= previous.background);
            assert!(current.foreground <= previous.foreground);
            assert_eq!(current.foreground, current.background);
            let flowers = Opacity {
                flower_fade: Some((42, frame as f64 / 60.0)),
                ..current
            };
            if frame as f64 / 60.0 <= FLOWER_FADE_START_SECONDS {
                assert_eq!(flowers.flower(0, 0), 1.0);
            }
            previous = current;
        }
    }

    #[test]
    fn transition_retains_palette_and_waits_for_presented_frames() {
        let ctx = Context::default();
        let mut transition = Transition::new(Splash::new(PALETTES[1]));
        let draw = |transition: &mut Transition| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 576.0))),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| transition.show(ui.ctx(), true));
        };
        draw(&mut transition);
        assert!(transition.first_presented.is_none());
        assert!(!transition.clear_frame_drawn);
        assert!(!transition.presented());
        assert!(transition.first_presented.is_some());
        assert_eq!(transition.splash.palette.name, "moss");
        transition.first_presented = Some(Instant::now() - std::time::Duration::from_millis(1200));
        draw(&mut transition);
        assert!(
            !transition.clear_frame_drawn,
            "late flowers must outlive the title fade"
        );
        transition.first_presented = Some(Instant::now() - std::time::Duration::from_secs(3));
        // Time alone does not end the transition before a clear frame is drawn.
        assert!(!transition.presented());
        draw(&mut transition);
        assert!(transition.clear_frame_drawn);
        assert!(transition.presented());
    }

    #[test]
    fn denser_grid_shrinks_cells_without_changing_title_alignment() {
        let layout = Layout::new(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 576.0)));
        assert!((layout.cell - 76.8).abs() < 0.00001);
        assert!((layout.title.width() - 4.0 * layout.cell).abs() < 0.00001);
        // f32 layout arithmetic may differ by one ULP at viewport coordinates.
        assert!((layout.title.center().x - 480.0).abs() < 0.001);
    }

    #[test]
    fn sway_holds_four_frames_and_staggers_groups() {
        for (frame, angle) in [-10.0, 0.0, 10.0, 0.0].into_iter().enumerate() {
            let time = frame as f64 * STEP_SECONDS + 0.01;
            assert_eq!(POSES[sway_frame(0, time)], angle);
            assert_eq!(sway_frame(0, time), sway_frame(0, time + 0.8));
            assert_eq!(
                sway_frame(0, time),
                sway_frame(0, time + 4.0 * STEP_SECONDS)
            );
        }
        assert_ne!(sway_frame(0, 0.1), sway_frame(SWAY_PHASES - 1, 0.1));
    }

    #[test]
    fn visible_flowers_spread_pose_updates_across_the_hold_period() {
        for size in [
            Vec2::new(960.0, 576.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(3440.0, 1440.0),
            Vec2::new(390.0, 844.0),
        ] {
            let viewport = Rect::from_min_size(Pos2::ZERO, size);
            let layout = Layout::new(viewport);
            let first = (viewport.min - layout.origin) / layout.cell;
            let last = (viewport.max - layout.origin) / layout.cell;
            let mut groups = [false; SWAY_PHASES];
            for row in first.y.floor() as i32..last.y.ceil() as i32 {
                for column in first.x.floor() as i32..last.x.ceil() as i32 {
                    if reserved(column, row) || (column + row).rem_euclid(3) == 0 {
                        continue;
                    }
                    // Count only fully visible flowers, not clipped edge cells.
                    if viewport.contains_rect(layout.tile(column, row)) {
                        groups[sway_group(column, row)] = true;
                    }
                }
            }
            let mut changes = Vec::new();
            for (group, visible) in groups.into_iter().enumerate() {
                if !visible {
                    continue;
                }
                let change = STEP_SECONDS - group as f64 * STEP_SECONDS / SWAY_PHASES as f64;
                assert_ne!(
                    sway_frame(group, change - 0.0001),
                    sway_frame(group, change + 0.0001)
                );
                changes.push(change);
            }
            changes.sort_by(f64::total_cmp);
            assert!(!changes.is_empty());
            let mut previous = changes[changes.len() - 1] - STEP_SECONDS;
            for change in changes {
                assert!(change - previous <= 0.151, "pose update gap at {size:?}");
                previous = change;
            }
        }
    }

    #[test]
    fn title_stays_on_the_grid_at_all_aspect_ratios() {
        for size in [
            Vec2::new(960.0, 576.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(3440.0, 1440.0),
            Vec2::new(390.0, 844.0),
        ] {
            let viewport = Rect::from_min_size(egui::pos2(13.0, 27.0), size);
            let layout = Layout::new(viewport);
            assert!(viewport.contains_rect(layout.title));
            for point in [layout.title.min, layout.title.max] {
                let cell = (point - layout.origin) / layout.cell;
                assert!((cell.x - cell.x.round()).abs() < 0.00001);
                assert!((cell.y - cell.y.round()).abs() < 0.00001);
            }
            assert!((layout.title.width() / layout.cell - 4.0).abs() < 0.00001);
            assert!((layout.title.height() / layout.cell - 1.0).abs() < 0.00001);
            let scale = layout.cell / CELL;
            let ink =
                Rect::from_min_size(egui::pos2(2.0, 7.0) * scale, Vec2::new(250.0, 48.0) * scale);
            let (text_pos, underline) = title_and_underline(layout.title, ink, scale);
            let positioned_ink = ink.translate(text_pos.to_vec2());
            assert!(layout.title.contains_rect(positioned_ink));
            assert!(layout.title.contains_rect(underline));
            assert!((underline.min.x - positioned_ink.min.x).abs() < 0.001);
            assert!((underline.max.x - positioned_ink.max.x).abs() < 0.001);
            assert!(underline.min.y > positioned_ink.max.y);
            assert!(
                (positioned_ink.union(underline).center() - layout.title.center()).length() < 0.001
            );
        }
        assert!(
            (0..6)
                .flat_map(|r| (0..10).map(move |c| reserved(c, r)))
                .filter(|r| *r)
                .count()
                == 4
        );
    }

    #[test]
    fn all_palettes_have_stable_local_pixel_geometry_and_rigid_poses() {
        for palette in PALETTES {
            let splash = Splash::new(palette);
            assert_eq!(splash.palette.colors, palette.colors);
            for kind in 0..2 {
                let original = flower_mesh(kind, palette);
                assert!(!original.is_empty());
                assert_eq!(
                    original.vertices.len(),
                    flower_mesh(kind, PALETTES[0]).vertices.len()
                );
                for rotated in &splash.flowers[kind] {
                    assert_eq!(original.indices, rotated.indices);
                    for (before, after) in original.vertices.iter().zip(&rotated.vertices) {
                        assert_eq!(before.color, after.color);
                        assert!(
                            (before.pos.to_vec2().length() - after.pos.to_vec2().length()).abs()
                                < 0.00001
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn batching_keeps_flower_indices_valid_after_scaling() {
        let mut field = Mesh::default();
        for kind in 0..2 {
            append_at(
                &mut field,
                &flower_mesh(kind, PALETTES[0]),
                egui::pos2(100.0 * kind as f32, 50.0),
                3.0,
                1.0,
            );
        }
        assert!(field.is_valid());
        assert_eq!(
            field.vertices.len(),
            flower_mesh(0, PALETTES[0]).vertices.len() + flower_mesh(1, PALETTES[0]).vertices.len()
        );
    }

    #[test]
    fn repaint_does_not_reselect_palette_or_rebuild_local_shapes() {
        let ctx = Context::default();
        for palette in PALETTES {
            let mut splash = Splash::new(palette);
            let vertex = splash.flowers[0][0].vertices[0];
            for progress in [0.0, 0.5, 1.0] {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 576.0))),
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| splash.show(ui.ctx(), progress));
                assert!(!output.shapes.is_empty());
                assert_eq!(splash.palette.colors, palette.colors);
                assert_eq!(splash.flowers[0][0].vertices[0].pos, vertex.pos);
                assert_eq!(splash.flowers[0][0].vertices[0].color, vertex.color);
            }
        }
    }
}
