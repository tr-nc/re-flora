//! XPBD strands. Positions use the owning CollisionWorld's units; masses are kilograms.
//! Bending stores centerline curvature, not material twist. No rigid-body feedback or self collision.
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct RopeMaterial {
    pub radius: f32,
    pub linear_density: f32,
    pub stretch_rigidity: f32,
    pub bend_rigidity: f32,
    pub damping: f32,
    pub friction: f32,
}
impl RopeMaterial {
    /// Convert a meter-authored material to a different position/force length unit.
    pub fn scaled(self, units_per_meter: f32) -> Self {
        Self {
            radius: self.radius * units_per_meter,
            linear_density: self.linear_density / units_per_meter,
            stretch_rigidity: self.stretch_rigidity * units_per_meter,
            bend_rigidity: self.bend_rigidity * units_per_meter.powi(3),
            ..self
        }
    }
}
#[derive(Clone, Debug)]
pub struct RopeDesc {
    /// Centerline samples. Defines rest lengths unless explicit material lengths are supplied.
    pub points: Vec<Vec3>,
    /// Preserve material lengths when restoring a deformed snapshot.
    pub rest_lengths: Option<Vec<f32>>,
    pub material: RopeMaterial,
    pub pin_start: bool,
    pub pin_end: bool,
    /// Extra mass at the last point (e.g. a simplified kite). Not a separate rigid body.
    pub end_mass: f32,
}
impl RopeDesc {
    /// Uses the same bounded validation as world publication, without changing live state.
    pub fn validate(&self) -> Result<(), RopeError> { Rope::new(self.clone()).map(|_| ()) }
}
#[derive(Debug, thiserror::Error)]
#[error("invalid rope: {0}")]
pub struct RopeError(pub &'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RopeId(pub(crate) u64);
#[derive(Clone, Copy, Debug)]
pub(crate) struct Contact {
    pub segment: usize,
    pub t: f32,
    /// Contact-center plane, already offset by the capsule radius.
    pub point: Vec3,
    pub normal: Vec3,
}

pub struct Rope {
    positions: Vec<Vec3>,
    previous: Vec<Vec3>,
    velocities: Vec<Vec3>,
    weights: Vec<f32>,
    rest: Vec<f32>,
    stretch_lambda: Vec<f32>,
    stretch_directions: Vec<Vec3>,
    stretch_diagonal: Vec<f32>,
    stretch_rhs: Vec<f32>,
    stretch_off: Vec<f32>,
    bend_lambda: Vec<Vec3>,
    material: RopeMaterial,
    pins: [Option<Vec3>; 2],
    grab: Option<(usize, Vec3)>,
    acceleration: Vec3,
    end_acceleration: Vec3,
    pub(crate) contacts: Vec<Contact>,
}
impl Rope {
    pub(crate) fn new(desc: RopeDesc) -> Result<Self, RopeError> {
        let m = desc.material;
        if !(2..=129).contains(&desc.points.len()) || !desc.points.iter().all(|p| p.is_finite()) {
            return Err(RopeError("expected 2..129 finite points"));
        }
        if ![m.radius, m.linear_density, m.stretch_rigidity]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
            || ![m.bend_rigidity, m.damping, m.friction, desc.end_mass]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.)
        {
            return Err(RopeError("invalid material or endpoint mass"));
        }
        let rest = desc.rest_lengths.unwrap_or_else(|| {
            desc.points
                .windows(2)
                .map(|p| p[0].distance(p[1]))
                .collect()
        });
        if rest.len() + 1 != desc.points.len() {
            return Err(RopeError("rest length count mismatch"));
        }
        if rest.iter().any(|l| !l.is_finite() || *l <= m.radius * 0.1) {
            return Err(RopeError("degenerate centerline"));
        }
        let n = desc.points.len();
        let weights: Vec<f32> = (0..n)
            .map(|i| {
                let length =
                    (if i > 0 { rest[i - 1] } else { 0. }) + rest.get(i).copied().unwrap_or(0.);
                1. / (m.linear_density * length * 0.5 + if i == n - 1 { desc.end_mass } else { 0. })
            })
            .collect();
        if weights.iter().any(|w| !w.is_finite() || *w <= 0.) { return Err(RopeError("unrepresentable node mass")); }
        let pins = [
            desc.pin_start.then_some(desc.points[0]),
            desc.pin_end.then_some(desc.points[n - 1]),
        ];
        Ok(Self {
            previous: desc.points.clone(),
            positions: desc.points,
            velocities: vec![Vec3::ZERO; n],
            weights,
            stretch_lambda: vec![0.; n - 1],
            stretch_directions: vec![Vec3::X; n - 1],
            stretch_diagonal: vec![0.; n - 1],
            stretch_rhs: vec![0.; n - 1],
            stretch_off: vec![0.; n - 1],
            bend_lambda: vec![Vec3::ZERO; n - 2],
            rest,
            material: m,
            pins,
            grab: None,
            acceleration: Vec3::ZERO,
            end_acceleration: Vec3::ZERO,
            contacts: Vec::new(),
        })
    }
    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }
    pub fn rest_lengths(&self) -> &[f32] {
        &self.rest
    }
    pub fn radius(&self) -> f32 {
        self.material.radius
    }
    pub fn is_pinned(&self, index: usize) -> bool {
        (index == 0 && self.pins[0].is_some())
            || (index == self.positions.len() - 1 && self.pins[1].is_some())
    }
    pub fn set_end_pinned(&mut self, pinned: bool) {
        self.pins[1] = pinned.then_some(*self.positions.last().unwrap());
    }
    pub fn release_start(&mut self) {
        self.pins[0] = None;
    }
    pub fn set_acceleration(
        &mut self,
        acceleration: Vec3,
        end_acceleration: Vec3,
    ) -> Result<(), RopeError> {
        if !acceleration.is_finite() || !end_acceleration.is_finite() {
            return Err(RopeError("nonfinite force"));
        }
        self.acceleration = acceleration;
        self.end_acceleration = end_acceleration;
        Ok(())
    }
    /// Grab a free node with a compliant, speed-limited target. Pins remain authoritative.
    pub fn set_grab(&mut self, grab: Option<(usize, Vec3)>) -> Result<(), RopeError> {
        if let Some((i, p)) = grab {
            if i >= self.positions.len() || !p.is_finite() || self.is_pinned(i) {
                return Err(RopeError("invalid grab"));
            }
        }
        self.grab = grab;
        Ok(())
    }
    pub fn max_strain(&self) -> f32 {
        self.positions
            .windows(2)
            .zip(&self.rest)
            .map(|(p, l)| (p[0].distance(p[1]) / l - 1.).abs())
            .fold(0., f32::max)
    }
    fn weight(&self, i: usize) -> f32 {
        if self.is_pinned(i) {
            0.
        } else {
            self.weights[i]
        }
    }
    pub(crate) fn predict(&mut self, h: f32, gravity: Vec3) {
        self.previous.clone_from(&self.positions);
        self.stretch_lambda.fill(0.);
        self.bend_lambda.fill(Vec3::ZERO);
        self.contacts.clear();
        let decay = (-self.material.damping * h).exp();
        for i in 0..self.positions.len() {
            if self.is_pinned(i) {
                self.velocities[i] = Vec3::ZERO;
                continue;
            }
            let extra = if i + 1 == self.positions.len() {
                self.end_acceleration
            } else {
                Vec3::ZERO
            };
            self.velocities[i] =
                self.velocities[i] * decay + (gravity + self.acceleration + extra) * h;
            self.positions[i] += self.velocities[i] * h;
        }
    }
    pub(crate) fn old_positions(&self) -> &[Vec3] {
        &self.previous
    }
    pub(crate) fn sweep_position(&mut self, i: usize, position: Vec3) {
        self.positions[i] = position;
    }
    /// Solve the coupled XPBD length system, not one segment at a time. A heavy
    /// endpoint on a fine line otherwise converges far too slowly even at small dt.
    /// J M^-1 J^T + compliance/h² is tridiagonal for a chain: Thomas elimination
    /// propagates tension across the entire strand in O(n), then Newton iterations
    /// interleave with bending, grabbing and unilateral contacts.
    fn solve_stretch(&mut self,h:f32) {
        let n=self.rest.len();
        for i in 0..n {
            let delta=self.positions[i+1]-self.positions[i];let length=delta.length();
            if length>1e-7 {self.stretch_directions[i]=delta/length;}
            let alpha=self.rest[i]/self.material.stretch_rigidity/(h*h);
            self.stretch_diagonal[i]=self.weight(i)+self.weight(i+1)+alpha;
            self.stretch_rhs[i]=-(length-self.rest[i])-alpha*self.stretch_lambda[i];
            self.stretch_off[i]=if i>0 {-self.weight(i)*self.stretch_directions[i-1].dot(self.stretch_directions[i])} else {0.};
        }
        for i in 1..n {
            let factor=self.stretch_off[i]/self.stretch_diagonal[i-1].max(f32::EPSILON);
            self.stretch_diagonal[i]-=factor*self.stretch_off[i];
            self.stretch_rhs[i]-=factor*self.stretch_rhs[i-1];
        }
        for i in (0..n).rev() {
            let next=if i+1<n {self.stretch_off[i+1]*self.stretch_rhs[i+1]} else {0.};
            self.stretch_rhs[i]=(self.stretch_rhs[i]-next)/self.stretch_diagonal[i].max(f32::EPSILON);
        }
        for i in 0..n {
            let dl=self.stretch_rhs[i];self.stretch_lambda[i]+=dl;
            let a=self.weight(i);let b=self.weight(i+1);let gradient=self.stretch_directions[i];
            self.positions[i]-=a*gradient*dl;self.positions[i+1]+=b*gradient*dl;
        }
    }
    pub(crate) fn solve(&mut self, h: f32) {
        let n = self.positions.len();
        let mut grab_lambda = Vec3::ZERO;
        let grab = self.grab.map(|(i, p)| {
            (
                i,
                self.previous[i]
                    + (p - self.previous[i]).clamp_length_max(self.material.radius * 0.5),
            )
        });
        for _ in 0..8 {
            self.solve_stretch(h);
            if self.material.bend_rigidity > 0. {
                for i in 1..n - 1 {
                    // Curvature energy EI/(2 L³) |x0-2x1+x2|²; isotropic, no twist state.
                    let l = (self.rest[i - 1] + self.rest[i]) * 0.5;
                    let alpha = l.powi(3) / self.material.bend_rigidity / (h * h);
                    let a = self.weight(i - 1);
                    let b = self.weight(i);
                    let c = self.weight(i + 1);
                    let residual =
                        self.positions[i - 1] - 2. * self.positions[i] + self.positions[i + 1];
                    let dl =
                        (-residual - alpha * self.bend_lambda[i - 1]) / (a + 4. * b + c + alpha);
                    self.bend_lambda[i - 1] += dl;
                    self.positions[i - 1] += a * dl;
                    self.positions[i] -= 2. * b * dl;
                    self.positions[i + 1] += c * dl;
                }
            }
            if let Some((i, target)) = grab {
                let w = self.weight(i);
                let alpha = 0.00002 / (h * h);
                let dl = (target - self.positions[i] - alpha * grab_lambda) / (w + alpha);
                grab_lambda += dl;
                self.positions[i] += w * dl;
            }
            for contact in &self.contacts {
                let i = contact.segment;
                let t = contact.t;
                let a = self.weight(i);
                let b = self.weight(i + 1);
                let w = a * (1. - t).powi(2) + b * t * t;
                if w == 0. {
                    continue;
                }
                let p = self.positions[i].lerp(self.positions[i + 1], t);
                let depth = (contact.point - p).dot(contact.normal).max(0.);
                let correction = contact.normal * depth / w;
                self.positions[i] += a * (1. - t) * correction;
                self.positions[i + 1] += b * t * correction;
                // Position-level Coulomb friction bounded by this normal correction.
                let motion = self.positions[i].lerp(self.positions[i + 1], t)
                    - self.previous[i].lerp(self.previous[i + 1], t);
                let tangent = motion - contact.normal * motion.dot(contact.normal);
                let friction = tangent.clamp_length_max(self.material.friction * depth) / w;
                self.positions[i] -= a * (1. - t) * friction;
                self.positions[i + 1] -= b * t * friction;
            }
        }
        for i in 0..n {
            self.velocities[i] = (self.positions[i] - self.previous[i]) / h;
        }
    }
}
