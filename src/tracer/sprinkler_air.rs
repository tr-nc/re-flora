//! Optical approximation only. The spray/contact producer and its clock are untouched.
use super::{Tracer, SPRINKLER_AIR_GRID_BYTES, SPRINKLER_AIR_GRID_SIDE, SPRINKLER_MIST_LOBES};
use crate::generated::gpu_structs::PushConstantSprinklerAirBounds as PushConstantSprinklerAir;
use anyhow::Result;
use re_flora_vkn::{CommandBuffer, DescriptorResource, Extent3D};

fn air_enabled(selected: bool, flow: bool, density: f32, count: u32, diagnostic: bool) -> bool {
    selected && flow && density > 0.0 && count > 0 && !diagnostic
}

impl Tracer {
    pub(crate) fn sprinkler_air_review_field(
        &self,
    ) -> Result<(crate::generated::gpu_structs::SprinklerAirInfo, Vec<u32>)> {
        anyhow::ensure!(
            self.sprinkler_air_active(),
            "air field readback needs an active prepared frame"
        );
        let info = self.sprinkler_review_buffer_elements(self.sprinkler_resources.air_info(), 1)?;
        let density = self.sprinkler_review_buffer_elements(
            self.sprinkler_resources.air_density(),
            SPRINKLER_AIR_GRID_SIDE.pow(3),
        )?;
        Ok((info[0], density))
    }

    pub(super) fn sprinkler_air_active(&self) -> bool {
        air_enabled(
            self.sprinkler_mist_air,
            self.sprinkler_mist_flow,
            self.sprinkler_mist_density,
            self.sprinkler_resources.instance_count,
            self.sprinkler_optical_diagnostic.is_some(),
        )
    }

    fn sprinkler_air_push(&self, background: bool, shadows: bool) -> PushConstantSprinklerAir {
        PushConstantSprinklerAir {
            count: self.sprinkler_resources.instance_count,
            lobes_per_instance: SPRINKLER_MIST_LOBES,
            grid_side: SPRINKLER_AIR_GRID_SIDE,
            background_enabled: u32::from(background),
            glass_enabled: u32::from(
                self.desc.glass_experiment_enabled || self.desc.dedicated_glass_enabled,
            ),
            shadows_enabled: u32::from(shadows),
            ..bytemuck::Zeroable::zeroed()
        }
    }

    pub(super) fn record_sprinkler_air_field(
        &self,
        cmdbuf: &CommandBuffer,
        slot: usize,
    ) -> Result<()> {
        let pipelines = self.pipeline_topology.compute();
        for pipeline in [
            &pipelines.sprinkler_air_bounds_ppl,
            &pipelines.sprinkler_air_splat_ppl,
        ] {
            pipeline.begin_transient_descriptor_frame(slot);
        }
        let resources = &self.sprinkler_resources;
        // Clear on every active frame, not just allocation: pulse-off, deletion,
        // movement and slot reuse must never retain previous density.
        resources
            .air_density()
            .record_fill(cmdbuf, 0, SPRINKLER_AIR_GRID_BYTES, 0);
        let push = self.sprinkler_air_push(false, false);
        pipelines
            .sprinkler_air_bounds_ppl
            .record_workgroups_with_descriptors(
                cmdbuf,
                &[
                    (
                        "mist_lobes",
                        DescriptorResource::Buffer(resources.mist_lobes()),
                    ),
                    (
                        "sprinkler_air_info",
                        DescriptorResource::Buffer(resources.air_info()),
                    ),
                ],
                Extent3D::new(1, 1, 1),
                Some(bytemuck::bytes_of(&push)),
            )?;
        pipelines
            .sprinkler_air_splat_ppl
            .record_workgroups_with_descriptors(
                cmdbuf,
                &[
                    (
                        "mist_lobes",
                        DescriptorResource::Buffer(resources.mist_lobes()),
                    ),
                    (
                        "sprinkler_air_info",
                        DescriptorResource::Buffer(resources.air_info()),
                    ),
                    (
                        "sprinkler_air_density",
                        DescriptorResource::Buffer(resources.air_density()),
                    ),
                ],
                // 2-D dispatch also covers 256*256 lobes on devices whose per-axis
                // workgroup-count limit is 65535 (a flat 65536 dispatch would be invalid).
                Extent3D::new(push.lobes_per_instance, push.count, 1),
                Some(bytemuck::bytes_of(&push)),
            )
    }

    pub(super) fn record_air_god_ray_pass(
        &self,
        cmdbuf: &CommandBuffer,
        slot: usize,
        background: bool,
        shadows: bool,
    ) -> Result<()> {
        let pipeline = &self.pipeline_topology.compute().god_ray_air_ppl;
        pipeline.begin_transient_descriptor_frame(slot);
        let push = self.sprinkler_air_push(background, shadows);
        pipeline.record_with_descriptors(
            cmdbuf,
            &[
                (
                    "sprinkler_air_info",
                    DescriptorResource::Buffer(self.sprinkler_resources.air_info()),
                ),
                (
                    "sprinkler_air_density",
                    DescriptorResource::Buffer(self.sprinkler_resources.air_density()),
                ),
            ],
            self.resources
                .extent_dependent_resources
                .god_ray_raw_tex
                .get_image()
                .get_desc()
                .extent,
            Some(bytemuck::bytes_of(&push)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn air_passes_share_the_reflected_push_layout() {
        use crate::generated::gpu_structs::{PushConstantGodRayAir, PushConstantSprinklerAirSplat};
        // Six scalar fields plus the reflected 16-byte block-alignment padding.
        assert_eq!(std::mem::size_of::<PushConstantSprinklerAir>(), 32);
        assert_eq!(std::mem::size_of::<PushConstantGodRayAir>(), 32);
        assert_eq!(std::mem::size_of::<PushConstantSprinklerAirSplat>(), 32);
        macro_rules! check {
            ($($field:ident),+) => { $(
                assert_eq!(std::mem::offset_of!(PushConstantSprinklerAir,$field),std::mem::offset_of!(PushConstantGodRayAir,$field));
                assert_eq!(std::mem::offset_of!(PushConstantSprinklerAir,$field),std::mem::offset_of!(PushConstantSprinklerAirSplat,$field));
            )+ };
        }
        check!(
            count,
            lobes_per_instance,
            grid_side,
            background_enabled,
            glass_enabled,
            shadows_enabled
        );
    }

    #[test]
    fn air_switch_only_selects_visible_flowing_mist_not_legacy_or_diagnostic_ablation() {
        for selected in [false, true] {
            for flow in [false, true] {
                for density in [0.0, 2.0, 6.0] {
                    for count in [0, 1, 16, 256] {
                        for diagnostic in [false, true] {
                            assert_eq!(
                                air_enabled(selected, flow, density, count, diagnostic),
                                selected && flow && density > 0.0 && count > 0 && !diagnostic
                            );
                        }
                    }
                }
            }
        }
    }
}
