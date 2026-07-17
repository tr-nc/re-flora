module noise_tex;

public static const int3 BLUE_NOISE_SIZE = int3(128, 128, 64);

public int3 getBlueNoiseSeed(uint2 dispatchThreadId, uint frameSerialIndex)
{
    return int3(dispatchThreadId % 128u, frameSerialIndex % 64u);
}

public float randomFloat(Sampler2DArray noiseTexture, int3 seed)
{
    return noiseTexture.Load(int4(seed, 0)).r;
}

public float2 randomUnitVector2(Sampler2DArray noiseTexture, int3 seed)
{
    return noiseTexture.Load(int4(seed, 0)).rg * 2.0 - 1.0;
}

public float3 randomUnitVector3(Sampler2DArray noiseTexture, int3 seed)
{
    return noiseTexture.Load(int4(seed, 0)).rgb * 2.0 - 1.0;
}

public float3 randomWeightedCosine(Sampler2DArray noiseTexture, int3 seed)
{
    return noiseTexture.Load(int4(seed, 0)).rgb * 2.0 - 1.0;
}
