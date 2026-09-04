module aabb;

public void intersectSlabs(float3 minimum, float3 maximum, float3 origin,
                           float3 inverseDirection, out float minimumDistance,
                           out float maximumDistance)
{
    float3 t0 = (minimum - origin) * inverseDirection;
    float3 t1 = (maximum - origin) * inverseDirection;
    float3 nearDistances = min(t0, t1);
    float3 farDistances = max(t0, t1);

    minimumDistance = max(max(nearDistances.x, nearDistances.y), nearDistances.z);
    maximumDistance = min(min(farDistances.x, farDistances.y), farDistances.z);
}

public bool isInsideAabb(int3 point, int3 minimum, int3 maximum)
{
    return all(point >= minimum) && all(point < maximum);
}
