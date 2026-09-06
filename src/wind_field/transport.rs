//! PROTOTYPE: do boundary inflow and local emitters feel coherent when they share
//! one persistent, spatially transported field? This is a dissipative 2D transport
//! model, not incompressible CFD: no pressure projection or terrain obstruction.
use glam::Vec2;

pub const SIDE: usize = 32;
pub const CELLS: usize = SIDE * SIDE;

pub struct Transport {
    values: Vec<Vec2>,
    next: Vec<Vec2>,
    pub extent: Vec2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_change_arrives_near_before_far_and_never_rewrites_interior() {
        let mut field = Transport::default();
        field.extent = Vec2::splat(128.);
        field.step(1. / 60., 30., |_| Vec2::X, |_| Vec2::ZERO);
        assert!(field.sample(Vec2::new(0., 64.)).x > 0.);
        assert_eq!(field.sample(Vec2::splat(64.)), Vec2::ZERO);
        for _ in 0..180 {
            field.step(1. / 60., 30., |_| Vec2::X, |_| Vec2::ZERO);
        }
        assert!(field.sample(Vec2::new(16., 64.)).x > field.sample(Vec2::new(112., 64.)).x + 0.1);
        let before = field.sample(Vec2::splat(64.));
        field.step(1. / 60., 30., |_| Vec2::Y, |_| Vec2::ZERO);
        assert_eq!(field.sample(Vec2::splat(64.)).y, before.y);
        for _ in 0..360 {
            field.step(1. / 60., 30., |_| Vec2::Y, |_| Vec2::ZERO);
        }
        assert!(field.sample(Vec2::new(64., 16.)).y > 0.1);
        assert!(field.values.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn local_input_uses_the_same_field_and_decays_without_an_emitter() {
        let mut field = Transport::default();
        let p = Vec2::splat(256.);
        for _ in 0..30 {
            field.step(
                1. / 60.,
                0.,
                |_| Vec2::ZERO,
                |q| {
                    if q.distance(p) < 40. {
                        Vec2::X
                    } else {
                        Vec2::ZERO
                    }
                },
            );
        }
        let peak = field.sample(p).x;
        assert!(peak > 0.1);
        assert_eq!(field.sample(Vec2::ZERO), Vec2::ZERO);
        for _ in 0..120 {
            field.step(1. / 60., 0., |_| Vec2::ZERO, |_| Vec2::ZERO);
        }
        assert!(field.sample(p).x < peak);
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            values: vec![Vec2::ZERO; CELLS],
            next: vec![Vec2::ZERO; CELLS],
            extent: Vec2::splat(512.),
        }
    }
}

impl Transport {
    pub fn values(&self) -> &[Vec2] {
        &self.values
    }

    pub fn step(
        &mut self,
        dt: f32,
        speed: f32,
        inflow: impl Fn(Vec2) -> Vec2,
        local: impl Fn(Vec2) -> Vec2,
    ) {
        let spacing = self.extent / (SIDE - 1) as f32;
        let max_velocity = self.values.iter().map(|v| v.length()).fold(8_f32, f32::max);
        let substeps = (dt * speed * max_velocity / (spacing.min_element() * 0.2))
            .ceil()
            .max(1.) as usize;
        let h = dt / substeps as f32;
        // Only border cells evaluate external inflow; edits to it cannot touch interior cells.
        let mut edges = vec![Vec2::ZERO; CELLS];
        let mut forces = vec![Vec2::ZERO; CELLS];
        for z in 0..SIDE {
            for x in 0..SIDE {
                let i = z * SIDE + x;
                let p = Vec2::new(x as f32, z as f32) * spacing;
                forces[i] = local(p);
                if x == 0 || z == 0 || x == SIDE - 1 || z == SIDE - 1 {
                    edges[i] = inflow(p);
                }
            }
        }
        for _ in 0..substeps {
            for z in 0..SIDE {
                for x in 0..SIDE {
                    let i = z * SIDE + x;
                    let v = self.values[i];
                    let b = edges[i];
                    let left = if x > 0 {
                        self.values[i - 1]
                    } else if b.x > 0. {
                        b
                    } else {
                        v
                    };
                    let right = if x + 1 < SIDE {
                        self.values[i + 1]
                    } else if b.x < 0. {
                        b
                    } else {
                        v
                    };
                    let down = if z > 0 {
                        self.values[i - SIDE]
                    } else if b.y > 0. {
                        b
                    } else {
                        v
                    };
                    let up = if z + 1 < SIDE {
                        self.values[i + SIDE]
                    } else if b.y < 0. {
                        b
                    } else {
                        v
                    };
                    let flux = |a: Vec2, b: Vec2, axis: usize| {
                        let va = a[axis] * speed;
                        let vb = b[axis] * speed;
                        (a * va + b * vb - (b - a) * va.abs().max(vb.abs())) * 0.5
                    };
                    let transported = v - h
                        * ((flux(v, right, 0) - flux(left, v, 0)) / spacing.x
                            + (flux(v, up, 1) - flux(down, v, 1)) / spacing.y);
                    // The same field receives local forcing; it is never added a second time in shaders.
                    self.next[i] = ((transported + forces[i] * (h * 3.)) * (-h * 0.18).exp())
                        .clamp_length_max(8.);
                }
            }
            std::mem::swap(&mut self.values, &mut self.next);
        }
    }

    pub fn sample(&self, p: Vec2) -> Vec2 {
        sample_grid(self.extent, p, |i| self.values[i])
    }
}

pub(super) fn sample_grid(extent: Vec2, p: Vec2, value: impl Fn(usize) -> Vec2) -> Vec2 {
    let q = (p / extent * (SIDE - 1) as f32).clamp(Vec2::ZERO, Vec2::splat((SIDE - 1) as f32));
    let x = (q.x as usize).min(SIDE - 2);
    let z = (q.y as usize).min(SIDE - 2);
    let f = q - Vec2::new(x as f32, z as f32);
    value(z * SIDE + x).lerp(value(z * SIDE + x + 1), f.x).lerp(
        value((z + 1) * SIDE + x).lerp(value((z + 1) * SIDE + x + 1), f.x),
        f.y,
    )
}
