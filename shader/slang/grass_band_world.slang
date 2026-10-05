module grass_band_world;
import grass_band_cache;
import stem_band_geometry;

[[vk::binding(9, 1)]] public StructuredBuffer<GrassBandPose> grass_band_pose_cache;

// One owner for the actual triangle positions used by plain and pixelized grass.
public float3 grassBandWorldPoint(uint index, uint band, uint height,
    uint voxelCount, uint vertex)
{
    float3 center = grass_band_pose_cache[index].position_height.xyz;
    float3 previous = center, next = center;
    bool hasNext = false;
    if (band > 0u) previous = grass_band_pose_cache[index - 1u].position_height.xyz;
    if (band + 1u < height && band + 1u < voxelCount)
    {
        GrassBandPose nextPose = grass_band_pose_cache[index + 1u];
        next = nextPose.position_height.xyz;
        hasNext = nextPose.color_valid.w != 0.0;
    }
    float3 a, b, tangentA, tangentB;
    stemBandCenterFrames(center, previous, next, band > 0u, hasNext,
        0.5 / 256.0, a, b, tangentA, tangentB);
    const float ra = 0.5 / 256.0, rb = ra;
    return stemBandPoint(a, b, tangentA, tangentB,
        float3(1, 0, 0), float3(1, 0, 0), float2(ra), float2(rb), stemBandCorner(vertex));
}
