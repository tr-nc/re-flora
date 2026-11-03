//! Shared power proposal for buffer-less RIS. Integer multiplicities make the
//! sampled PDF exactly the one stored with each slot, with full light support.
use crate::generated::gpu_structs::LightGpu;

pub(crate) const LIGHT_PROPOSAL_SLOTS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightProposal {
    pub light_index: u32,
    pub inverse_probability: f32,
}

pub(crate) fn build_light_proposals(lights: &[LightGpu]) -> Vec<LightProposal> {
    assert!(lights.len() <= LIGHT_PROPOSAL_SLOTS);
    let mut result = vec![LightProposal::default(); LIGHT_PROPOSAL_SLOTS];
    if lights.is_empty() {
        return result;
    }
    let powers: Vec<f64> = lights
        .iter()
        .map(|light| {
            let color = light.color;
            (0.2126 * f64::from(color[0])
                + 0.7152 * f64::from(color[1])
                + 0.0722 * f64::from(color[2]))
                * f64::from(light.intensity)
        })
        .collect();
    let power_sum: f64 = powers.iter().sum();
    // Reserve a uniform floor, so even a weak nearby light cannot lose support
    // to a much brighter distant light. Quantization changes proposals, not energy.
    let remaining = LIGHT_PROPOSAL_SLOTS - lights.len();
    let weights: Vec<f64> = powers
        .iter()
        .map(|power| {
            let uniform = 1.0 / lights.len() as f64;
            0.25 * uniform
                + 0.75
                    * if power_sum > 0.0 {
                        power / power_sum
                    } else {
                        uniform
                    }
        })
        .collect();
    let mut counts: Vec<usize> = weights
        .iter()
        .map(|weight| 1 + (weight * remaining as f64).floor() as usize)
        .collect();
    let mut order: Vec<usize> = (0..lights.len()).collect();
    order.sort_by(|&a, &b| {
        let fraction = |i: usize| (weights[i] * remaining as f64).fract();
        fraction(b).total_cmp(&fraction(a)).then(a.cmp(&b))
    });
    let unused = LIGHT_PROPOSAL_SLOTS - counts.iter().sum::<usize>();
    for &index in order.iter().take(unused) {
        counts[index] += 1;
    }
    let mut offset = 0;
    for (index, count) in counts.into_iter().enumerate() {
        result[offset..offset + count].fill(LightProposal {
            light_index: index as u32,
            inverse_probability: LIGHT_PROPOSAL_SLOTS as f32 / count as f32,
        });
        offset += count;
    }
    debug_assert_eq!(offset, LIGHT_PROPOSAL_SLOTS);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    fn light(intensity: f32, color: [f32; 3]) -> LightGpu {
        LightGpu {
            intensity,
            color,
            ..LightGpu::zeroed()
        }
    }

    #[test]
    fn proposals_preserve_support_and_exact_discrete_expectation() {
        for count in [1, 8, 64, 256] {
            let lights: Vec<_> = (0..count)
                .map(|i| light(if i == 0 { 1.0e30 } else { 1.0e-20 }, [0.0, 0.0, 1.0]))
                .collect();
            let table = build_light_proposals(&lights);
            let mut recovered = vec![0.0f64; count];
            for slot in &table {
                recovered[slot.light_index as usize] +=
                    f64::from(slot.inverse_probability) / LIGHT_PROPOSAL_SLOTS as f64;
            }
            for value in recovered {
                assert!((value - 1.0).abs() < 1.0e-6, "{value}");
            }
        }
    }

    #[test]
    fn power_changes_rebuild_distribution_and_empty_removal_has_no_indices() {
        let a = light(100.0, [1.0; 3]);
        let b = light(1.0, [1.0; 3]);
        let count_first =
            |table: Vec<LightProposal>| table.iter().filter(|p| p.light_index == 0).count();
        assert!(count_first(build_light_proposals(&[a, b])) > 3000);
        assert!(count_first(build_light_proposals(&[b, a])) < 1100);
        assert!(build_light_proposals(&[])
            .iter()
            .all(|p| p.inverse_probability == 0.0));
        let dark = build_light_proposals(&[light(0.0, [0.0; 3]); 8]);
        assert_eq!(dark.iter().filter(|p| p.light_index == 0).count(), 512);
    }
}
