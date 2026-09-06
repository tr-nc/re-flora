#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindResponseCurve {
    pub min_strength: f32,
    pub max_strength: f32,
    pub power: f32,
}

impl WindResponseCurve {
    pub fn factor(self, normalized_strength: f32) -> f32 {
        let normalized_strength = normalized_strength.clamp(0.0, 1.0);
        let (min_strength, max_strength) = if self.min_strength <= self.max_strength {
            (self.min_strength, self.max_strength)
        } else {
            (self.max_strength, self.min_strength)
        };
        let range = max_strength - min_strength;
        if range <= f32::EPSILON {
            return if normalized_strength >= max_strength {
                1.0
            } else {
                0.0
            };
        }

        let scaled = ((normalized_strength - min_strength) / range).clamp(0.0, 1.0);
        scaled.powf(self.power.max(0.001))
    }
}
