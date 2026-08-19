module ddgi_sampling;

static const float DDGI_GOLDEN_ANGLE = 2.39996322972865332;

public float3 ddgiFibonacciDirection(uint rayIndex, uint rayCount)
{
    float y = 1.0 - (2.0 * float(rayIndex) + 1.0) / float(rayCount);
    float radius = sqrt(max(0.0, 1.0 - y * y));
    float azimuth = DDGI_GOLDEN_ANGLE * float(rayIndex);
    return float3(radius * cos(azimuth), y, radius * sin(azimuth));
}

// Quaternion layout is (x, y, z, w), matching glam::Quat::to_array().
public float3 ddgiRotateDirection(float3 direction, float4 rotation)
{
    float3 twiceCross = 2.0 * cross(rotation.xyz, direction);
    return direction + rotation.w * twiceCross +
           cross(rotation.xyz, twiceCross);
}

public float3 ddgiEpochDirection(uint rayIndex, uint rayCount,
                                 float4 rotation)
{
    return normalize(ddgiRotateDirection(
        ddgiFibonacciDirection(rayIndex, rayCount), rotation));
}
