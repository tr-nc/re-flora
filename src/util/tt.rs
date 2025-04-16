use std::collections::HashMap;

/// Represents an allocation record.
#[derive(Debug, Clone)]
pub struct Allocation {
    /// A unique identifier for this allocation.
    pub id: u64,
    /// The starting offset of the allocation within the pool.
    pub offset: usize,
    /// The size of the allocation in bytes.
    pub size: usize,
}

/// Represents a free block within the pool.
#[derive(Debug, Clone)]
pub struct FreeBlock {
    /// The starting offset of the free block.
    pub offset: usize,
    /// The size in bytes of the free block.
    pub size: usize,
}

/// The trait that abstracts allocation strategies.
pub trait AllocationStrategy {
    /// Allocates a continuous block of memory of `req_size` bytes.
    ///
    /// Returns the allocation information if successful.
    fn allocate(&mut self, req_size: usize) -> Result<Allocation, String>;

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

/// A first-fit allocator that manages free memory regions using a free list.
pub struct FirstFitAllocator {
    total_size: usize,
    allocated: HashMap<u64, Allocation>,
    free_list: Vec<FreeBlock>,
    next_id: u64,
}

impl FirstFitAllocator {
    /// Creates a new first-fit allocator with the given total size (in bytes).
    ///
    /// Note that this does not allocate any memory; it only manages allocation metadata.
    pub fn new(total_size: usize) -> Self {
        let free_list = vec![FreeBlock {
            offset: 0,
            size: total_size,
        }];
        FirstFitAllocator {
            total_size,
            allocated: HashMap::new(),
            free_list,
            next_id: 1,
        }
    }

    /// Helper function to coalesce adjacent free blocks.
    fn coalesce_free_list(&mut self) {
        self.free_list.sort_by_key(|block| block.offset);
        let mut merged: Vec<FreeBlock> = Vec::new();
        for block in self.free_list.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.offset + last.size == block.offset {
                    last.size += block.size;
                } else {
                    merged.push(block);
                }
            } else {
                merged.push(block);
            }
        }
        self.free_list = merged;
    }
}

impl AllocationStrategy for FirstFitAllocator {
    fn allocate(&mut self, req_size: usize) -> Result<Allocation, String> {
        // First-fit: iterate through free_list and pick the first block that fits.
        for i in 0..self.free_list.len() {
            if self.free_list[i].size >= req_size {
                let alloc_offset = self.free_list[i].offset;
                if self.free_list[i].size == req_size {
                    self.free_list.remove(i);
                } else {
                    self.free_list[i].offset += req_size;
                    self.free_list[i].size -= req_size;
                }
                let id = self.next_id;
                self.next_id += 1;
                let allocation = Allocation {
                    id,
                    offset: alloc_offset,
                    size: req_size,
                };
                self.allocated.insert(id, allocation.clone());
                return Ok(allocation);
            }
        }
        Err("Not enough free memory".to_string())
    }

    fn lookup(&self, id: u64) -> Option<Allocation> {
        self.allocated.get(&id).cloned()
    }

    fn deallocate(&mut self, id: u64) -> Result<(), String> {
        if let Some(allocation) = self.allocated.remove(&id) {
            let freed_block = FreeBlock {
                offset: allocation.offset,
                size: allocation.size,
            };
            self.free_list.push(freed_block);
            self.coalesce_free_list();
            Ok(())
        } else {
            Err("Allocation id not found".to_string())
        }
    }

    fn cleanup(&mut self) {
        // Gather mutable references to allocated blocks, sort by offset.
        let mut allocations: Vec<&mut Allocation> = self.allocated.values_mut().collect();
        allocations.sort_by_key(|alloc| alloc.offset);
        let mut current_offset = 0;
        for alloc in allocations.iter_mut() {
            alloc.offset = current_offset;
            current_offset += alloc.size;
        }
        self.free_list.clear();
        if current_offset < self.total_size {
            self.free_list.push(FreeBlock {
                offset: current_offset,
                size: self.total_size - current_offset,
            });
        }
    }

