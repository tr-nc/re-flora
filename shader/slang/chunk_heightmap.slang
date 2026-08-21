import chunk_writer_types;
import terrain_height;

[[vk::binding(0, 0)]]
ConstantBuffer<U_RegionInfo> region_info;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_Heightmap, Std430DataLayout> heightmap;

[shader("compute")]
[numthreads(8, 8, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint x = dispatchThreadId.x;
    uint z = dispatchThreadId.y;
    if (x >= region_info.dim.x || z >= region_info.dim.z)
    {
        return;
    }

    static const float VOXEL_SCALE = 1.0 / 256.0;
    uint3 atlasPosition = region_info.offset + uint3(x, 0u, z);
    float3 worldVoxelPosition =
        (float3(atlasPosition) + 0.5) * VOXEL_SCALE;

    float surfaceHeight = computeSurfaceHeight2D(worldVoxelPosition.xz);
    float baseShape = computeBaseShape2D(worldVoxelPosition.xz);
    uint index = x * region_info.dim.z + z;
    heightmap.data[index].surface_height = surfaceHeight;
    heightmap.data[index].base_shape = baseShape;
}
