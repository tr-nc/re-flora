use super::AllocationStrategy;
use crate::util::{BufferAllocation, FreeBlock};
use std::fmt::Debug;
use std::{collections::HashMap, fmt::Formatter};

#[derive(Clone)]
pub struct FirstFitAllocator {
    total_size: u64,
    allocated: HashMap<u64, BufferAllocation>,
    free_list: Vec<FreeBlock>,
    next_id: u64,
}

impl Debug for FirstFitAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FirstFitAllocator {{ total_size: {}, allocated: {}, free_list: {} }}",
            self.total_size,
            self.allocated.len(),
            self.free_list.len()
        )
    }
}

impl FirstFitAllocator {
    /// Creates a new first-fit allocator with the given total size (in bytes).
    pub fn new(total_size: u64) -> Self {
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
    fn allocate(&mut self, req_size: u64) -> Result<BufferAllocation, String> {
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
                let allocation = BufferAllocation {
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

    fn lookup(&self, id: u64) -> Option<BufferAllocation> {
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
        let mut allocations: Vec<&mut BufferAllocation> = self.allocated.values_mut().collect();
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
