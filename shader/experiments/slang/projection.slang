module projection;

public float3 projectScreenUvToWorld(float2 screenUv, float depth,
                                     float4x4 inverseViewProjection)
{
    float4 ndcPosition = float4(screenUv * 2.0 - 1.0, depth, 1.0);
    float4 worldPosition = mul(inverseViewProjection, ndcPosition);
    return worldPosition.xyz / worldPosition.w;
}
