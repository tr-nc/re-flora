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

/// A buffer pool that uses a freelist to manage continuous memory allocations.
/// The actual memory is handled by external code.
pub struct BufferPool {
    /// The total size of the pool in bytes.
    total_size: usize,
    /// A map from allocation IDs to their corresponding allocation metadata.
    allocated: HashMap<u64, Allocation>,
    /// A list of free blocks within the pool.
    free_list: Vec<FreeBlock>,
    /// A counter used to generate unique allocation IDs.
    next_id: u64,
}

impl BufferPool {
    /// Creates a new buffer pool with the given total size (in bytes).
    /// Note that this function does not allocate the actual memory.
    pub fn new(total_size: usize) -> Self {
        // Initially, the entire pool is free.
        let free_list = vec![FreeBlock {
            offset: 0,
            size: total_size,
        }];
        BufferPool {
            total_size,
            allocated: HashMap::new(),
            free_list,
            next_id: 1,
        }
    }

    /// Allocates a continuous block of memory of `req_size` bytes.
    ///
    /// Returns a tuple (allocation ID, allocated size, offset) upon success.
    /// The allocation ID is unique and can be used as a key in a HashMap.
    pub fn allocate(&mut self, req_size: usize) -> Result<(u64, usize, usize), String> {
        // A simple first-fit algorithm: iterate through the free list and look for a block
        // that is large enough to satisfy the request.
        for i in 0..self.free_list.len() {
            if self.free_list[i].size >= req_size {
                let alloc_offset = self.free_list[i].offset;
                if self.free_list[i].size == req_size {
                    // The free block is exactly consumed by the request.
                    self.free_list.remove(i);
                } else {
                    // Split the free block: allocate from the beginning.
                    self.free_list[i].offset += req_size;
                    self.free_list[i].size -= req_size;
                }
                // Generate a unique allocation ID.
                let id = self.next_id;
                self.next_id += 1;
                let allocation = Allocation {
                    id,
                    offset: alloc_offset,
                    size: req_size,
                };
                self.allocated.insert(id, allocation);
                return Ok((id, req_size, alloc_offset));
            }
        }
        Err("Not enough free memory".to_string())
    }

    /// Looks up the allocation given its ID.
    ///
    /// Returns an Option containing a tuple (size, offset) if found.
    pub fn lookup(&self, id: u64) -> Option<(usize, usize)> {
        self.allocated
            .get(&id)
            .map(|alloc| (alloc.size, alloc.offset))
    }

    /// Deallocates the allocation associated with the given ID.
    /// The freed region is added back to the free list, with adjacent free regions coalesced.
    pub fn deallocate(&mut self, id: u64) -> Result<(), String> {
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

    /// Coalesces adjacent free blocks in the free list to reduce fragmentation.
    fn coalesce_free_list(&mut self) {
        self.free_list.sort_by_key(|block| block.offset);
        let mut merged: Vec<FreeBlock> = Vec::new();
        for block in self.free_list.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.offset + last.size == block.offset {
                    // Merge adjacent blocks.
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

    /// Compacts (cleans up) the pool so that all allocated blocks are packed continuously.
    /// The UUIDs remain the same, but their offsets are updated.
    pub fn cleanup(&mut self) {
        // Gather mutable references to all allocated blocks.
        let mut allocations: Vec<&mut Allocation> = self.allocated.values_mut().collect();
        // Sort the allocations based on their current offset.
        allocations.sort_by_key(|alloc| alloc.offset);
        let mut current_offset = 0;
        // Update each allocation so that they become contiguous.
        for alloc in allocations.iter_mut() {
            alloc.offset = current_offset;
            current_offset += alloc.size;
        }
        // Reset the free list: the remaining memory becomes one free block.
        self.free_list.clear();
        if current_offset < self.total_size {
            self.free_list.push(FreeBlock {
                offset: current_offset,
                size: self.total_size - current_offset,
            });
        }
    }

    /// Resets the entire buffer pool, freeing all allocations.
    pub fn reset(&mut self) {
        self.allocated.clear();
        self.free_list.clear();
        self.free_list.push(FreeBlock {
            offset: 0,
            size: self.total_size,
        });
        self.next_id = 1;
    }

    // Future extension point:
    // Other memory allocation strategies (e.g., best-fit, buddy system) can be implemented here
    // by abstracting the allocation/deallocation logic behind a trait interface.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_deallocate() {
        let total_size = 1000;
        let mut pool = BufferPool::new(total_size);

        // Allocate 200 bytes.
        let (id1, size1, offset1) = pool.allocate(200).unwrap();
        assert_eq!(size1, 200);
        assert_eq!(offset1, 0);

        // Allocate 300 bytes.
        let (id2, _size2, offset2) = pool.allocate(300).unwrap();
        assert_eq!(offset2, 200);

        // Allocate 100 bytes.
        let (id3, _size3, offset3) = pool.allocate(100).unwrap();
        assert_eq!(offset3, 500);

        // Lookup allocation id2.
        let (lookup_size, lookup_offset) = pool.lookup(id2).unwrap();
        assert_eq!(lookup_size, 300);
        assert_eq!(lookup_offset, offset2);

        // Deallocate allocation id2.
        pool.deallocate(id2).unwrap();

        // Allocate 250 bytes (this should fit into the previously freed 300-byte block).
        let (id4, _size4, offset4) = pool.allocate(250).unwrap();
        assert_eq!(offset4, 200);

        // Reset the pool.
        pool.reset();
        assert!(pool.allocated.is_empty());
        assert_eq!(pool.free_list.len(), 1);
        assert_eq!(pool.free_list[0].offset, 0);
        assert_eq!(pool.free_list[0].size, total_size);
    }

    #[test]
    fn test_cleanup() {
        let total_size = 1000;
        let mut pool = BufferPool::new(total_size);

        // Allocate three blocks.
        let (id1, _s1, _o1) = pool.allocate(100).unwrap(); // allocation at offset 0..100
        let (id2, _s2, _o2) = pool.allocate(200).unwrap(); // allocation at offset 100..300
        let (id3, _s3, _o3) = pool.allocate(150).unwrap(); // allocation at offset 300..450

        // Deallocate the second block to create fragmentation.
        pool.deallocate(id2).unwrap();

        // Allocate a new block that fits into the freed space.
        let (id4, _s4, off4) = pool.allocate(150).unwrap(); // Should allocate at offset 100.
        assert_eq!(off4, 100);

        // At this point:
        //   id1: offset 0,  size 100
        //   id3: offset 300, size 150
        //   id4: offset 100, size 150
        //
        // After cleanup, the allocated blocks will be repacked contiguously:
        //   id1: offset 0,   size 100
        //   id4: offset 100, size 150
        //   id3: offset 250, size 150

        pool.cleanup();

        let alloc1 = pool.lookup(id1).unwrap();
        let alloc4 = pool.lookup(id4).unwrap();
        let alloc3 = pool.lookup(id3).unwrap();

        // Compare the new offsets.
        assert_eq!(alloc1.1, 0);
        assert_eq!(alloc4.1, 100);
        assert_eq!(alloc3.1, 250);

        // The remaining free block should start at offset 400 (total allocated = 400, remainder = 600).
        assert_eq!(pool.free_list.len(), 1);
        let free = &pool.free_list[0];
        assert_eq!(free.offset, 400);
        assert_eq!(free.size, 600);
    }
}
