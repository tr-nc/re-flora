use ash::{vk, Device};
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator as GpuAllocator},
    MemoryLocation,
};
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Allocator {
    pub allocator: Arc<Mutex<GpuAllocator>>,
}

impl Allocator {
    pub fn new(allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>) -> Self {
        Self { allocator }
    }

    fn get_allocator(&self) -> MutexGuard<GpuAllocator> {
        self.allocator.lock().unwrap()
    }

    /// Creates a Vulkan buffer.
    pub fn create_buffer(
        &mut self,
        device: &Device,
        size: usize,
        usage: vk::BufferUsageFlags,
    ) -> (vk::Buffer, Allocation) {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size as _)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

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
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .unwrap()
        };

        (buffer, allocation)
    }

    /// Create a Vulkan image.
    ///
    /// This creates a 2D RGBA8_SRGB image with TRANSFER_DST and SAMPLED flags.
    ///
    /// # Arguments
    ///
    /// * `device` - A reference to Vulkan device.
    /// * `width` - The width of the image to create.
    /// * `height` - The height of the image to create.
    pub fn create_image(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
    ) -> (vk::Image, Allocation) {
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

        let image = unsafe { device.create_image(&image_info, None).unwrap() };
        let requirements = unsafe { device.get_image_memory_requirements(image) };

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
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .unwrap()
        };

        (image, allocation)
    }

    /// Destroys a buffer.
    ///
    /// # Arguments
    ///
    /// * `device` - A reference to Vulkan device.
    /// * `buffer` - The buffer to destroy.
    pub fn destroy_buffer(&mut self, device: &Device, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free buffer memory");
        unsafe { device.destroy_buffer(buffer, None) };
    }

    /// Destroys an image.
    ///
    /// # Arguments
    ///
    /// * `device` - A reference to Vulkan device.
    /// * `image` - The image to destroy.
    pub fn destroy_image(&mut self, device: &Device, image: vk::Image, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free image memory");
        unsafe { device.destroy_image(image, None) };
    }

    /// Update buffer data
    ///
    /// # Arguments
    ///
    /// * `device` - A reference to Vulkan device.
    /// * `data` - The data to update the buffer with.
    pub fn update_buffer<T: Copy>(
        &mut self,
        _device: &Device,
        memory: &mut Allocation,
        data: &[T],
    ) {
        let size = std::mem::size_of_val(data) as _;
        unsafe {
            let data_ptr = memory.mapped_ptr().unwrap().as_ptr();
            let mut align = ash::util::Align::new(data_ptr, std::mem::align_of::<T>() as _, size);
            align.copy_from_slice(data);
        };
    }
}
