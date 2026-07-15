module packer;

public uint packNormalOct16(float3 normal)
{
    float invL1Norm = 1.0 / max(abs(normal.x) + abs(normal.y) + abs(normal.z), 1.0e-8);
    float2 projected = normal.xy * invL1Norm;
    float2 signs = select(projected >= float2(0.0), float2(1.0), float2(-1.0));
    float2 folded = (1.0 - abs(projected.yx)) * signs;
    projected = select(normal.z < 0.0, folded, projected);

    uint2 quantized = uint2(round(clamp(projected * 0.5 + 0.5, 0.0, 1.0) * 255.0));
    return quantized.x | (quantized.y << 8u);
}

public float3 unpackNormalOct16(uint packed)
{
    uint2 quantized = uint2(packed & 0xffu, (packed >> 8u) & 0xffu);
    float2 projected = float2(quantized) * (2.0 / 255.0) - 1.0;
    float3 normal = float3(projected, 1.0 - abs(projected.x) - abs(projected.y));
    float fold = max(-normal.z, 0.0);
    normal.xy += select(normal.xy >= float2(0.0), float2(-fold), float2(fold));
    return normalize(normal);
}

public uint packNormalV2(float3 normal)
{
    uint3 quantized = uint3(((normal + 1.0) * 0.5) * 127.0);
    return quantized.r | (quantized.g << 7u) | (quantized.b << 14u);
}

public uint packRgbe(float3 color)
{
    float3 positiveColor = max(float3(0.0), color);
    float maximum = max(positiveColor.r, max(positiveColor.g, positiveColor.b));
    if (maximum == 0.0)
    {
        return 0u;
    }

    float exponent = floor(log2(maximum));
    uint result = uint(clamp(exponent + 20.0, 0.0, 31.0)) << 27u;
    float scale = pow(2.0, -exponent) * 256.0;
    uint3 encoded = min(uint3(511u), uint3(round(positiveColor * scale)));
    result |= encoded.r;
    result |= encoded.g << 9u;
    result |= encoded.b << 18u;
    return result;
}
