use crate::renderer::allocator::Allocator;
use ash::vk;
use ash::Device;

pub fn create_and_fill_buffer<T>(
    device: &Device,
    allocator: &mut Allocator,
    data: &[T],
    usage: vk::BufferUsageFlags,
) -> (vk::Buffer, gpu_allocator::vulkan::Allocation)
where
    T: Copy,
{
    let size = std::mem::size_of_val(data);
    let (buffer, mut memory) = allocator.create_buffer(device, size, usage);
    allocator.update_buffer(device, &mut memory, data);
    (buffer, memory)
}
