module dither;

static const float OUTPUT_QUANTIZATION_STEP = 1.0 / 255.0;
static const float2 DITHER_HASH_SCALE = float2(0.06711056, 0.00583715);
static const float DITHER_HASH_MULTIPLIER = 52.9829189;
static const float DITHER_HASH_PHASE = 0.125;

// The postprocess target is R8G8B8A8_UNORM. Keep this spatial offset at the
// final linear 8-bit quantization stage; the later swapchain blit converts to
// the sRGB display encoding. This deterministic interleaved hash avoids a
// short repeating tile without introducing a temporal crawl or shimmer.
public float getDitherOffset(int2 screenSpaceUv)
{
    float hashInput = frac(dot(float2(screenSpaceUv), DITHER_HASH_SCALE) + DITHER_HASH_PHASE);
    float noise = frac(DITHER_HASH_MULTIPLIER * hashInput);
    return (noise - 0.5) * OUTPUT_QUANTIZATION_STEP;
}
