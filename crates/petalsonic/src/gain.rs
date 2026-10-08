//! Utilities for converting between decibels and linear gain.
//!
//! These helpers are used throughout the engine for both HRTF gain compensation
//! and per-source volume handling.

/// Convert decibels to linear gain.
///
/// - `0.0` dB -> `1.0` (unity gain)
/// - `-6.0` dB ~ `0.501`
/// - `6.0` dB ~ `1.995`
#[inline]
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
