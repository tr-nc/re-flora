module color;

public float3 srgbToLinear(float3 color)
{
    bool3 useLinearSegment = color < float3(0.04045);
    float3 higher = pow((color + 0.055) / 1.055, float3(2.4));
    float3 lower = color / 12.92;
    return select(useLinearSegment, lower, higher);
}
