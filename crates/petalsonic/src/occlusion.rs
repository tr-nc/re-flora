use crate::error::{PetalSonicError, Result};

/// Maximum number of stable directional direct-field lobes produced for one Voice.
pub const MAX_DIRECT_LOBES: usize = 4;

const MIN_RESPONSE_SECONDS: f32 = 0.001;
const MAX_RESPONSE_SECONDS: f32 = 10.0;
const MAX_RESPONSE_AGE_SECONDS: f32 = 5.0;

/// Bounded temporal and attenuation policy for a distributed ambient Voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistributedOcclusionProfile {
    gain_floor: [f32; 3],
    attack_seconds: f32,
    release_seconds: f32,
    enter_occluded_visibility: f32,
    exit_occluded_visibility: f32,
    minimum_dwell_seconds: f32,
    max_response_age_seconds: f32,
    lobe_count: u8,
}

impl DistributedOcclusionProfile {
    /// Sets the minimum linear gain per frequency band after energy aggregation.
    pub fn with_gain_floor(mut self, gain_floor: [f32; 3]) -> Result<Self> {
        if gain_floor
            .iter()
            .any(|gain| !gain.is_finite() || !(0.0..=1.0).contains(gain) || *gain == 0.0)
        {
            return Err(invalid_profile(
                "gain floor must contain finite values in the range 0.0 < gain <= 1.0",
            ));
        }
        self.gain_floor = gain_floor;
        Ok(self)
    }

    /// Sets continuous gain response times: attack toward lower energy, release toward higher.
    pub fn with_response_times(
        mut self,
        attack_seconds: f32,
        release_seconds: f32,
    ) -> Result<Self> {
        validate_response_seconds("attack", attack_seconds)?;
        validate_response_seconds("release", release_seconds)?;
        self.attack_seconds = attack_seconds;
        self.release_seconds = release_seconds;
        Ok(self)
    }

    /// Sets Schmitt visibility thresholds and minimum state dwell time.
    pub fn with_classification(
        mut self,
        enter_occluded_visibility: f32,
        exit_occluded_visibility: f32,
        minimum_dwell_seconds: f32,
    ) -> Result<Self> {
        if !enter_occluded_visibility.is_finite()
            || !exit_occluded_visibility.is_finite()
            || !(0.0..=1.0).contains(&enter_occluded_visibility)
            || !(0.0..=1.0).contains(&exit_occluded_visibility)
            || enter_occluded_visibility >= exit_occluded_visibility
        {
            return Err(invalid_profile(
                "classification requires 0 <= enter < exit <= 1",
            ));
        }
        if !minimum_dwell_seconds.is_finite()
            || !(0.0..=MAX_RESPONSE_SECONDS).contains(&minimum_dwell_seconds)
        {
            return Err(invalid_profile(format!(
                "minimum dwell must be finite and in 0..={MAX_RESPONSE_SECONDS} seconds"
            )));
        }
        self.enter_occluded_visibility = enter_occluded_visibility;
        self.exit_occluded_visibility = exit_occluded_visibility;
        self.minimum_dwell_seconds = minimum_dwell_seconds;
        Ok(self)
    }

    /// Sets the maximum age at which a budget-skipped response may be reused.
    pub fn with_max_response_age(mut self, max_response_age_seconds: f32) -> Result<Self> {
        if !max_response_age_seconds.is_finite()
            || !(MIN_RESPONSE_SECONDS..=MAX_RESPONSE_AGE_SECONDS)
                .contains(&max_response_age_seconds)
        {
            return Err(invalid_profile(format!(
                "maximum response age must be in {MIN_RESPONSE_SECONDS}..={MAX_RESPONSE_AGE_SECONDS} seconds"
            )));
        }
        self.max_response_age_seconds = max_response_age_seconds;
        Ok(self)
    }

    /// Sets the bounded number of stable directional lobes.
    pub fn with_lobe_count(mut self, lobe_count: u8) -> Result<Self> {
        if !(1..=MAX_DIRECT_LOBES as u8).contains(&lobe_count) {
            return Err(invalid_profile(format!(
                "lobe count must be in 1..={MAX_DIRECT_LOBES}"
            )));
        }
        self.lobe_count = lobe_count;
        Ok(self)
    }

    pub fn gain_floor(self) -> [f32; 3] {
        self.gain_floor
    }

    pub fn response_times_seconds(self) -> (f32, f32) {
        (self.attack_seconds, self.release_seconds)
    }

    pub fn classification(self) -> (f32, f32, f32) {
        (
            self.enter_occluded_visibility,
            self.exit_occluded_visibility,
            self.minimum_dwell_seconds,
        )
    }

    pub fn max_response_age_seconds(self) -> f32 {
        self.max_response_age_seconds
    }

    pub fn lobe_count(self) -> u8 {
        self.lobe_count
    }
}

impl Default for DistributedOcclusionProfile {
    fn default() -> Self {
        Self {
            // -4, -8, and -12 dB. These are general conservative defaults and remain tunable.
            gain_floor: [0.630_957_37, 0.398_107_17, 0.251_188_64],
            attack_seconds: 0.2,
            release_seconds: 0.15,
            enter_occluded_visibility: 0.25,
            exit_occluded_visibility: 0.55,
            minimum_dwell_seconds: 0.12,
            max_response_age_seconds: 0.25,
            lobe_count: 3,
        }
    }
}

/// Geometry-response policy captured independently from source extent and route placement.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OcclusionProfile {
    /// Compatibility behavior: exact material transmission with the existing direct smoothing.
    #[default]
    PointExact,
    /// Energy aggregation with bounded attenuation, temporal response, and stable directions.
    AmbientDistributed(DistributedOcclusionProfile),
}

fn validate_response_seconds(name: &str, seconds: f32) -> Result<()> {
    if seconds.is_finite() && (MIN_RESPONSE_SECONDS..=MAX_RESPONSE_SECONDS).contains(&seconds) {
        Ok(())
    } else {
        Err(invalid_profile(format!(
            "{name} must be in {MIN_RESPONSE_SECONDS}..={MAX_RESPONSE_SECONDS} seconds"
        )))
    }
}

fn invalid_profile(reason: impl Into<String>) -> PetalSonicError {
    PetalSonicError::InvalidConfiguration {
        field: "occlusion_profile",
        reason: reason.into(),
    }
}
