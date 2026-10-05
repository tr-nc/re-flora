// Resolve sampling density independently of the coarse displayed pixel grid.
public uint2 scenePixelExtent(uint2 renderExtent, uint samplesPerAxis)
{
    return renderExtent / samplesPerAxis;
}

public int2 scenePixelCoordinate(int2 outputPixel, uint2 outputExtent, uint2 pixelExtent)
{
    float2 ratio = float2(pixelExtent) / float2(outputExtent);
    return min(int2((float2(outputPixel) + 0.5) * ratio), int2(pixelExtent) - 1);
}

// Average linear HDR samples before tone mapping. Never filter adjacent coarse
// cells: each displayed block represents exactly its own 2x2 sampling footprint.
public float3 scenePixelBoxAverage(float3 a, float3 b, float3 c, float3 d)
{
    return (a + b + c + d) * 0.25;
}