    fn reset(&mut self) {
        self.allocated.clear();
        self.free_list.clear();
        self.free_list.push(FreeBlock {
            offset: 0,
            size: self.total_size,
        });
        self.next_id = 1;
    }
}

/// A best-fit allocator implementation which always chooses the smallest free block
/// that fits the requested size.
pub struct BestFitAllocator {
    total_size: usize,
    allocated: HashMap<u64, Allocation>,
    free_list: Vec<FreeBlock>,
    next_id: u64,
}

impl BestFitAllocator {
    /// Creates a new best-fit allocator with the given total size (in bytes).
    pub fn new(total_size: usize) -> Self {
        let free_list = vec![FreeBlock {
            offset: 0,
            size: total_size,
        }];
        BestFitAllocator {
            total_size,
            allocated: HashMap::new(),
            free_list,
            next_id: 1,
        }
    }

    /// Helper function to coalesce adjacent free blocks.
    fn coalesce_free_list(&mut self) {
        self.free_list.sort_by_key(|block| block.offset);
        let mut merged: Vec<FreeBlock> = Vec::new();
        for block in self.free_list.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.offset + last.size == block.offset {
                    last.size += block.size;
                } else {
                    merged.push(block);
                }
            } else {
                merged.push(block);
            }
        }
        self.free_list = merged;
    }
}

impl AllocationStrategy for BestFitAllocator {
    fn allocate(&mut self, req_size: usize) -> Result<Allocation, String> {
        // Best-fit: choose the free block with the smallest size that is large enough.
        let mut best_index: Option<usize> = None;
        let mut best_size = usize::MAX;
        for (i, block) in self.free_list.iter().enumerate() {
            if block.size >= req_size && block.size < best_size {
                best_index = Some(i);
                best_size = block.size;
            }
        }

        if let Some(i) = best_index {
            let alloc_offset = self.free_list[i].offset;
            if self.free_list[i].size == req_size {
                self.free_list.remove(i);
            } else {
                self.free_list[i].offset += req_size;
                self.free_list[i].size -= req_size;
            }
            let id = self.next_id;
            self.next_id += 1;
            let allocation = Allocation {
                id,
                offset: alloc_offset,
                size: req_size,
            };
            self.allocated.insert(id, allocation.clone());
            return Ok(allocation);
        }
        Err("Not enough free memory".to_string())
    }

    fn lookup(&self, id: u64) -> Option<Allocation> {
        self.allocated.get(&id).cloned()
    }

    fn deallocate(&mut self, id: u64) -> Result<(), String> {
        if let Some(allocation) = self.allocated.remove(&id) {
            self.free_list.push(FreeBlock {
                offset: allocation.offset,
                size: allocation.size,
            });
            self.coalesce_free_list();
            Ok(())
        } else {
            Err("Allocation id not found".to_string())
        }
    }

    fn cleanup(&mut self) {
        let mut allocations: Vec<&mut Allocation> = self.allocated.values_mut().collect();
        allocations.sort_by_key(|alloc| alloc.offset);
        let mut current_offset = 0;
        for alloc in allocations.iter_mut() {
            alloc.offset = current_offset;
            current_offset += alloc.size;
        }
        self.free_list.clear();
        if current_offset < self.total_size {
            self.free_list.push(FreeBlock {
                offset: current_offset,
                size: self.total_size - current_offset,
            });
        }
    }

