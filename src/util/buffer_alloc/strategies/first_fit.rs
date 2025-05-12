use super::AllocationStrategy;
use crate::util::{BufferAllocation, FreeBlock};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct FirstFitAllocator {
    total_size: u64,
    allocated: HashMap<u64, BufferAllocation>,
    pub free_list: Vec<FreeBlock>,
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
        let mut merged: Vec<FreeBlock> = Vec::with_capacity(self.free_list.len());
        for block in self.free_list.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.offset + last.size == block.offset {
                    last.size += block.size;
                    continue;
                }
            }
            merged.push(block);
        }
        self.free_list = merged;
    }
}

impl AllocationStrategy for FirstFitAllocator {
    fn allocate(&mut self, req_size: u64) -> Result<BufferAllocation, String> {
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
        let mut allocs: Vec<&mut BufferAllocation> = self.allocated.values_mut().collect();
        allocs.sort_by_key(|a| a.offset);
        let mut cur = 0;
        for a in allocs {
            a.offset = cur;
            cur += a.size;
        }
        self.free_list.clear();
        if cur < self.total_size {
            self.free_list.push(FreeBlock {
                offset: cur,
                size: self.total_size - cur,
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

    fn resize(&mut self, id: u64, to_size: u64) -> Result<BufferAllocation, String> {
        // 1) Check exists
        let (old_offset, old_size) = if let Some(a) = self.allocated.get(&id) {
            (a.offset, a.size)
        } else {
            return Err("Allocation id not found".into());
        };

        // 2) No-op
        if to_size == old_size {
            return Ok(self.allocated.get(&id).unwrap().clone());
        }

        // 3) Shrink in place
        if to_size < old_size {
            let delta = old_size - to_size;
            if let Some(a) = self.allocated.get_mut(&id) {
                a.size = to_size;
            }
            self.free_list.push(FreeBlock {
                offset: old_offset + to_size,
                size: delta,
            });
            self.coalesce_free_list();
            return Ok(self.allocated.get(&id).unwrap().clone());
        }

        // 4) Expand in place if possible
        let expand_by = to_size - old_size;
        if let Some(idx) = self
            .free_list
            .iter()
            .position(|b| b.offset == old_offset + old_size && b.size >= expand_by)
        {
            if self.free_list[idx].size == expand_by {
                self.free_list.remove(idx);
            } else {
                self.free_list[idx].offset += expand_by;
                self.free_list[idx].size -= expand_by;
            }
            if let Some(a) = self.allocated.get_mut(&id) {
                a.size = to_size;
            }
            return Ok(self.allocated.get(&id).unwrap().clone());
        }

        // 5) Otherwise we must move
        // find a free block large enough
        if let Some(idx) = self.free_list.iter().position(|b| b.size >= to_size) {
            let new_offset = self.free_list[idx].offset;
            if self.free_list[idx].size == to_size {
                self.free_list.remove(idx);
            } else {
                self.free_list[idx].offset += to_size;
                self.free_list[idx].size -= to_size;
            }
            // free the old
            self.free_list.push(FreeBlock {
                offset: old_offset,
                size: old_size,
            });
            self.coalesce_free_list();
            // update
            if let Some(a) = self.allocated.get_mut(&id) {
                a.offset = new_offset;
                a.size = to_size;
            }
            return Ok(self.allocated.get(&id).unwrap().clone());
        }

        Err("Not enough free memory to resize".into())
    }
}
