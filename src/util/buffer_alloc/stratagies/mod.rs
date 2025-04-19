#![allow(dead_code)]

use super::Allocation;

mod first_fit;
pub use first_fit::*;

pub trait AllocationStrategy {
    /// Allocates a continuous block of memory of `req_size` bytes.
    ///
    /// Returns the allocation record if successful.
    fn allocate(&mut self, req_size: u64) -> Result<Allocation, String>;

    /// Looks up an allocation by its unique id.
    fn lookup(&self, id: u64) -> Option<Allocation>;

    /// Deallocates the allocation with the given identifier.
    fn deallocate(&mut self, id: u64) -> Result<(), String>;

    /// Cleans up the pool by compacting allocations.
    ///
    /// After cleanup all allocated blocks will be contiguous.
    fn cleanup(&mut self);

    /// Resets the allocator, clearing all allocations.
    fn reset(&mut self);
}