    fn reset(&mut self) {
        self.allocated.clear();
        self.free_list.clear();
        self.free_list.push(FreeBlock {
            offset: 0,
            size: self.total_size,
        });
        self.next_id = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Common test logic for an allocator.
    fn exercise_allocator<A: AllocationStrategy>(allocator: &mut A) {
        // Allocate 200 bytes.
        let alloc1 = allocator.allocate(200).unwrap();
        assert_eq!(alloc1.size, 200);
        assert_eq!(alloc1.offset, 0);

        // Allocate 300 bytes.
        let alloc2 = allocator.allocate(300).unwrap();
        assert_eq!(alloc2.offset, 200);

        // Allocate 100 bytes.
        let alloc3 = allocator.allocate(100).unwrap();
        assert_eq!(alloc3.offset, 500);

        // Lookup allocation alloc2.
        let lookup2 = allocator.lookup(alloc2.id).unwrap();
        assert_eq!(lookup2.size, 300);
        assert_eq!(lookup2.offset, alloc2.offset);

        // Deallocate allocation alloc2.
        allocator.deallocate(alloc2.id).unwrap();

        // Allocate 250 bytes; this should reuse freed memory.
        let alloc4 = allocator.allocate(250).unwrap();
        // For both strategies, since alloc2 was freed at offset 200 with size 300,
        // the new allocation should typically start at 200.
        assert_eq!(alloc4.offset, 200);

        // Verify reset: after reset no previous allocation should be found.
        allocator.reset();
        assert!(allocator.lookup(alloc1.id).is_none());
        let alloc_reset = allocator.allocate(100).unwrap();
        assert_eq!(alloc_reset.offset, 0);
    }

    #[test]
    fn test_first_fit_allocator() {
        let total_size = 1000;
        let mut allocator = FirstFitAllocator::new(total_size);
        exercise_allocator(&mut allocator);
    }

    #[test]
    fn test_best_fit_allocator() {
        let total_size = 1000;
        let mut allocator = BestFitAllocator::new(total_size);
        exercise_allocator(&mut allocator);
    }

    #[test]
    fn test_cleanup_first_fit() {
        let total_size = 1000;
        let mut allocator = FirstFitAllocator::new(total_size);
        let alloc1 = allocator.allocate(100).unwrap(); // offset 0..100
        let alloc2 = allocator.allocate(200).unwrap(); // offset 100..300
        let alloc3 = allocator.allocate(150).unwrap(); // offset 300..450

        // Deallocate the second block to create fragmentation.
        allocator.deallocate(alloc2.id).unwrap();

        // Allocate a new block that fits into the freed space.
        let alloc4 = allocator.allocate(150).unwrap(); // Expected at offset 100.
        assert_eq!(alloc4.offset, 100);

        // Cleanup (compact the memory).
        allocator.cleanup();

        let lookup1 = allocator.lookup(alloc1.id).unwrap();
        let lookup4 = allocator.lookup(alloc4.id).unwrap();
        let lookup3 = allocator.lookup(alloc3.id).unwrap();

        // After cleanup the blocks become contiguous.
        assert_eq!(lookup1.offset, 0);
        assert_eq!(lookup4.offset, 100);
        assert_eq!(lookup3.offset, 250);
    }

    #[test]
    fn benchmark_allocation_strategies() {
        // Benchmark the first-fit vs. best-fit allocation performance.
        let total_size = 1_000_000;
        let iterations = 1000;
        let allocation_size = 1000;

        // Benchmark First-Fit.
        let mut first_fit = FirstFitAllocator::new(total_size);
        let start_ff = Instant::now();
        let mut ids_ff = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let alloc = first_fit.allocate(allocation_size).unwrap();
            ids_ff.push(alloc.id);
        }
        for id in ids_ff {
            first_fit.deallocate(id).unwrap();
        }
        let duration_ff = start_ff.elapsed();

        // Benchmark Best-Fit.
        let mut best_fit = BestFitAllocator::new(total_size);
        let start_bf = Instant::now();
        let mut ids_bf = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let alloc = best_fit.allocate(allocation_size).unwrap();
            ids_bf.push(alloc.id);
        }
        for id in ids_bf {
            best_fit.deallocate(id).unwrap();
        }
        let duration_bf = start_bf.elapsed();

        println!("First-Fit allocation duration: {:?}", duration_ff);
        println!("Best-Fit allocation duration: {:?}", duration_bf);

        // Even though the durations may be very similar for a small test case,
        // different strategies might perform differently under heavy fragmentation.
    }
}
