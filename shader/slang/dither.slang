module dither;

static const float OUTPUT_QUANTIZATION_STEP = 1.0 / 255.0;
static const float2 DITHER_GRID = float2(1.0 / 16.0, 10.0 / 36.0);
// Half a cell centers the 16x18 period exactly around zero.
static const float DITHER_PHASE = 0.25 + 1.0 / 288.0;

// The postprocess target is R8G8B8A8_UNORM. Keep this spatial offset at the
// final linear 8-bit quantization stage; the later swapchain blit converts to
// the sRGB display encoding.
public float getDitherOffset(int2 screenSpaceUv)
{
    float gridPosition = frac(dot(float2(screenSpaceUv), DITHER_GRID) + DITHER_PHASE);
    return (gridPosition - 0.5) * OUTPUT_QUANTIZATION_STEP;
}
