import ddgi_types;

[[vk::binding(35, 0)]] [[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> ddgi_voxel_visibility_bits;

[[vk::binding(36, 0)]]
ConstantBuffer<U_DdgiVoxelVisibilityInfo> ddgi_voxel_visibility_info;

[[vk::binding(38, 0)]] [[vk::image_format("r8ui")]]
RWTexture3D<uint> ddgi_voxel_visibility_blocks;

[shader("compute")]
[numthreads(4, 4, 4)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    U_DdgiVoxelVisibilityInfo info = ddgi_voxel_visibility_info;
    uint3 updateDimensions = info.update_block_dimensions.xyz;
    if (any(dispatchThreadId >= updateDimensions)) return;
    uint3 blockDimensions =
        (info.voxel_dimensions + uint3(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE - 1u)) /
        DDGI_VOXEL_VISIBILITY_BLOCK_SIZE;
    uint3 blockCoordinate = info.update_block_offset.xyz + dispatchThreadId;
    if (any(blockCoordinate >= blockDimensions)) return;

    uint3 firstVoxel = blockCoordinate * DDGI_VOXEL_VISIBILITY_BLOCK_SIZE;
    uint occupied = 0u;
    [unroll]
    for (uint z = 0u; z < DDGI_VOXEL_VISIBILITY_BLOCK_SIZE; ++z)
    {
        uint voxelZ = firstVoxel.z + z;
        if (voxelZ >= info.voxel_dimensions.z) break;
        [unroll]
        for (uint y = 0u; y < DDGI_VOXEL_VISIBILITY_BLOCK_SIZE; ++y)
        {
            uint voxelY = firstVoxel.y + y;
            if (voxelY >= info.voxel_dimensions.y) break;
            uint word = ddgi_voxel_visibility_bits[
                uint3(firstVoxel.x >> 5u, voxelY, voxelZ)];
            uint validX = min(
                DDGI_VOXEL_VISIBILITY_BLOCK_SIZE,
                info.voxel_dimensions.x - firstVoxel.x);
            uint mask = ((1u << validX) - 1u) << (firstVoxel.x & 31u);
            occupied |= word & mask;
        }
    }
    ddgi_voxel_visibility_blocks[blockCoordinate] = occupied != 0u ? 1u : 0u;
}
