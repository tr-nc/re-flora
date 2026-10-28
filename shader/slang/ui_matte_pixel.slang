module ui_matte_pixel;
import ui_matte_blur_kernel;

// The final viewport cell is cropped: count only its real pixels, never pad it
// with black or repeated edge pixels. Caller dispatches ceil(extent / cellSize).
public uint2 uiMatteCellRange(uint cell, uint cellSize, uint extent)
{
    uint begin = cell * cellSize;
    return uint2(begin, min(begin + cellSize, extent));
}

public float3 uiMatteCellMean<T : IMatteBlurLine>(T line, uint count)
{
    float3 sum = float3(0.0);
    for (uint i = 0u; i < count; ++i) sum += line.sample(int(i));
    return sum / float(max(count, 1u));
}
