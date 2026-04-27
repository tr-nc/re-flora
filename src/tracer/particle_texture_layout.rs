use crate::particles::{BUTTERFLY_FRAMES_PER_VARIANT, BUTTERFLY_VIEW_COUNT};
use crate::tracer::ButterflyPalettePreset;

#[derive(Debug, Clone, Copy)]
pub struct ParticleTextureLayout {
    leaf_layer: u32,
    leaf_layer_count: u32,
    butterfly_base_layer: u32,
    butterfly_preset_count: u32,
    butterfly_view_count: u32,
    butterfly_frames_per_view: u32,
    total_layer_count: u32,
}

impl ParticleTextureLayout {
    pub const fn new() -> Self {
        let leaf_layer = 0;
        let leaf_layer_count = 1;
        let butterfly_base_layer = leaf_layer + leaf_layer_count;
        let butterfly_preset_count = ButterflyPalettePreset::COUNT;
        let butterfly_view_count = BUTTERFLY_VIEW_COUNT;
        let butterfly_frames_per_view = BUTTERFLY_FRAMES_PER_VARIANT;
        let butterfly_layer_count =
            butterfly_preset_count * butterfly_view_count * butterfly_frames_per_view;
        let total_layer_count = butterfly_base_layer + butterfly_layer_count;

        Self {
            leaf_layer,
            leaf_layer_count,
            butterfly_base_layer,
            butterfly_preset_count,
            butterfly_view_count,
            butterfly_frames_per_view,
            total_layer_count,
        }
    }

    pub const fn leaf_layer(self) -> u32 {
        self.leaf_layer
    }

    pub const fn butterfly_base_layer(self) -> u32 {
        self.butterfly_base_layer
    }

    pub const fn butterfly_preset_count(self) -> u32 {
        self.butterfly_preset_count
    }

    pub const fn butterfly_view_count(self) -> u32 {
        self.butterfly_view_count
    }

    pub const fn butterfly_frames_per_view(self) -> u32 {
        self.butterfly_frames_per_view
    }

    pub const fn butterfly_layers_per_preset(self) -> u32 {
        self.butterfly_view_count * self.butterfly_frames_per_view
    }

    pub const fn butterfly_layer_count(self) -> u32 {
        self.butterfly_preset_count * self.butterfly_layers_per_preset()
    }

    pub const fn butterfly_preset_base_layer(self, preset_index: u32) -> u32 {
        self.butterfly_base_layer + preset_index * self.butterfly_layers_per_preset()
    }

    pub const fn total_layer_count(self) -> u32 {
        self.total_layer_count
    }

    pub fn assert_valid(self) {
        assert!(
            self.leaf_layer_count == 1,
            "Particle texture layout must reserve exactly one leaf layer"
        );
        assert!(
            self.butterfly_preset_count > 0
                && self.butterfly_view_count > 0
                && self.butterfly_frames_per_view > 0,
            "Particle texture layout butterfly dimensions must be non-zero"
        );
        assert!(
            self.butterfly_base_layer == self.leaf_layer + self.leaf_layer_count,
            "Particle texture layout butterfly base layer is not contiguous"
        );
        assert!(
            self.total_layer_count == self.butterfly_base_layer + self.butterfly_layer_count(),
            "Particle texture layout total layer count is not contiguous"
        );
    }

    pub fn contains_layer(self, layer: u32) -> bool {
        layer < self.total_layer_count
    }
}
