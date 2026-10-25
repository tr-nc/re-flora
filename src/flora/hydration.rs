//! Root-fed plant storage. Units are gameplay water units, not calibrated botany.
//! A soil moisture band holds SOIL_BAND_CAPACITY units; each plant holds one.
//! Whole bands are reserved at the root receiver before fractional consumption;
//! soil diffusion cannot transfer water that has already been consumed.

pub const SOIL_BAND_CAPACITY: f64 = 32.0;
pub const DEFAULT_HYDRATION: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub uptake_seconds: f64,
    pub transpiration_seconds: f64,
}

impl Settings {
    pub fn normalized(self) -> Self {
        let bounded = |v: f64, fallback: f64| {
            if v.is_finite() {
                v.clamp(0.1, 3600.0)
            } else {
                fallback
            }
        };
        Self {
            uptake_seconds: bounded(self.uptake_seconds, 8.0),
            transpiration_seconds: bounded(self.transpiration_seconds, 120.0),
        }
    }
}

/// Advance all occupants of ONE supporting soil voxel together. Shared scarcity
/// scales uptake equally; output order never affects allocation. Returns total
/// transpired water. `water` is the actual remaining water, not a percentage.
pub fn advance_receiver(plants: &mut [f32], water: &mut f64, dt: f64, settings: Settings) -> f64 {
    assert!(dt.is_finite() && dt >= 0.0);
    assert!(water.is_finite() && *water >= 0.0);
    let settings = settings.normalized();
    let mut remaining = dt;
    let mut transpired = 0.0;
    // Bound changes in soil availability; the constant-coefficient plant update
    // within each step is analytic rather than a per-render-frame lerp.
    while remaining > 1e-10 {
        let step = remaining.min(0.0625);
        remaining -= step;
        let availability = (*water / (3.0 * SOIL_BAND_CAPACITY)).clamp(0.0, 1.0);
        let a = availability / settings.uptake_seconds;
        let b = 1.0 / settings.transpiration_seconds;
        let rate = a + b;
        let equilibrium = a / rate;
        let blend = -(-rate * step).exp_m1();
        let flows: Vec<_> = plants
            .iter()
            .map(|&h| {
                let h = f64::from(h.clamp(0.0, 1.0));
                let integrated = equilibrium * step + (h - equilibrium) * blend / rate;
                (
                    (a * (step - integrated)).max(0.0),
                    (b * integrated).max(0.0),
                )
            })
            .collect();
        let requested: f64 = flows.iter().map(|f| f.0).sum();
        let allocation = if requested > 0.0 {
            (*water / requested).min(1.0)
        } else {
            1.0
        };
        let mut used = 0.0;
        for (h, (uptake, loss)) in plants.iter_mut().zip(flows) {
            let uptake = uptake * allocation;
            let old = f64::from(*h);
            let loss = loss.min(old + uptake);
            *h = (old + uptake - loss).clamp(0.0, 1.0) as f32;
            used += uptake;
            transpired += loss;
        }
        *water = (*water - used).max(0.0);
    }
    transpired
}

/// Root-zone reservation is separate from freely moving coarse atlas water.
pub fn available_water(level: u8, reserve: f64) -> f64 {
    f64::from(level.min(3)) * SOIL_BAND_CAPACITY + reserve
}

/// Debit whole bands BEFORE exposing the remainder to future uptake. No reverse
/// transfer: the fractional reserve belongs to this root voxel, shared by all
/// its occupants. External soil drying/spread can only spend the atlas bands.
pub fn settle_uptake(level: u8, reserve: f64, uptake: f64) -> (u8, f64) {
    let needed = (uptake - reserve).max(0.0);
    let bands = ((needed / SOIL_BAND_CAPACITY).ceil() as u8).min(level);
    (
        level - bands,
        (reserve + f64::from(bands) * SOIL_BAND_CAPACITY - uptake).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        Settings {
            uptake_seconds: 8.0,
            transpiration_seconds: 120.0,
        }
    }
    #[test]
    fn dry_roots_spend_stored_water_without_immediate_wilting() {
        let mut h = [1.0];
        advance_receiver(&mut h, &mut 0.0, 1.0, settings());
        assert!((f64::from(h[0]) - (-1.0_f64 / 120.0).exp()).abs() < 1e-6);
        assert!(h[0] > 0.99);
    }
    #[test]
    fn wet_roots_recover_gradually_and_conserve_water() {
        let mut h = [0.1, 0.3, 0.8];
        let before: f64 = h.iter().map(|&x| f64::from(x)).sum();
        let mut soil = 96.0;
        let loss = advance_receiver(&mut h, &mut soil, 2.0, settings());
        let after: f64 = h.iter().map(|&x| f64::from(x)).sum();
        assert!(h[0] > 0.1 && h[0] < 0.5);
        assert!((before + 96.0 - after - soil - loss).abs() < 2e-6);
        assert!(soil < 96.0);
    }
    #[test]
    fn shared_soil_cannot_be_spent_twice_and_order_does_not_matter() {
        let mut a = [0.2, 0.7, 0.3];
        let mut b = [0.3, 0.7, 0.2];
        let mut wa = 0.0001;
        let mut wb = wa;
        advance_receiver(&mut a, &mut wa, 4.0, settings());
        advance_receiver(&mut b, &mut wb, 4.0, settings());
        b.reverse();
        assert_eq!(a, b);
        assert!((wa - wb).abs() < 1e-12);
        assert!(wa >= 0.0);
    }
    #[test]
    fn scarce_receiver_and_reservation_never_duplicate_water() {
        let mut plants = vec![0.0; 1024];
        let mut water = 0.001;
        let initial = water;
        let loss = advance_receiver(
            &mut plants,
            &mut water,
            0.125,
            Settings {
                uptake_seconds: 0.1,
                transpiration_seconds: 120.0,
            },
        );
        let stored: f64 = plants.iter().map(|&h| f64::from(h)).sum();
        assert!(water >= 0.0);
        assert!((initial - water - stored - loss).abs() < 1e-8);
        let (level, reserve) = settle_uptake(1, 0.0, 0.25);
        assert_eq!(level, 0); // No coarse band remains available to diffuse twice.
        assert_eq!(reserve, 31.75);
        assert_eq!(available_water(level, reserve) + 0.25, 32.0);
        // Repeated consumption survives intervening changes to free atlas water.
        let (next, remain) = settle_uptake(level, reserve, 1.0);
        assert_eq!((next, remain), (0, 30.75));
    }

    #[test]
    fn partitioned_time_and_coarse_soil_roundtrip_preserve_storage() {
        let mut a = [0.2];
        let mut b = a;
        let mut wa = 96.0;
        let mut wb = wa;
        advance_receiver(&mut a, &mut wa, 2.0, settings());
        for _ in 0..8 {
            advance_receiver(&mut b, &mut wb, 0.25, settings());
        }
        assert_eq!(a, b);
        assert!((wa - wb).abs() < 1e-10);
        let (level, reserve) = settle_uptake(3, 0.0, 96.0 - wa);
        assert!((available_water(level, reserve) - wa).abs() < 1e-10);
        let before = b;
        advance_receiver(&mut b, &mut wb, 0.0, settings());
        assert_eq!(b, before);
    }
}
