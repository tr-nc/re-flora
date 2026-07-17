module dither;

public float3 getDitherMask(int2 screenSpaceUv)
{
    const float ditherBit = 8.0;
    float gridPosition = frac(dot(float2(screenSpaceUv) - float2(0.5, 0.5),
                                  float2(1.0 / 16.0, 10.0 / 36.0) + 0.25));
    float ditherShift = 0.25 * (1.0 / (pow(2.0, ditherBit) - 1.0));
    float3 rgbShift = float3(ditherShift, -ditherShift, ditherShift);
    rgbShift = lerp(2.0 * rgbShift, -2.0 * rgbShift, gridPosition);
    return 0.5 / 255.0 + rgbShift;
}
