module dither;

static const float OUTPUT_QUANTIZATION_STEP = 1.0 / 255.0;
static const uint DITHER_HASH_SEED = 0x12345678u;

// The postprocess target is R8G8B8A8_UNORM. Keep this spatial offset at the
// final linear 8-bit quantization stage; the later swapchain blit converts to
// the sRGB display encoding. This deterministic integer hash avoids a short
// repeating tile without introducing a temporal crawl or shimmer.
public float getDitherOffset(int2 screenSpaceUv, float strengthLsb)
{
    uint2 pixel = uint2(screenSpaceUv);
    uint hash = pixel.x * 0x9E3779B9u + pixel.y * 0x85EBCA6Bu + DITHER_HASH_SEED;
    hash ^= hash >> 16;
    hash *= 0x7FEB352Du;
    hash ^= hash >> 15;
    float noise = float(hash & 0xFFFFu) / 65535.0;
    return (noise - 0.5) * (2.0 * strengthLsb * OUTPUT_QUANTIZATION_STEP);
}
