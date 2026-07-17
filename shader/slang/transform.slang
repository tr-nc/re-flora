module transform;

public float3x3 makeTbn(float3 normal)
{
    float3 up;
    float3 tangent;
    if (abs(normal.y) < 0.99999)
    {
        up = float3(0.0, 1.0, 0.0);
        tangent = normalize(cross(up, normal));
    }
    else
    {
        up = float3(1.0, 0.0, 0.0);
        tangent = normalize(cross(normal, up));
    }
    float3 bitangent = cross(normal, tangent);
    return float3x3(tangent, bitangent, normal);
}
