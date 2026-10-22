//! Copy policy for host-mapped Vulkan buffers, not ordinary process memory.
use ash::vk;

fn uses_streaming_stores(properties: vk::MemoryPropertyFlags) -> bool {
    cfg!(target_arch = "x86_64")
        && properties.contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
        && !properties.contains(vk::MemoryPropertyFlags::HOST_CACHED)
}

/// Copy exactly `len` bytes without reading the destination.
///
/// # Safety
/// The source and destination must be valid, non-overlapping ranges of `len`
/// bytes. The caller owns host-write access to the mapped destination and its
/// subsequent Vulkan synchronization. Source bytes may include uninitialized
/// padding from a valid `Copy` value; do not manufacture a byte slice or SIMD
/// Rust value from them. A zero-length copy does not access either pointer.
#[inline]
pub(super) unsafe fn copy_to_mapped(
    source: *const u8,
    destination: *mut u8,
    len: usize,
    properties: vk::MemoryPropertyFlags,
) {
    if len == 0 {
        return;
    }
    if uses_streaming_stores(properties) {
        #[cfg(target_arch = "x86_64")]
        {
            // SAFETY: the caller supplies disjoint valid ranges. SSE2 is baseline
            // on x86-64. This routine handles both pointers' arbitrary alignment.
            unsafe { copy_uncached_x86_64(source, destination, len) };
            return;
        }
    }
    // Cached mappings and other architectures retain the standard copy. Raw
    // pointer copy, unlike constructing &[u8], also preserves Copy padding.
    unsafe { std::ptr::copy_nonoverlapping(source, destination, len) };
}

