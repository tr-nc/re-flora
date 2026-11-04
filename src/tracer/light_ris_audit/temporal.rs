//! Compare real receiver outputs by surface identity, never by compacted row.
use std::collections::HashMap;

pub(super) const HEADER_WORDS: usize = 20;
pub(super) const RECORD_WORDS: usize = 24;
pub(super) const MAX_RECORDS: usize = 4096;
pub(super) const BUFFER_BYTES: u64 = ((HEADER_WORDS + RECORD_WORDS * MAX_RECORDS) * 4) as u64;

#[derive(Default)]
pub(super) struct TemporalAudit {
    previous: HashMap<[u32; 5], Record>,
}
#[derive(Clone)]
struct Record {
    surface: [u32; 6],
    reference: [f32; 3],
    rendered: [f32; 3],
    pool: u32,
    revisions: [u32; 3],
    total_reference_luma: f32,
}
#[derive(Debug, Default)]
pub(super) struct Observation {
    pub matched: usize,
    pub stationary: usize,
    pub lit_stationary: usize,
    pub changed: usize,
    pub pool_changed: usize,
    pub max_relative_delta: f32,
    pub error_delta_sum: f32,
    pub lit_matched: usize,
    pub moving_lit: usize,
    pub lit_error_delta_sum: f32,
    pub max_moving_error_delta: f32,
    pub max_moving_total_error_delta: f32,
    pub moving_spike: Option<String>,
    pub max_total_excess_delta: f32,
}
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.into_iter()
        .zip(b)
        .all(|(a, b)| (a - b).abs() <= 1.0e-8 + 1.0e-5 * a.abs().max(b.abs()))
}
impl TemporalAudit {
    pub fn observe(&mut self, words: &[u32]) -> Observation {
        let mut result = Observation::default();
        let mut current = HashMap::new();
        for w in words.chunks_exact(RECORD_WORDS) {
            let key = w[..5].try_into().unwrap();
            let record = Record {
                surface: w[5..11].try_into().unwrap(),
                reference: std::array::from_fn(|i| f32::from_bits(w[11 + i])),
                rendered: std::array::from_fn(|i| f32::from_bits(w[14 + i])),
                pool: w[17],
                revisions: w[18..21].try_into().unwrap(),
                total_reference_luma: f32::from_bits(w[21]),
            };
            if let Some(old) = self.previous.get(&key) {
                result.matched += 1;
                let scale = luma(record.reference).max(luma(old.reference)).max(1.0e-5);
                let error_delta = ((luma(record.rendered) - luma(record.reference))
                    - (luma(old.rendered) - luma(old.reference)))
                .abs()
                    / scale;
                result.error_delta_sum += error_delta;
                if luma(record.reference).max(luma(old.reference)) > 1.0e-6
                    && old.revisions == record.revisions
                {
                    result.lit_matched += 1;
                    result.lit_error_delta_sum += error_delta;
                    let total_scale = record
                        .total_reference_luma
                        .max(old.total_reference_luma)
                        .max(1.0e-5);
                    let output_delta = (luma(record.rendered) - luma(old.rendered)).abs();
                    let reference_delta = (luma(record.reference) - luma(old.reference)).abs();
                    result.max_total_excess_delta = result
                        .max_total_excess_delta
                        .max((output_delta - reference_delta).max(0.0) / total_scale);
                    if old.surface != record.surface {
                        result.moving_lit += 1;
                        result.max_moving_error_delta =
                            result.max_moving_error_delta.max(error_delta);
                        let total_scale = record
                            .total_reference_luma
                            .max(old.total_reference_luma)
                            .max(1.0e-5);
                        let total_error_delta = error_delta * scale / total_scale;
                        if total_error_delta > result.max_moving_total_error_delta {
                            result.max_moving_total_error_delta = total_error_delta;
                            if total_error_delta > 0.02 {
                                result.moving_spike = Some(format!(
                                    "key={key:?} total={total_scale} old_position={:?} position={:?} old_rendered={:?} rendered={:?} old_reference={:?} reference={:?} output_delta={} reference_delta={} error_delta={total_error_delta}",
                                    &old.surface[..3], &record.surface[..3], old.rendered, record.rendered,
                                    old.reference, record.reference,
                                    (luma(record.rendered)-luma(old.rendered))/total_scale,
                                    (luma(record.reference)-luma(old.reference))/total_scale));
                            }
                        }
                    }
                }
                if old.surface == record.surface
                    && old.revisions == record.revisions
                    && close(old.reference, record.reference)
                {
                    result.stationary += 1;
                    if luma(record.reference) > 1.0e-6 {
                        result.lit_stationary += 1;
                    }
                    result.pool_changed += usize::from(old.pool != record.pool);
                    if !close(old.rendered, record.rendered) {
                        result.changed += 1;
                        result.max_relative_delta = result
                            .max_relative_delta
                            .max((luma(record.rendered) - luma(old.rendered)).abs() / scale);
                    }
                }
            }
            current.insert(key, record);
        }
        self.previous = current;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receiver(id: u32, value: f32) -> [u32; RECORD_WORDS] {
        let mut r = [0; RECORD_WORDS];
        r[0] = id;
        r[21] = 10.0f32.to_bits();
        for i in 0..3 {
            r[11 + i] = 1.0f32.to_bits();
            r[14 + i] = value.to_bits();
        }
        r
    }
    #[test]
    fn follows_receiver_identity_across_compaction_and_detects_real_output_flicker() {
        let mut audit = TemporalAudit::default();
        let a = receiver(1, 1.0);
        let b = receiver(2, 0.5);
        audit.observe(&[a, b].concat());
        assert_eq!(audit.observe(&[b, a].concat()).changed, 0);
        let mut changed = a;
        changed[14] = 2.0f32.to_bits();
        changed[17] = 7;
        let result = audit.observe(&[b, changed].concat());
        assert_eq!(result.stationary, 2);
        assert_eq!(result.lit_stationary, 2);
        assert_eq!(result.changed, 1);
        assert_eq!(result.pool_changed, 1);
        assert!(result.max_relative_delta > 0.2);
    }
    #[test]
    fn moving_lit_error_does_not_hide_in_unlit_samples_or_legitimate_light_changes() {
        let mut audit = TemporalAudit::default();
        audit.observe(&receiver(1, 1.0));
        let mut moved = receiver(1, 2.0);
        moved[5] = 1;
        for i in 11..14 {
            moved[i] = 2.0f32.to_bits();
        }
        let result = audit.observe(&moved);
        assert_eq!(result.moving_lit, 1);
        assert_eq!(result.lit_error_delta_sum, 0.0);
        moved[5] = 2;
        for i in 14..17 {
            moved[i] = 3.0f32.to_bits();
        }
        let result = audit.observe(&moved);
        assert_eq!(result.lit_matched, 1);
        assert!((result.max_moving_error_delta - 0.5).abs() < 1.0e-6);
        assert!((result.max_moving_total_error_delta - 0.1).abs() < 1.0e-6);
    }
    #[test]
    fn a_reference_only_shadow_step_is_not_excess_rendered_variation() {
        let mut audit = TemporalAudit::default();
        audit.observe(&receiver(1, 1.0));
        let mut moved = receiver(1, 1.0);
        moved[5] = 1;
        for channel in 11..14 {
            moved[channel] = 2.0f32.to_bits();
        }
        let result = audit.observe(&moved);
        assert!(result.max_moving_total_error_delta > 0.09);
        assert_eq!(result.max_total_excess_delta, 0.0);
    }
    #[test]
    fn changed_pose_lighting_or_geometry_are_not_static_failures() {
        for index in [5, 8, 11, 18, 20] {
            let mut audit = TemporalAudit::default();
            let r = receiver(1, 1.0);
            audit.observe(&r);
            let mut moved = receiver(1, 2.0);
            moved[index] = 2.0f32.to_bits();
            let result = audit.observe(&moved);
            assert_eq!(result.matched, 1);
            assert_eq!(result.stationary, 0);
            assert_eq!(result.changed, 0);
        }
    }
}
