//! Image budgets belong to the complete pipeline layout, not individual sets or
//! read/write access policy. Read-only storage images still consume storage slots.
use crate::DescriptorSetLayoutBinding;
use anyhow::{ensure, Result};
use ash::vk;

/// These layouts do not use UPDATE_AFTER_BIND_POOL. Count array elements and all
/// sets, once for the total and once for each stage that can access the binding.
pub(super) fn validate_image_descriptor_limits(
    bindings: &[DescriptorSetLayoutBinding],
    limits: &vk::PhysicalDeviceLimits,
) -> Result<()> {
    for (types, per_stage_name, per_stage, total_name, total_limit) in [
        (
            &[
                vk::DescriptorType::STORAGE_IMAGE,
                vk::DescriptorType::STORAGE_TEXEL_BUFFER,
            ][..],
            "maxPerStageDescriptorStorageImages",
            limits.max_per_stage_descriptor_storage_images,
            "maxDescriptorSetStorageImages",
            limits.max_descriptor_set_storage_images,
        ),
        (
            &[
                vk::DescriptorType::SAMPLED_IMAGE,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
            ][..],
            "maxPerStageDescriptorSampledImages",
            limits.max_per_stage_descriptor_sampled_images,
            "maxDescriptorSetSampledImages",
            limits.max_descriptor_set_sampled_images,
        ),
    ] {
        let mut by_stage = [0_u64; 32];
        let mut total = 0_u64;
        for binding in bindings
            .iter()
            .filter(|b| types.contains(&b.descriptor_type))
        {
            let count = u64::from(binding.descriptor_count);
            total += count;
            for (bit, stage_count) in by_stage.iter_mut().enumerate() {
                if binding.stage_flags.as_raw() & (1 << bit) != 0 {
                    *stage_count += count;
                }
            }
        }
        for (bit, count) in by_stage.into_iter().enumerate() {
            ensure!(count <= u64::from(per_stage),
                "pipeline image descriptors exceed {per_stage_name}: stage={:?} required={count} limit={per_stage}",
                vk::ShaderStageFlags::from_raw(1 << bit));
        }
        ensure!(
            total <= u64::from(total_limit),
            "pipeline image descriptors exceed {total_name}: required={total} limit={total_limit}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DescriptorAccess;

    fn limits() -> vk::PhysicalDeviceLimits {
        vk::PhysicalDeviceLimits {
            max_per_stage_descriptor_storage_images: 8,
            max_descriptor_set_storage_images: 16,
            max_per_stage_descriptor_sampled_images: 16,
            max_descriptor_set_sampled_images: 32,
            ..Default::default()
        }
    }
    fn binding(
        kind: vk::DescriptorType,
        count: u32,
        stages: vk::ShaderStageFlags,
    ) -> DescriptorSetLayoutBinding {
        DescriptorSetLayoutBinding {
            no: 0,
            name: "image".to_owned(),
            descriptor_type: kind,
            descriptor_count: count,
            stage_flags: stages,
            access: DescriptorAccess::ReadOnly,
        }
    }
    #[test]
    fn read_only_storage_images_and_texel_arrays_share_the_same_stage_budget() {
        let images = binding(
            vk::DescriptorType::STORAGE_IMAGE,
            6,
            vk::ShaderStageFlags::COMPUTE,
        );
        let texels = binding(
            vk::DescriptorType::STORAGE_TEXEL_BUFFER,
            2,
            vk::ShaderStageFlags::COMPUTE,
        );
        assert!(
            validate_image_descriptor_limits(&[images.clone(), texels.clone()], &limits()).is_ok()
        );
        let error = validate_image_descriptor_limits(
            &[
                images,
                DescriptorSetLayoutBinding {
                    descriptor_count: 3,
                    ..texels
                },
            ],
            &limits(),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("maxPerStageDescriptorStorageImages"));
        assert!(error.to_string().contains("required=9 limit=8"));
    }
    #[test]
    fn distinct_stages_have_independent_budgets_but_share_the_layout_total() {
        let bindings = [
            binding(
                vk::DescriptorType::STORAGE_IMAGE,
                6,
                vk::ShaderStageFlags::VERTEX,
            ),
            binding(
                vk::DescriptorType::STORAGE_IMAGE,
                6,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ];
        assert!(validate_image_descriptor_limits(&bindings, &limits()).is_ok());
        let small = vk::PhysicalDeviceLimits {
            max_descriptor_set_storage_images: 11,
            ..limits()
        };
        assert!(validate_image_descriptor_limits(&bindings, &small)
            .unwrap_err()
            .to_string()
            .contains("maxDescriptorSetStorageImages"));
        let shared = binding(
            vk::DescriptorType::STORAGE_IMAGE,
            8,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        );
        assert!(validate_image_descriptor_limits(
            &[shared],
            &vk::PhysicalDeviceLimits {
                max_descriptor_set_storage_images: 8,
                ..limits()
            }
        )
        .is_ok());
    }
    #[test]
    fn sampled_and_combined_images_use_the_sampled_budget_not_the_storage_budget() {
        let bindings = [
            binding(
                vk::DescriptorType::STORAGE_IMAGE,
                8,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                vk::DescriptorType::SAMPLED_IMAGE,
                8,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                8,
                vk::ShaderStageFlags::COMPUTE,
            ),
        ];
        assert!(validate_image_descriptor_limits(&bindings, &limits()).is_ok());
        let small = vk::PhysicalDeviceLimits {
            max_per_stage_descriptor_sampled_images: 15,
            ..limits()
        };
        assert!(validate_image_descriptor_limits(&bindings, &small)
            .unwrap_err()
            .to_string()
            .contains("maxPerStageDescriptorSampledImages"));
    }
    #[test]
    fn counts_across_sets_cannot_overflow_or_be_deduplicated_by_binding_number() {
        let huge = binding(
            vk::DescriptorType::STORAGE_IMAGE,
            u32::MAX,
            vk::ShaderStageFlags::COMPUTE,
        );
        let error = validate_image_descriptor_limits(&[huge.clone(), huge], &limits()).unwrap_err();
        assert!(error.to_string().contains("required=8589934590"));
        assert!(
            validate_image_descriptor_limits(&[], &vk::PhysicalDeviceLimits::default()).is_ok()
        );
    }
}
