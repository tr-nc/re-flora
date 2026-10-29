module ui_matte_pixel;

// Panel-local cells crossing either viewport edge sample only their real pixels.
// gridOrigin is in (-cellSize, 0]. Panel/clip bounds must not reset this domain.
public uint2 uiMatteCellRange(uint cell, uint cellSize, uint extent, int gridOrigin)
{
    int begin = gridOrigin + int(cell * cellSize);
    return uint2(clamp(begin, 0, int(extent)), clamp(begin + int(cellSize), 0, int(extent)));
}

public interface IMatteCellSource
{
    float3 sample(uint2 pixel);
}

// Five nearest-texel taps: center and the centers of four quadrants, each 1/5.
// Quantiles are measured over the viewport-clipped cell, not the panel crop.
// Tiny cells may repeat texels; no out-of-domain padding or random phase.
public float3 uiMatteCellFiveTap<T : IMatteCellSource>(T source, uint2 lo, uint2 hi)
{
    uint2 size = hi - lo;
    uint2 center = lo + size / 2u;
    uint2 low = lo + size / 4u;
    uint2 high = lo + (size * 3u) / 4u;
    return (source.sample(center)
        + source.sample(low)
        + source.sample(uint2(high.x, low.y))
        + source.sample(uint2(low.x, high.y))
        + source.sample(high)) * 0.2;
}
