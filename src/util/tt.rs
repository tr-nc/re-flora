use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Allocation {
    pub id: u64,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct FreeBlock {
    pub offset: usize,
    pub size: usize,
}

/// The trait that abstracts allocation strategies.
pub trait AllocationStrategy {
    /// Allocates a continuous block of memory of `req_size` bytes.
    ///
    /// Returns the allocation record if successful.
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
    /// Note: This does not allocate the actual memory.
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

    /// Helper function to merge adjacent free blocks.
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
        // First-fit: find the first free block that is large enough.
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
        // Repack all allocated blocks so that they become contiguous.
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

/// A best-fit allocator implementation which chooses the smallest free block
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

    /// Helper function to merge adjacent free blocks.
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
        // Best-fit: choose the smallest free block that fits the request.
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
    use rand::seq::SliceRandom;
    use rand::Rng;
    use std::time::Instant;

    #[test]
    fn test_first_fit_allocator() {
        let total_size = 1000;
        let mut allocator = FirstFitAllocator::new(total_size);
        // Allocate 200 bytes.
        let alloc1 = allocator.allocate(200).unwrap();
        assert_eq!(alloc1.size, 200);
        assert_eq!(alloc1.offset, 0);

        // Allocate 300 bytes.
        let alloc2 = allocator.allocate(300).unwrap();
        assert_eq!(alloc2.offset, 200);

        // Allocate additional 100 bytes.
        let alloc3 = allocator.allocate(100).unwrap();
        assert_eq!(alloc3.offset, 500);

        // Lookup allocation alloc2.
        let lookup2 = allocator.lookup(alloc2.id).unwrap();
        assert_eq!(lookup2.size, 300);
        assert_eq!(lookup2.offset, alloc2.offset);

        // Deallocate allocation alloc2.
        allocator.deallocate(alloc2.id).unwrap();

        // Allocate 250 bytes; reused the freed area.
        let alloc4 = allocator.allocate(250).unwrap();
        assert_eq!(alloc4.offset, 200);

        // Reset the allocator.
        allocator.reset();
        assert!(allocator.lookup(alloc1.id).is_none());
        let alloc_reset = allocator.allocate(100).unwrap();
        assert_eq!(alloc_reset.offset, 0);
    }

    #[test]
    fn test_best_fit_allocator() {
        let total_size = 1000;
        let mut allocator = BestFitAllocator::new(total_size);
        // Test similar to first-fit.
        let alloc1 = allocator.allocate(200).unwrap();
        assert_eq!(alloc1.size, 200);
        assert_eq!(alloc1.offset, 0);

        let alloc2 = allocator.allocate(300).unwrap();
        assert_eq!(alloc2.offset, 200);

        let alloc3 = allocator.allocate(100).unwrap();
        assert_eq!(alloc3.offset, 500);

        let lookup2 = allocator.lookup(alloc2.id).unwrap();
        assert_eq!(lookup2.size, 300);
        assert_eq!(lookup2.offset, alloc2.offset);

        allocator.deallocate(alloc2.id).unwrap();
        let alloc4 = allocator.allocate(250).unwrap();
        assert_eq!(alloc4.offset, 200);

        allocator.reset();
        assert!(allocator.lookup(alloc1.id).is_none());
        let alloc_reset = allocator.allocate(100).unwrap();
        assert_eq!(alloc_reset.offset, 0);
    }

    #[test]
    fn test_cleanup_first_fit() {
        let total_size = 1000;
        let mut allocator = FirstFitAllocator::new(total_size);
        let alloc1 = allocator.allocate(100).unwrap(); // offset 0..100
        let alloc2 = allocator.allocate(200).unwrap(); // offset 100..300
        let alloc3 = allocator.allocate(150).unwrap(); // offset 300..450

        allocator.deallocate(alloc2.id).unwrap();
        let alloc4 = allocator.allocate(150).unwrap(); // Expected at offset 100.
        assert_eq!(alloc4.offset, 100);

        allocator.cleanup();

        let lookup1 = allocator.lookup(alloc1.id).unwrap();
        let lookup4 = allocator.lookup(alloc4.id).unwrap();
        let lookup3 = allocator.lookup(alloc3.id).unwrap();

        // After cleanup allocations should be repacked contiguously.
        assert_eq!(lookup1.offset, 0);
        assert_eq!(lookup4.offset, 100);
        assert_eq!(lookup3.offset, 250);
    }

    #[test]
    fn benchmark_allocation_strategies() {
        // Configurable parameters:
        let pool_size: usize = 4 * 1024 * 1024 * 1024; // 4GB pool size
        let initial_allocations: usize = 1000;
        let iterations: usize = 100;
        let min_alloc_size: usize = 2 * 1024 * 1024; // 2MB
        let max_alloc_size: usize = 5 * 1024 * 1024; // 15MB

        // --- Benchmark First-Fit Allocator ---
        {
            let mut allocator = FirstFitAllocator::new(pool_size);
            let mut allocations: Vec<Allocation> = Vec::with_capacity(initial_allocations);
            let mut rng = rand::rng();

            // Initial allocations.
            for _ in 0..initial_allocations {
                let alloc_size = rng.random_range(min_alloc_size..=max_alloc_size);
                let alloc = allocator.allocate(alloc_size).unwrap();
                allocations.push(alloc);
            }

            let start_ff = Instant::now();

            for _ in 0..iterations {
                // Randomly determine the number of allocations to deallocate (between 1 and 8).
                let num_to_remove = rng.random_range(1..=8);
                if allocations.len() < num_to_remove {
                    break;
                }
                // Choose random unique indices from the current allocations.
                let mut indices: Vec<usize> = (0..allocations.len()).collect();
                indices.shuffle(&mut rng);
                let mut dealloc_indices: Vec<usize> =
                    indices.into_iter().take(num_to_remove).collect();
                dealloc_indices.sort_by(|a, b| b.cmp(a)); // sort descending for safe removal

                // Deallocate the selected allocations.
                for i in dealloc_indices.iter() {
                    let alloc = allocations.remove(*i);
                    allocator.deallocate(alloc.id).unwrap();
                }

                // Allocate new blocks with random sizes to replace the ones removed.
                for _ in 0..num_to_remove {
                    let alloc_size = rng.random_range(min_alloc_size..=max_alloc_size);
                    let alloc = allocator.allocate(alloc_size).unwrap();
                    allocations.push(alloc);
                }
            }
            let duration_ff = start_ff.elapsed();
            println!(
                "First-Fit Benchmark Avg Time: {:?}",
                duration_ff / iterations as u32
            );
            println!("Free List size: {}", allocator.free_list.len());
        }

        // --- Benchmark Best-Fit Allocator ---
        {
            let mut allocator = BestFitAllocator::new(pool_size);
            let mut allocations: Vec<Allocation> = Vec::with_capacity(initial_allocations);
            let mut rng = rand::rng();

            // Initial allocations.
            for _ in 0..initial_allocations {
                let alloc_size = rng.random_range(min_alloc_size..=max_alloc_size);
                let alloc = allocator.allocate(alloc_size).unwrap();
                allocations.push(alloc);
            }

            let start_bf = Instant::now();

            for _ in 0..iterations {
                let num_to_remove = rng.random_range(1..=8);
                if allocations.len() < num_to_remove {
                    break;
                }
                let mut indices: Vec<usize> = (0..allocations.len()).collect();
                indices.shuffle(&mut rng);
                let mut dealloc_indices: Vec<usize> =
                    indices.into_iter().take(num_to_remove).collect();
                dealloc_indices.sort_by(|a, b| b.cmp(a));

                for i in dealloc_indices.iter() {
                    let alloc = allocations.remove(*i);
                    allocator.deallocate(alloc.id).unwrap();
                }

                for _ in 0..num_to_remove {
                    let alloc_size = rng.random_range(min_alloc_size..=max_alloc_size);
                    let alloc = allocator.allocate(alloc_size).unwrap();
                    allocations.push(alloc);
                }
            }
            let duration_bf = start_bf.elapsed();
            println!(
                "Best-Fit Benchmark Avg Time: {:?}",
                duration_bf / iterations as u32
            );
            println!("Free List size: {}", allocator.free_list.len());
        }
    }
}
