#[derive(Debug, Clone)]
pub struct BufferAllocation {
    pub id: u64,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct FreeBlock {
    pub offset: u64,
    pub size: u64,
}

mod strategies;
pub use strategies::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_fit_allocator() {
        let total_size = 100000;
        let mut allocator = FirstFitAllocator::new(total_size);
        // allocate 200 bytes.
        let alloc1 = allocator.allocate(200).unwrap();
        assert_eq!(alloc1.size, 200);
        assert_eq!(alloc1.offset, 0);

        // allocate 300 bytes.
        let alloc2 = allocator.allocate(300).unwrap();
        assert_eq!(alloc2.offset, 200);

        // allocate additional 100 bytes.
        let alloc3 = allocator.allocate(100).unwrap();
        assert_eq!(alloc3.offset, 500);

        // lookup allocation alloc2.
        let lookup2 = allocator.lookup(alloc2.id).unwrap();
        assert_eq!(lookup2.size, 300);
        assert_eq!(lookup2.offset, alloc2.offset);

        // deallocate allocation alloc2.
        allocator.deallocate(alloc2.id).unwrap();

        // allocate 250 bytes; reused the freed area.
        let alloc4 = allocator.allocate(250).unwrap();
        assert_eq!(alloc4.offset, 200);

        // reset the allocator.
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

        // after cleanup allocations should be repacked contiguously.
        assert_eq!(lookup1.offset, 0);
        assert_eq!(lookup4.offset, 100);
        assert_eq!(lookup3.offset, 250);
    }

    #[test]
    fn allocation_churn_keeps_allocator_usable() {
        let mut allocator = FirstFitAllocator::new(10_000);
        let mut allocations = Vec::new();

        for size in [64, 128, 256, 512, 1024, 2048] {
            allocations.push(allocator.allocate(size).unwrap());
        }

        for _ in 0..32 {
            let removed = allocations.remove(1);
            allocator.deallocate(removed.id).unwrap();
            allocations.push(allocator.allocate(96).unwrap());

            let removed = allocations.remove(2);
            allocator.deallocate(removed.id).unwrap();
            allocations.push(allocator.allocate(160).unwrap());
        }

        for allocation in &allocations {
            assert_eq!(allocator.lookup(allocation.id).unwrap().id, allocation.id);
        }

        allocator.cleanup();
        let mut next_offset = 0;
        let mut packed = allocations
            .iter()
            .filter_map(|allocation| allocator.lookup(allocation.id))
            .collect::<Vec<_>>();
        packed.sort_by_key(|allocation| allocation.offset);
        for allocation in packed {
            assert_eq!(allocation.offset, next_offset);
            next_offset += allocation.size;
        }
    }
}
