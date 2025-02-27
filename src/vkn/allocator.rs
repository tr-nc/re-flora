use super::Device;
use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator as GpuAllocator},
    MemoryLocation,
};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct Allocator {
    device: Device,
    pub allocator: Arc<Mutex<GpuAllocator>>,
}

impl Allocator {
    pub fn new(device: &Device, allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>) -> Self {
        Self {
            device: device.clone(),
            allocator,
        }
    }

    fn get_allocator(&self) -> MutexGuard<GpuAllocator> {
        self.allocator.lock().unwrap()
    }

    pub fn create_buffer(
        &mut self,
        size: usize,
        usage: vk::BufferUsageFlags,
    ) -> (vk::Buffer, Allocation) {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size as _)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let mut allocator = self.get_allocator();

        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name: "",
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate buffer memory");

        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .unwrap()
        };

        (buffer, allocation)
    }

    pub fn create_image(&mut self, width: u32, height: u32) -> (vk::Image, Allocation) {
        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_SRGB)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let image = unsafe { self.device.create_image(&image_info, None).unwrap() };
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };

        let mut allocator = self.get_allocator();

        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name: "",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate image memory");

        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .unwrap()
        };

        (image, allocation)
    }

    pub fn destroy_buffer(&mut self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free buffer memory");
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    pub fn destroy_image(&mut self, image: vk::Image, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free image memory");
        unsafe { self.device.destroy_image(image, None) };
    }

    pub fn update_buffer<T: Copy>(&mut self, memory: &mut Allocation, data: &[T]) {
        let size = std::mem::size_of_val(data) as _;
        unsafe {
            let data_ptr = memory.mapped_ptr().unwrap().as_ptr();
            let mut align = ash::util::Align::new(data_ptr, std::mem::align_of::<T>() as _, size);
            align.copy_from_slice(data);
        };
    }
}
