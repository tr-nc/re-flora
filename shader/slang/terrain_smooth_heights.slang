import terrain_smooth;
import voxel_data;

[[vk::binding(0, 0)]]
ConstantBuffer<U_TerrainSmoothInfo> terrain_smooth_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_TerrainSmoothColumns, Std430DataLayout>
    terrain_smooth_columns;

[[vk::binding(2, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint2 column = dispatchThreadId.xy;
    if (column.x >= terrain_smooth_info.dim.x ||
        column.y >= terrain_smooth_info.dim.z)
    {
        return;
    }

    uint index = terrainSmoothColumnIndex(terrain_smooth_info, column);
    uint atlasX = terrain_smooth_info.offset.x + column.x;
    uint atlasZ = terrain_smooth_info.offset.z + column.y;
    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    float topHeight = -1.0;
    float valid = 0.0;

    for (int y = int(atlasHeight) - 1; y >= 0; --y)
    {
        uint voxelType = voxelTypeFromAtlasData(
            chunk_atlas[int3(int(atlasX), y, int(atlasZ))]);
        if (terrainSmoothIsTerrainVoxel(voxelType))
        {
            topHeight = float(y) + 0.5;
            valid = 1.0;
            break;
        }
    }

    terrain_smooth_columns.data[index] =
        float4(topHeight, topHeight, valid, 0.0);
}
