module projection;

public float3 projectScreenUvToWorld(float2 screenUv, float depth,
                                     float4x4 inverseViewProjection)
{
    float4 ndcPosition = float4(screenUv * 2.0 - 1.0, depth, 1.0);
    float4 worldPosition = mul(inverseViewProjection, ndcPosition);
    return worldPosition.xyz / worldPosition.w;
}

public float2 projectWorldToScreen(float3 worldPosition, float4x4 viewProjection)
{
    float4 clipPosition = mul(viewProjection, float4(worldPosition, 1.0));
    float2 ndc = clipPosition.xy / clipPosition.w;
    return ndc * 0.5 + 0.5;
}
