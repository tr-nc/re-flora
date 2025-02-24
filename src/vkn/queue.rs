
pub struct QueueFamilyIndices {
    /// Guaranteed to support GRAPHICS + PRESENT + COMPUTE + TRANSFER,
    /// and should be used for all main tasks
    pub general: u32,
    /// Exclusive to transfer operations, may be slower, but enables
    /// potential parallelism for background transfer operations
    pub transfer_only: u32,
}

impl QueueFamilyIndices {
    pub fn get_all_indices(&self) -> Vec<u32> {
        vec![self.general, self.transfer_only]
    }
}
