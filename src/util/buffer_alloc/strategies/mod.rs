#![allow(dead_code)]

use super::BufferAllocation;

mod first_fit;
pub use first_fit::*;

pub trait AllocationStrategy {
    /// Allocates a continuous block of memory of `req_size` bytes.
    ///
    /// Returns the allocation record if successful.
    fn allocate(&mut self, req_size: u64) -> Result<BufferAllocation, String>;

    /// Looks up an allocation by its unique id.
    fn lookup(&self, id: u64) -> Option<BufferAllocation>;

    /// Deallocates the allocation with the given identifier.
    fn deallocate(&mut self, id: u64) -> Result<(), String>;

    /// Cleans up the pool by compacting allocations.
    ///
    /// After cleanup all allocated blocks will be contiguous.
    fn cleanup(&mut self);

    /// Resets the allocator, clearing all allocations.
    fn reset(&mut self);

    /// Resize an existing allocation `id` to `to_size` bytes.
    ///
    /// - If `to_size <= old_size`, shrinks in place (offset unchanged).
    /// - If `to_size > old_size`, first tries to expand in place if
    ///   there is a free block immediately after; otherwise moves
    ///   the block to a new region (offset may change).
    fn resize(&mut self, id: u64, to_size: u64) -> Result<BufferAllocation, String>;
}