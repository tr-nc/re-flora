module composition_camera_effects;

public float3 sampleLensFlare(float2 screenUv,
                              readonly RWTexture2D<float4> lensFlareOutput)
{
    uint width;
    uint height;
    lensFlareOutput.GetDimensions(width, height);
    int2 size = int2(width, height);
    float2 texel = screenUv * float2(size) - float2(0.5);
    int2 base = int2(floor(texel));
    float2 fraction = frac(texel);
    int2 maximum = size - int2(1);
    int2 c00 = clamp(base, int2(0), maximum);
    int2 c10 = clamp(base + int2(1, 0), int2(0), maximum);
    int2 c01 = clamp(base + int2(0, 1), int2(0), maximum);
    int2 c11 = clamp(base + int2(1, 1), int2(0), maximum);
    float3 top = lerp(lensFlareOutput[c00].rgb,
                      lensFlareOutput[c10].rgb, fraction.x);
    float3 bottom = lerp(lensFlareOutput[c01].rgb,
                         lensFlareOutput[c11].rgb, fraction.x);
    return lerp(top, bottom, fraction.y);
}

public float sampleGodRay(float2 screenUv,
                          readonly RWTexture2D<float> godRayOutput)
{
    uint width;
    uint height;
    godRayOutput.GetDimensions(width, height);
    int2 size = int2(width, height);
    float2 texel = screenUv * float2(size) - float2(0.5);
    int2 base = int2(floor(texel));
    float2 fraction = frac(texel);
    int2 maximum = size - int2(1);
    int2 c00 = clamp(base, int2(0), maximum);
    int2 c10 = clamp(base + int2(1, 0), int2(0), maximum);
    int2 c01 = clamp(base + int2(0, 1), int2(0), maximum);
    int2 c11 = clamp(base + int2(1, 1), int2(0), maximum);
    float top = lerp(godRayOutput[c00], godRayOutput[c10], fraction.x);
    float bottom = lerp(godRayOutput[c01], godRayOutput[c11], fraction.x);
    return lerp(top, bottom, fraction.y);
}
