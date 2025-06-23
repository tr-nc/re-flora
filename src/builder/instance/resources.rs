use ash::vk;

use crate::vkn::{AccelStruct, Allocator, Buffer, BufferUsage, ShaderModule, VulkanContext};

pub struct InstanceResources {
    pub instance_build_info: Buffer,
    pub instances: Buffer,
}

impl InstanceResources {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        instance_maker_sm: &ShaderModule,
        instance_cap: u64,
    ) -> Self {
        let device = vulkan_ctx.device();

        let instance_info_layout = instance_maker_sm
            .get_buffer_layout("U_InstanceInfo")
            .unwrap();
        let instance_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            instance_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let instances_layout = instance_maker_sm.get_buffer_layout("B_Instances").unwrap();
        let instances = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            instances_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
            instance_cap,
        );

        Self {
            instance_build_info: instance_info,
            instances,
        }
    }
}
