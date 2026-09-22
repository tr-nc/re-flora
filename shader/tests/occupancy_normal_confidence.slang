import occupancy_normal;
import voxel_data;
import packer;

bool occupied(uint shape, int3 p)
{
    if (shape == 0u) return all(p == int3(0));
    if (shape == 1u) return p.x == 0 && p.z == 0;
    if (shape == 2u) return p.x == 0 && p.z == 0 && p.y <= 0;
    if (shape == 3u) return p.x == p.y && p.y == p.z;
    if (shape == 4u) return p.x == p.y && p.y == p.z && p.y <= 0;
    if (shape == 5u) return p.z == 0;
    if (shape == 6u) return p.z == 0 && p.x <= 0;
    if (shape == 7u) return p.y <= 0;
    if (shape == 8u) return p.x + p.y + p.z <= 0;
    if (shape == 9u) return p.x <= 0 && abs(p.z) <= 1;
    return ((uint(p.x+2) * 73856093u ^ uint(p.y+2) * 19349663u ^ uint(p.z+2) * 83492791u) >> (shape-10u)) % 3u == 0u;
}

export __extern_cpp int main()
{
    bool passed = true;
    for (uint shape = 0u; shape < 30u; ++shape)
    {
        OccupancyNormalMoments moments = beginOccupancyNormal();
        float3 first = float3(0);
        for (int z = -2; z <= 2; ++z)
            for (int y = -2; y <= 2; ++y)
            {
                uint row = 0u;
                for (int x = -2; x <= 2; ++x)
                {
                    int3 p = int3(x,y,z);
                    if (!occupied(shape, p)) continue;
                    row |= 1u << uint(x+2);
                    first += float3(p);
                }
                accumulateOccupancyRow(row, y, z, moments);
            }
        float4 normal = finishOccupancyNormal(moments);
        float3 original = dot(first, first) > 0.0 ? -first * rsqrt(dot(first, first)) : float3(0,1,0);
        passed = passed && packNormalOct16(normal.xyz) == packNormalOct16(original)
            && all(isfinite(normal)) && normal.w >= 0.0 && normal.w <= 1.0;
        if (shape <= 6u) passed = passed && normal.w < 0.00001;
        if (shape == 7u) passed = passed && normal.w == 1.0;
        if (shape == 8u) passed = passed && normal.w > 0.95;
        if (shape == 9u) passed = passed && normal.w > 0.0 && normal.w < 1.0;
        printf("shape=%u confidence=%f pass=%d\n", shape, normal.w, passed);
    }
    // Metadata never changes material/normal bits, and legacy records decode as
    // fully reliable. All confidence and face combinations survive publication.
    for (uint type = 1u; type <= 15u; ++type)
        for (uint faces = 1u; faces <= 63u; ++faces)
            for (uint c = 0u; c <= 63u; ++c)
            {
                float confidence = float(c) / 63.0;
                uint normal = (faces*4093u + c*65521u) & 65535u;
                uint data = packVoxelSurfaceLightingData(type, normal, confidence, faces);
                passed = passed && voxelTypeFromData(data) == type
                    && voxelNormalBitsFromData(data) == normal
                    && voxelExposedFacesFromData(data) == faces
                    && abs(voxelNormalConfidenceFromData(data) - confidence) < 0.000001;
            }
    uint legacy = packVoxelSurfaceData(5u, 32767u);
    passed = passed && voxelNormalConfidenceFromData(legacy) == 1.0 && voxelExposedFacesFromData(legacy) == 0u;
    printf("occupancy and packed surface metadata: %d\n", passed);
    return passed ? 0 : 1;
}
