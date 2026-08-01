import ddgi_types;
import voxel_data;
import voxel_types;

[[vk::binding(16, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[[vk::binding(35, 0)]] [[vk::image_format("r32ui")]]
RWTexture3D<uint> ddgi_voxel_visibility_bits;

[[vk::binding(36, 0)]]
ConstantBuffer<U_DdgiVoxelVisibilityInfo> ddgi_voxel_visibility_info;

[shader("compute")]
[numthreads(4, 4, 4)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    U_DdgiVoxelVisibilityInfo info = ddgi_voxel_visibility_info;
    if (any(dispatchThreadId >= info.packed_word_dimensions)) return;

    uint firstX = dispatchThreadId.x << 5u;
    uint packed = 0u;
    [unroll]
    for (uint bit = 0u; bit < 32u; ++bit)
    {
        uint x = firstX + bit;
        if (x < info.voxel_dimensions.x &&
            voxelTypeFromAtlasData(chunk_atlas[
                uint3(x, dispatchThreadId.y, dispatchThreadId.z)]) != VOXEL_TYPE_EMPTY)
        {
            packed |= 1u << bit;
        }
    }
    ddgi_voxel_visibility_bits[dispatchThreadId] = packed;
}
