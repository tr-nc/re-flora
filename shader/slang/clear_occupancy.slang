static const uint OCCUPANCY_GROUP_SIZE = 8u;

struct U_ClearOccupancyInfo
{
    uint3 chunk_dim;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_ClearOccupancyInfo> clear_occupancy_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r32ui")]]
RWTexture3D<uint> occupancy_data;

[shader("compute")]
[numthreads(OCCUPANCY_GROUP_SIZE, OCCUPANCY_GROUP_SIZE, OCCUPANCY_GROUP_SIZE)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    if (any(dispatchThreadId >= clear_occupancy_info.chunk_dim))
    {
        return;
    }

    occupancy_data[dispatchThreadId] = 0u;
}
