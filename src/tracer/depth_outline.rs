//! Scene-wide depth outlines. Owns execution and parameter safety; the filter
//! owns depth reconstruction, slope rejection and foreground-only coverage.
//! Call after scene composition/glass resolve, before upscaling and native UI.
use re_flora_vkn::{
    CommandBuffer, ComputePipeline, DescriptorPool, Device, Extent3D, ResourceContainer,
    ShaderModule,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub strength: f32,
    pub color: [f32; 3], // sRGB, matching the saved color picker
    pub relative_threshold: f32,
    pub minimum_gap: f32, // world units, not device depth
    pub softness: f32,
    pub sky_strength: f32,
    pub thin_strength: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.25,
            color: [20. / 255., 30. / 255., 36. / 255.],
            relative_threshold: 0.04,
            minimum_gap: 0.004,
            softness: 0.5,
            sky_strength: 0.6,
            thin_strength: 0.25,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Parameters {
    color_strength: [f32; 4],
    thresholds: [f32; 4],
    detail: [f32; 4],
}
fn bounded(value: f32, fallback: f32, lo: f32, hi: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        fallback
    }
}
impl Settings {
    fn parameters(self) -> Parameters {
        let d = Self::default();
        Parameters {
            color_strength: [
                bounded(self.color[0], d.color[0], 0., 1.),
                bounded(self.color[1], d.color[1], 0., 1.),
                bounded(self.color[2], d.color[2], 0., 1.),
                bounded(self.strength, d.strength, 0., 1.),
            ],
            thresholds: [
                bounded(self.relative_threshold, d.relative_threshold, 0.0001, 1.),
                bounded(self.minimum_gap, d.minimum_gap, 0., 1.),
                bounded(self.softness, d.softness, 0.01, 2.),
                0.,
            ],
            detail: [
                bounded(self.sky_strength, d.sky_strength, 0., 1.),
                bounded(self.thin_strength, d.thin_strength, 0., 1.),
                0.,
                0.,
            ],
        }
    }
}
pub(super) struct DepthOutline {
    pipeline: ComputePipeline,
}
impl DepthOutline {
    pub fn new(
        device: &Device,
        shader: &ShaderModule,
        pool: &DescriptorPool,
        resources: &[&dyn ResourceContainer],
    ) -> Self {
        Self {
            pipeline: ComputePipeline::new(device, shader, pool, resources),
        }
    }
    // Only the topology's coordinated extent publication needs this seam.
    pub fn pipeline(&self) -> &ComputePipeline {
        &self.pipeline
    }
    pub fn record(&self, command: &CommandBuffer, extent: Extent3D, settings: Settings) {
        let params = settings.parameters();
        if settings.enabled && params.color_strength[3] > 0. {
            self.pipeline
                .record(command, extent, Some(bytemuck::bytes_of(&params)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_saved_parameters_cannot_poison_the_shader() {
        let mut settings = Settings::default();
        settings.strength = f32::NAN;
        settings.color = [f32::INFINITY, -1., 2.];
        settings.relative_threshold = 0.;
        settings.softness = -2.;
        let p = settings.parameters();
        assert_eq!(p.color_strength[3], 0.25);
        assert_eq!(&p.color_strength[1..3], &[0., 1.]);
        assert!(p.thresholds[0] > 0. && p.thresholds[2] > 0.);
        assert!(bytemuck::cast_slice::<_, f32>(std::slice::from_ref(&p))
            .iter()
            .all(|v| v.is_finite()));
        assert_eq!(std::mem::size_of::<Parameters>(), 48);
    }
}
