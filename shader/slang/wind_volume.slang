import flora_types;
import wind_volume_sample;

static const uint WIND_VOLUME_GROUP_SIZE = 4u;
static const uint WIND_VOLUME_BUCKET_COUNT = 4u;

struct WindVolumePushConstants
{
    float time;
    uint bucket_index;
};

[[vk::push_constant]]
ConstantBuffer<WindVolumePushConstants> pc;

[[vk::binding(0, 0)]] ConstantBuffer<U_WindVolumeInfo> wind_volume_info;

[[vk::binding(1, 0)]]
[[vk::image_format("rg16f")]]
WTexture3D<float4> wind_volume_tex;

[shader("compute")]
[numthreads(WIND_VOLUME_GROUP_SIZE, WIND_VOLUME_GROUP_SIZE,
            WIND_VOLUME_GROUP_SIZE)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint width;
    uint height;
    uint depth;
    wind_volume_tex.GetDimensions(width, height, depth);
    int3 coordinate = int3(dispatchThreadId);
    int3 bucketSize = int3(
        int(width / WIND_VOLUME_BUCKET_COUNT), int(height), int(depth));
    if (any(coordinate >= bucketSize))
    {
        return;
    }

    int3 targetCoordinate = coordinate;
    targetCoordinate.x += int(pc.bucket_index) * bucketSize.x;

    float3 uvw = (float3(coordinate) + float3(0.5)) /
                 float3(bucketSize);
    float3 worldPosition = uvw * wind_volume_info.world_chunk_extent;
    float3 wind = sampleProceduralWind(worldPosition, pc.time);
    wind_volume_tex.Store(
        targetCoordinate, float4(wind.xz, 0.0, 0.0));
}
