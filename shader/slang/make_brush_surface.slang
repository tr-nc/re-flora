import flora_brush;
import surface_extraction;

[[vk::binding(0, 0)]]
ConstantBuffer<U_EditOccupancyInfo> edit_occupancy_info;
[[vk::binding(1, 0)]]
[[vk::image_format("r32ui")]]
WTexture3D<uint> surface;
[[vk::binding(2, 0)]]
[[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

// Same extractor, 5x5x5 normal, packing and world-space halo as terrain building.
// Only the brush's aligned tiles are needed: preserved roots never read surface.
[shader("compute")]
[numthreads(SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE, SURFACE_GROUP_SIZE)]
void main(uint3 groupId : SV_GroupID, uint3 localId : SV_GroupThreadID,
          uint localIndex : SV_GroupIndex)
{
    int3 base = int3(edit_occupancy_info.surface_offset + groupId * SURFACE_GROUP_SIZE);
    preloadSurfaceData(chunk_atlas, int3(edit_occupancy_info.chunk_world_offset),
        int3(edit_occupancy_info.chunk_dim), true, base, localIndex);
    GroupMemoryBarrierWithGroupSync();
    int3 voxel = base + int3(localId);
    uint properties;
    extractSurfaceVoxel(base, voxel, int3(edit_occupancy_info.chunk_world_offset),
        edit_occupancy_info.chunk_dim, properties);
    if (all(voxel < int3(edit_occupancy_info.chunk_dim)))
        surface.Store(uint3(voxel), properties); // also overwrite air / interior, never stale
}
