module ddgi_octahedral;

public float signNotZero(float value)
{
    return value >= 0.0 ? 1.0 : -1.0;
}

public float2 octEncode(float3 direction)
{
    direction = normalize(direction);
    float inverseL1 = rcp(abs(direction.x) + abs(direction.y) +
                          abs(direction.z));
    float2 encoded = direction.xz * inverseL1;
    if (direction.y < 0.0)
    {
        float2 unfolded = encoded;
        encoded.x = (1.0 - abs(unfolded.y)) * signNotZero(unfolded.x);
        encoded.y = (1.0 - abs(unfolded.x)) * signNotZero(unfolded.y);
    }
    return encoded;
}

public float3 octDecode(float2 encoded)
{
    float3 direction =
        float3(encoded.x, 1.0 - abs(encoded.x) - abs(encoded.y), encoded.y);
    if (direction.y < 0.0)
    {
        float unfoldedX = direction.x;
        float unfoldedZ = direction.z;
        direction.x = (1.0 - abs(unfoldedZ)) * signNotZero(unfoldedX);
        direction.z = (1.0 - abs(unfoldedX)) * signNotZero(unfoldedZ);
    }
    return normalize(direction);
}

public uint2 ddgiTileOrigin(uint probeIndex, uint tileColumns,
                           uint storedSide)
{
    return uint2(probeIndex % tileColumns, probeIndex / tileColumns) *
           storedSide;
}

public uint2 ddgiGutterSource(uint2 destination, uint interiorSide)
{
    uint last = interiorSide + 1u;
    bool corner = (destination.x == 0u || destination.x == last) &&
                  (destination.y == 0u || destination.y == last);
    if (corner)
    {
        return uint2(destination.x > 0u ? 1u : interiorSide,
                     destination.y > 0u ? 1u : interiorSide);
    }
    if (destination.y == 0u || destination.y == last)
    {
        return uint2(last - destination.x,
                     destination.y > 0u ? interiorSide : 1u);
    }
    return uint2(destination.x > 0u ? interiorSide : 1u,
                 last - destination.y);
}
