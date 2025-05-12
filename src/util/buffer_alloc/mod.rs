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
    use rand::seq::SliceRandom;
    use rand::Rng;
    use std::time::Instant;

    #[test]
    fn test_first_fit_allocator() {
        let total_size = 100000;
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
        let pool_size: u64 = 4 * 1024 * 1024 * 1024; // 4GB pool size
        let initial_allocations: usize = 1000;
        let iterations: u64 = 1_000_000;
        let min_alloc_size: u64 = 2 * 1024 * 1024; // 2MB
        let max_alloc_size: u64 = 5 * 1024 * 1024; // 15MB

        {
            let mut allocator = FirstFitAllocator::new(pool_size);
            let mut allocations: Vec<BufferAllocation> = Vec::with_capacity(initial_allocations);
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
            println!("{:?}", allocator);
        }
    }

    #[test]
    fn test_resize_first_fit() {
        let mut allocator = FirstFitAllocator::new(1000);

        // 1) Shrink in place
        let a1 = allocator.allocate(200).unwrap();
        let resized1 = allocator.resize(a1.id, 150).unwrap();
        assert_eq!(resized1.offset, a1.offset);
        assert_eq!(resized1.size, 150);
        // Ensure freed 50 bytes landed just after
        let next_free = allocator
            .free_list
            .iter()
            .find(|b| b.offset == a1.offset + 150)
            .unwrap();
        assert_eq!(next_free.size, 850);

        // 2) Expand in place: allocate a second in between, free it
        let a2 = allocator.allocate(100).unwrap(); // sits at offset 200
        allocator.deallocate(a2.id).unwrap();
        let resized2 = allocator.resize(resized1.id, 200).unwrap();
        // Since the freed a2 was immediately after, we should expand in place
        assert_eq!(resized2.offset, resized1.offset);
        assert_eq!(resized2.size, 200);

        // 3) Expand with move: fill the next region so in-place fails
        let b1 = allocator.allocate(300).unwrap(); // next to resized2
        let before_offset = allocator.lookup(resized2.id).unwrap().offset;
        let resized3 = allocator.resize(resized2.id, 400).unwrap();
        let after_offset = resized3.offset;
        // We expect it to move to some other free region => offset changes
        assert_ne!(before_offset, after_offset);
        assert_eq!(resized3.size, 400);
        // The old region should now be free again:
        assert!(allocator
            .free_list
            .iter()
            .any(|b| b.offset == before_offset && b.size == 200));
    }
}
