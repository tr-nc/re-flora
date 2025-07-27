#ifndef BITS_GLSL
#define BITS_GLSL

#extension GL_ARB_gpu_shader_int64 : enable

// GLSL popcount for a 64-bit integer
uint bit_count_u64(uint64_t x) {
    // split into high and low 32-bit halves
    return bitCount(uint(x >> 32)) + bitCount(uint(x));
}

// count number of set bits in the lower [0..width) bits of a 64-bit mask
uint bit_count_u64_var(uint64_t mask, uint width) {
    // extract the low 32 bits
    uint himask = uint(mask);
    uint count  = 0u;

    // if width ≥ 32, count all bits in the low half, then prepare the high half
    if (width >= 32u) {
        count  = bitCount(himask);
        himask = uint(mask >> 32);
    }

    // now mask off only the lower (width mod 32) bits of himask
    // width & 31 gives the remainder in [0..31],
    // so (1 << (width & 31)) − 1 is a mask of that many low bits
    uint m = 1u << (width & 31u);
    count += bitCount(himask & (m - 1u));

    return count;
}

#endif // BITS_GLSL
