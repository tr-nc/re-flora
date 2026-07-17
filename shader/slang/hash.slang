module hash;

uint murmurHash11(uint source)
{
    const uint multiplier = 0x5bd1e995u;
    uint value = 1190494759u;
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

public float hash11(float source)
{
    uint value = murmurHash11(asuint(source));
    return asfloat((value & 0x007fffffu) | 0x3f800000u) - 1.0;
}

public uint murmurHash12(uint2 source)
{
    const uint multiplier = 0x5bd1e995u;
    uint value = 1190494759u;
    source *= multiplier;
    source ^= source >> 24u;
    source *= multiplier;
    value *= multiplier;
    value ^= source.x;
    value *= multiplier;
    value ^= source.y;
    value ^= value >> 13u;
    value *= multiplier;
    value ^= value >> 15u;
    return value;
}

public float hash12(float2 source)
{
    uint value = murmurHash12(asuint(source));
    return asfloat((value & 0x007fffffu) | 0x3f800000u) - 1.0;
}

uint murmurHash13(uint3 source)
{
    const uint multiplier = 0x5bd1e995u;
    uint value = 1190494759u;
    source *= multiplier;
    source ^= source >> 24u;
    source *= multiplier;
    value *= multiplier;
    value ^= source.x;
    value *= multiplier;
    value ^= source.y;
    value *= multiplier;
    value ^= source.z;
    value ^= value >> 13u;
    value *= multiplier;
    value ^= value >> 15u;
    return value;
}

public float hash13(float3 source)
{
    uint value = murmurHash13(asuint(source));
    return asfloat((value & 0x007fffffu) | 0x3f800000u) - 1.0;
}

public uint wellonsHash(uint value)
{
    value ^= value >> 16u;
    value *= 0x7feb352du;
    value ^= value >> 15u;
    value *= 0x846ca68bu;
    value ^= value >> 16u;
    return value;
}
