//! Spatial filter ownership: recipe, footprint and dispatch must agree. Shaders
//! receive the exact sigma/radius used to expand the horizontal dependency halo.
use egui::Rect;

const FROST_POINTS: f32 = 7.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Region {
    pub origin: [u32; 2],
    pub extent: [u32; 2],
}

impl Region {
    fn between(min: [u32; 2], max: [u32; 2]) -> Self {
        Self {
            origin: min,
            extent: [max[0] - min[0], max[1] - min[1]],
        }
    }
}

pub(super) struct FilterPlan {
    pub sigma: f32,
    pub radius: u32,
    pub passes: [Region; 2],
}

impl FilterPlan {
    pub fn new(bounds: Rect, viewport: [u32; 2], scale: f32, cell: u32) -> Self {
        let sigma = FROST_POINTS * scale;
        let radius = (3.0 * sigma).ceil() as u32;
        let mut plan = Self {
            sigma,
            radius,
            passes: [Region::default(); 2],
        };
        if !bounds.is_positive() {
            return plan;
        }
        let min = [bounds.min.x, bounds.min.y];
        let max = [bounds.max.x, bounds.max.y];
        let mut lo = [0; 2];
        let mut hi = [0; 2];
        for axis in 0..2 {
            lo[axis] = (min[axis] * scale)
                .floor()
                .clamp(0.0, viewport[axis] as f32) as u32;
            hi[axis] = (max[axis] * scale).ceil().clamp(0.0, viewport[axis] as f32) as u32;
        }
        if (0..2).any(|a| hi[a] <= lo[a]) {
            return plan;
        }
        if cell == 0 {
            // Conservative one-texel support for the fragment's linear sampler,
            // including fractional geometry and pixel-center rounding.
            for a in 0..2 {
                lo[a] = lo[a].saturating_sub(1);
                hi[a] = hi[a].saturating_add(1).min(viewport[a]);
            }
            plan.passes = [
                Region::between(
                    [lo[0], lo[1].saturating_sub(radius)],
                    [hi[0], hi[1].saturating_add(radius).min(viewport[1])],
                ),
                Region::between(lo, hi),
            ];
        } else {
            let start = [lo[0] / cell, lo[1] / cell];
            let end = [hi[0].div_ceil(cell), hi[1].div_ceil(cell)];
            // Horizontal means are packed by cell-column/source-row; vertical
            // means by cell-column/cell-row. Read entire cells, not panel crops.
            plan.passes = [
                Region::between(
                    [start[0], start[1] * cell],
                    [end[0], end[1].saturating_mul(cell).min(viewport[1])],
                ),
                Region::between(start, end),
            ];
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contains(r: Region, x: u32, y: u32) -> bool {
        x >= r.origin[0]
            && y >= r.origin[1]
            && x < r.origin[0] + r.extent[0]
            && y < r.origin[1] + r.extent[1]
    }

    #[test]
    fn gaussian_dispatch_covers_every_sample_without_processing_the_viewport() {
        for scale in [0.75, 1.0, 1.75, 2.0] {
            let p = FilterPlan::new(
                Rect::from_min_max(egui::pos2(11.2, 20.3), egui::pos2(75.4, 52.8)),
                [5120, 2880],
                scale,
                0,
            );
            assert_eq!(p.sigma, 7.0 * scale);
            let [h, v] = p.passes;
            assert!(h.extent[0] * h.extent[1] + v.extent[0] * v.extent[1] < 5120 * 2880 / 100);
            for y in v.origin[1]..v.origin[1] + v.extent[1] {
                for x in v.origin[0]..v.origin[0] + v.extent[0] {
                    for dy in -(p.radius as i32)..=p.radius as i32 {
                        let sy = (y as i32 + dy).clamp(0, 2879) as u32;
                        assert!(
                            contains(h, x, sy),
                            "vertical read must never use stale horizontal pixels"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pixel_cells_include_pixels_outside_panel_bounds_and_crop_only_at_viewport() {
        let p = FilterPlan::new(
            Rect::from_min_max(egui::pos2(15.2, 17.3), egui::pos2(37.0, 29.0)),
            [37, 29],
            1.0,
            12,
        );
        assert_eq!(
            p.passes[0],
            Region {
                origin: [1, 12],
                extent: [3, 17]
            }
        );
        assert_eq!(
            p.passes[1],
            Region {
                origin: [1, 1],
                extent: [3, 2]
            }
        );
        for cy in 1..3 {
            for cx in 1..4 {
                for y in cy * 12..((cy + 1) * 12).min(29) {
                    assert!(contains(p.passes[0], cx, y));
                }
            }
        }
    }

    #[test]
    fn offscreen_empty_and_viewport_edges_do_not_create_invalid_dispatches() {
        for cell in [0, 12, 48] {
            for bounds in [
                Rect::NOTHING,
                Rect::from_min_max(egui::pos2(-50.0, -50.0), egui::pos2(-1.0, -1.0)),
            ] {
                assert_eq!(
                    FilterPlan::new(bounds, [37, 29], 1.0, cell).passes,
                    [Region::default(); 2]
                );
            }
            let p = FilterPlan::new(
                Rect::from_min_max(egui::pos2(-10.0, -10.0), egui::pos2(60.0, 50.0)),
                [37, 29],
                1.0,
                cell,
            );
            assert_eq!(p.passes[0].origin, [0, 0]);
            assert_eq!(p.passes[1].origin, [0, 0]);
            assert_eq!(p.passes[0].extent[1], 29);
        }
    }
}
