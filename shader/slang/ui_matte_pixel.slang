module ui_matte_pixel;
import ui_matte_blur_kernel;

// Panel-local cells crossing either viewport edge count only their real pixels,
// never black padding or repeated edges. gridOrigin is in (-cellSize, 0].
public uint2 uiMatteCellRange(uint cell, uint cellSize, uint extent, int gridOrigin)
{
    int begin = gridOrigin + int(cell * cellSize);
    return uint2(clamp(begin, 0, int(extent)), clamp(begin + int(cellSize), 0, int(extent)));
}

public float3 uiMatteCellMean<T : IMatteBlurLine>(T line, uint count)
{
    float3 sum = float3(0.0);
    for (uint i = 0u; i < count; ++i) sum += line.sample(int(i));
    return sum / float(max(count, 1u));
}
