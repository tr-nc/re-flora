module flora_noise;

import color;
import flora_types;
import gradient_noise;
import hash;

public float constructFloat01(uint value)
{
    value = (value & 0x007fffffu) | 0x3f800000u;
    return asfloat(value) - 1.0;
}

uint3 murmurHash31(uint source)
{
    const uint multiplier = 0x5bd1e995u;
    uint3 value = uint3(1190494759u, 2147483647u, 3559788179u);
    source *= multiplier;
    source ^= source >> 24u;
    source *= multiplier;
    value *= multiplier;
    value ^= source;
    value ^= value >> 13u;
    value *= multiplier;
    value ^= value >> 15u;
    return value;
}

uint3 murmurHash34(uint4 source)
{
    const uint multiplier = 0x5bd1e995u;
    uint3 value = uint3(1190494759u, 2147483647u, 3559788179u);
    source *= multiplier;
    source ^= source >> 24u;
    source *= multiplier;
    value *= multiplier;
    value ^= source.x;
    value *= multiplier;
    value ^= source.y;
    value *= multiplier;
    value ^= source.z;
    value *= multiplier;
    value ^= source.w;
    value ^= value >> 13u;
    value *= multiplier;
    value ^= value >> 15u;
    return value;
}

public float3 signedUnitNoise(float seed)
{
    uint3 value = murmurHash31(asuint(seed));
    return asfloat((value & 0x007fffffu) | 0x3f800000u) * 2.0 - 3.0;
}

public float3 signedUnitNoise(float4 seed)
{
    uint3 value = murmurHash34(asuint(seed));
    return asfloat((value & 0x007fffffu) | 0x3f800000u) * 2.0 - 3.0;
}

public float3 applyHsvOffset(float3 linearRgb, float3 hsvOffset)
{
    float3 hsv = rgbToHsv(linearRgb);
    hsv.x = frac(hsv.x + hsvOffset.x);
    hsv.y = clamp(hsv.y + hsvOffset.y, 0.0, 1.0);
    hsv.z = clamp(hsv.z + hsvOffset.z, 0.0, 1.0);
    return hsvToRgb(hsv);
}

public float sampleGrassBandInterpolation(float2 worldXz)
{
    float sum = 0.0;
    float amplitude = 1.0;
    float amplitudeSum = 0.0;
    float frequency = 0.008;
    for (int octave = 0; octave < 3; ++octave)
    {
        sum += seededClassicGradientNoise2D(
                   worldXz * frequency, 9041u + uint(octave) * 1000u) *
               amplitude;
        amplitudeSum += amplitude;
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    float noise = sum / amplitudeSum;
    return clamp(noise * 0.5 + 0.5, 0.0, 1.0);
}
