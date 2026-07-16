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

static const uint LAVENDER_PALETTE_LENGTH = 6u;
static const uint LAVENDER_HEIGHT_PALETTE_RGB10[72] =
{
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x00C5AA9Fu, 0x01075764u, 0x012857DAu, 0x01492BFFu, 0x0117EBA9u,
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x0A31CE9Fu, 0x0D425764u, 0x0F02A7DAu, 0x1082EBFFu, 0x0E4283A9u,
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x2326CA9Fu, 0x2D78CB64u, 0x33A9FFDAu, 0x38DAFFFFu, 0x31197FA9u,
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x02D0260Bu, 0x03A02EA5u, 0x04203702u, 0x04903B4Fu, 0x03F032DBu,
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x2A57148Fu, 0x36C928B8u, 0x3E3A68D2u, 0x3FFB70E7u, 0x3B29E0C7u,
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x28E1F8B4u, 0x34E28CE8u, 0x3C12E508u, 0x3FF32D22u, 0x3912BCFBu,
};

public uint sampleLavenderHeightPaletteRgb10(uint seed, uint heightRow)
{
    uint paletteIndex = uint(constructFloat01(seed) * float(LAVENDER_PALETTE_LENGTH));
    uint row = min(heightRow, uint(FLORA_HEIGHT_COLOR_TABLE_LEN - 1));
    return LAVENDER_HEIGHT_PALETTE_RGB10[
        paletteIndex * uint(FLORA_HEIGHT_COLOR_TABLE_LEN) + row];
}