/// Generic libc memcpy may select REP MOVSB, which is extremely slow for some
/// uncached GPU mappings. Pin the write-only SSE2 path rather than changing libc
/// tunables, allocation placement, or individual upload callers.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
unsafe fn copy_uncached_x86_64(source: *const u8, destination: *mut u8, len: usize) {
    // Raw assembly transports object representations (including padding) without
    // exposing possibly-uninitialized bytes as initialized Rust integer/SIMD
    // values. No instruction reads destination memory or touches out-of-range
    // bytes. Align the destination, stream full vectors, then copy the tail.
    //
    // SFENCE completes/orders the non-temporal stores before returning to the
    // caller's submission/publication. Default asm memory effects are essential:
    // do NOT mark this nomem/readonly/pure. This is not a replacement for Vulkan
    // flushes on non-coherent allocations or for frame-slot GPU synchronization.
    unsafe {
        std::arch::asm!(
            "test {len}, {len}",
            "jz 8f",
            "2:",
            "test {dst}, 15",
            "jz 3f",
            "mov al, [{src}]",
            "mov [{dst}], al",
            "inc {src}",
            "inc {dst}",
            "dec {len}",
            "jnz 2b",
            "jmp 8f",
            "3:",
            "cmp {len}, 64",
            "jb 5f",
            "4:",
            "movdqu xmm0, [{src}]",
            "movdqu xmm1, [{src} + 16]",
            "movdqu xmm2, [{src} + 32]",
            "movdqu xmm3, [{src} + 48]",
            "movntdq [{dst}], xmm0",
            "movntdq [{dst} + 16], xmm1",
            "movntdq [{dst} + 32], xmm2",
            "movntdq [{dst} + 48], xmm3",
            "add {src}, 64",
            "add {dst}, 64",
            "sub {len}, 64",
            "cmp {len}, 64",
            "jae 4b",
            "5:",
            "cmp {len}, 16",
            "jb 6f",
            "movdqu xmm0, [{src}]",
            "movntdq [{dst}], xmm0",
            "add {src}, 16",
            "add {dst}, 16",
            "sub {len}, 16",
            "jmp 5b",
            "6:",
            "test {len}, {len}",
            "jz 8f",
            "7:",
            "mov al, [{src}]",
            "mov [{dst}], al",
            "inc {src}",
            "inc {dst}",
            "dec {len}",
            "jnz 7b",
            "8:",
            "sfence",
            src = inout(reg) source => _,
            dst = inout(reg) destination => _,
            len = inout(reg) len => _,
            out("al") _,
            out("xmm0") _, out("xmm1") _, out("xmm2") _, out("xmm3") _,
            options(nostack),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;

    #[test]
    fn policy_uses_actual_cache_flags_not_the_requested_heap() {
        use vk::MemoryPropertyFlags as F;
        for flags in [
            F::HOST_VISIBLE,
            F::HOST_VISIBLE | F::HOST_COHERENT,
            F::HOST_VISIBLE | F::HOST_COHERENT | F::DEVICE_LOCAL,
        ] {
            assert_eq!(uses_streaming_stores(flags), cfg!(target_arch = "x86_64"));
            assert!(!uses_streaming_stores(flags | F::HOST_CACHED));
        }
        assert!(!uses_streaming_stores(F::DEVICE_LOCAL));
        assert!(!uses_streaming_stores(F::empty()));
    }

    #[test]
    fn copies_exact_bytes_for_independent_alignments_and_vector_tails() {
        let source: Vec<u8> = (0..512).map(|i| (i * 37 + 11) as u8).collect();
        for cached in [false, true] {
            let flags = vk::MemoryPropertyFlags::HOST_VISIBLE
                | if cached {
                    vk::MemoryPropertyFlags::HOST_CACHED
                } else {
                    vk::MemoryPropertyFlags::empty()
                };
            for src_offset in 0..32 {
                for dst_offset in 0..64 {
                    for len in [
                        0, 1, 2, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255,
                    ] {
                        let mut dest = [0xa5; 384];
                        let original = dest;
                        unsafe {
                            copy_to_mapped(
                                source.as_ptr().add(src_offset),
                                dest.as_mut_ptr().add(dst_offset),
                                len,
                                flags,
                            );
                        }
                        assert_eq!(
                            &dest[dst_offset..dst_offset + len],
                            &source[src_offset..src_offset + len]
                        );
                        assert_eq!(&dest[..dst_offset], &original[..dst_offset]);
                        assert_eq!(&dest[dst_offset + len..], &original[dst_offset + len..]);
                    }
                }
            }
        }
    }

    #[test]
    fn copy_preserves_initialized_fields_of_padded_copy_types() {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Padded {
            tag: u8,
            value: u64,
        }
        let source = [Padded {
            tag: 7,
            value: 0x123456789abcdef,
        }; 16];
        for flags in [
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED,
        ] {
            let mut dest = MaybeUninit::<[Padded; 16]>::uninit();
            unsafe {
                copy_to_mapped(
                    source.as_ptr().cast(),
                    dest.as_mut_ptr().cast(),
                    std::mem::size_of_val(&source),
                    flags,
                );
                for item in dest.assume_init() {
                    assert_eq!(item.tag, 7);
                    assert_eq!(item.value, 0x123456789abcdef);
                }
            }
        }
    }

    // Guard pages detect over-reads too, which ordinary destination canaries
    // cannot. This is a small CPU-only test, never a GPU/window performance test.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn copies_at_guard_pages_without_reading_or_writing_outside_the_range() {
        use std::ffi::{c_int, c_void};
        unsafe extern "C" {
            fn getpagesize() -> c_int;
            fn mmap(
                address: *mut c_void,
                length: usize,
                protection: c_int,
                flags: c_int,
                fd: c_int,
                offset: i64,
            ) -> *mut c_void;
            fn mprotect(address: *mut c_void, length: usize, protection: c_int) -> c_int;
            fn munmap(address: *mut c_void, length: usize) -> c_int;
        }
        struct Guarded {
            base: *mut c_void,
            page: usize,
        }
        impl Guarded {
            fn new() -> Self {
                unsafe {
                    let page = getpagesize() as usize;
                    // Linux MAP_PRIVATE | MAP_ANONYMOUS; only the middle page is RW.
                    let base = mmap(std::ptr::null_mut(), page * 3, 0, 0x22, -1, 0);
                    assert_ne!(base as isize, -1);
                    let mapped = Self { base, page };
                    assert_eq!(mprotect(mapped.data().cast(), page, 3), 0);
                    mapped
                }
            }
            fn data(&self) -> *mut u8 {
                unsafe { self.base.cast::<u8>().add(self.page) }
            }
        }
        impl Drop for Guarded {
            fn drop(&mut self) {
                unsafe {
                    assert_eq!(munmap(self.base, self.page * 3), 0);
                }
            }
        }
        let source = Guarded::new();
        let destination = Guarded::new();
        let pattern: Vec<u8> = (0..source.page).map(|i| (i * 37 + 11) as u8).collect();
        unsafe {
            std::ptr::copy_nonoverlapping(pattern.as_ptr(), source.data(), source.page);
        }
        for flags in [
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED,
        ] {
            for len in (0..=129).chain([255, 256, 257, source.page - 1, source.page]) {
                for (src, dst) in [
                    (source.page - len, 0),
                    (0, source.page - len),
                    (source.page - len, source.page - len),
                ] {
                    unsafe {
                        std::ptr::write_bytes(destination.data(), 0xa5, destination.page);
                        copy_to_mapped(
                            source.data().add(src),
                            destination.data().add(dst),
                            len,
                            flags,
                        );
                        let result =
                            std::slice::from_raw_parts(destination.data(), destination.page);
                        assert_eq!(&result[dst..dst + len], &pattern[src..src + len]);
                        assert!(result[..dst].iter().all(|&b| b == 0xa5));
                        assert!(result[dst + len..].iter().all(|&b| b == 0xa5));
                    }
                }
            }
        }
    }

    #[test]
    fn zero_length_copy_does_not_dereference_null_pointers() {
        unsafe {
            copy_to_mapped(
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
            );
        }
    }
}
