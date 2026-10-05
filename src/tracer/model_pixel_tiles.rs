//! Legacy pixel-tile layout used by particle publication tests.

const BATCH_TEXELS: usize = 64 * 1024 * 1024 / 16;
// Guaranteed Vulkan Z dispatch limit; split work this frame, never reject leaves.
const BATCH_INSTANCES: u32 = 65_535;

/// Layout follows the published back-to-front draw stream, not instance order.
/// Offscreen instances keep their draw positions but reserve no tile cells.
#[derive(Default)]
pub(super) struct ParticleTiles {
    offsets: Vec<u32>,
}
impl ParticleTiles {
    pub fn pack(models: &[(u32, bool)], draw_order: &[u32]) -> Self {
        assert_eq!(models.len(), draw_order.len());
        let mut offsets = vec![0; models.len()];
        let mut seen = vec![false; models.len()];
        let mut count = 0;
        let mut texels = 0;
        for &index in draw_order {
            let index = index as usize;
            assert!(!seen[index], "model draw order must be a permutation");
            seen[index] = true;
            let (resolution, visible) = models[index];
            assert!((8..=64).contains(&resolution));
            let cells = if visible {
                (resolution * resolution) as usize
            } else {
                0
            };
            if texels + cells > BATCH_TEXELS || count == BATCH_INSTANCES {
                count = 0;
                texels = 0;
            }
            offsets[index] = texels as u32;
            count += 1;
            texels += cells;
        }
        Self { offsets }
    }

    pub fn offsets(&self) -> &[u32] {
        &self.offsets
    }
}
