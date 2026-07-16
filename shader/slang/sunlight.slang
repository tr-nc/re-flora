module sunlight;

public float sunLuminanceFromDirection(float3 sunDirection, float baseLuminance)
{
    float dayFactor = smoothstep(-0.05, 0.05, sunDirection.y);
    return baseLuminance * dayFactor;
}
