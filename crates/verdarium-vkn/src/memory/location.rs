#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryLocation {
    GpuOnly,
    CpuToGpu,
    GpuToCpu,
}

impl From<MemoryLocation> for gpu_allocator::MemoryLocation {
    fn from(value: MemoryLocation) -> Self {
        match value {
            MemoryLocation::GpuOnly => gpu_allocator::MemoryLocation::GpuOnly,
            MemoryLocation::CpuToGpu => gpu_allocator::MemoryLocation::CpuToGpu,
            MemoryLocation::GpuToCpu => gpu_allocator::MemoryLocation::GpuToCpu,
        }
    }
}
