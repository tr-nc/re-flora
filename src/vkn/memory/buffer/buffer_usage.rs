use ash::vk;
use spirv_reflect::types::ReflectDescriptorType;

#[derive(Debug, Clone, Copy)]
pub struct BufferUsage {
    usage: vk::BufferUsageFlags,
}

impl BufferUsage {
    pub fn empty() -> Self {
        Self {
            usage: vk::BufferUsageFlags::empty(),
        }
    }

    pub fn from_flags(usage: vk::BufferUsageFlags) -> Self {
        Self { usage }
    }

    pub fn union_with(&mut self, other: &Self) {
        self.usage |= other.usage;
    }

    pub fn from_reflect_descriptor_type(reflect_descriptor_type: ReflectDescriptorType) -> Self {
        use ReflectDescriptorType::*;
        let usage = match reflect_descriptor_type {
            Undefined => {
                log::error!(
                    "ReflectDescriptorType::Undefined encountered. Defaulting to empty usage."
                );
                vk::BufferUsageFlags::empty()
            }
            // these descriptor types do not directly map to a buffer usage.
            Sampler => {
                log::error!("ReflectDescriptorType::Sampler does not map to a buffer usage flag.");
                vk::BufferUsageFlags::empty()
            }
            CombinedImageSampler => {
                log::error!("ReflectDescriptorType::CombinedImageSampler does not map to a buffer usage flag.");
                vk::BufferUsageFlags::empty()
            }
            SampledImage => {
                log::error!(
                    "ReflectDescriptorType::SampledImage does not map to a buffer usage flag."
                );
                vk::BufferUsageFlags::empty()
            }
            StorageImage => {
                log::error!(
                    "ReflectDescriptorType::StorageImage does not map to a buffer usage flag."
                );
                vk::BufferUsageFlags::empty()
            }
            // these variants map directly to buffer usage flags.
            UniformTexelBuffer => vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER,
            StorageTexelBuffer => vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER,
            UniformBuffer | UniformBufferDynamic => vk::BufferUsageFlags::UNIFORM_BUFFER,
            StorageBuffer | StorageBufferDynamic => vk::BufferUsageFlags::STORAGE_BUFFER,
            // input attachments are intended for images/framebuffers.
            InputAttachment => {
                log::error!(
                    "ReflectDescriptorType::InputAttachment does not map to a buffer usage flag."
                );
                vk::BufferUsageFlags::empty()
            }
            // unsupported types
            AccelerationStructureKHR => {
                log::error!(
                    "ReflectDescriptorType::AccelerationStructureKHR is currently not supported."
                );
                vk::BufferUsageFlags::empty()
            }
        };

        Self::from_flags(usage)
    }

    pub fn as_raw(&self) -> vk::BufferUsageFlags {
        self.usage
    }
}
