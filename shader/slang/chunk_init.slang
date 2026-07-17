import chunk_writer_types;
import solid_workgroups;
import sparse_surface_build;
import voxel_data;
import voxel_types;

[[vk::binding(0, 0)]]
ConstantBuffer<U_RegionInfo> region_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_Heightmap, Std430DataLayout> heightmap;

[[vk::binding(2, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(3, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

float hash01(uint3 position)
{
    uint value = position.x * 73856093u;
    value ^= position.y * 19349663u;
    value ^= position.z * 83492791u;
    value ^= value >> 13u;
    value *= 1274126177u;
    return float(value & 1023u) / 1023.0;
}

uint getVoxelType(float density, float baseDensity, uint3 voxelCoordinate)
{
    static const float VOXEL_SCALE = 1.0 / 256.0;
    static const float TOP_SOIL_DEPTH_WORLD = 25.0 * VOXEL_SCALE;
    static const float TRANSITION_DEPTH_WORLD = 15.0 * VOXEL_SCALE;

    if (density < 0.0)
    {
        return VOXEL_TYPE_EMPTY;
    }
    if (baseDensity <= TOP_SOIL_DEPTH_WORLD)
    {
        return VOXEL_TYPE_DIRT;
    }

    float transitionEnd = TOP_SOIL_DEPTH_WORLD + TRANSITION_DEPTH_WORLD;
    if (baseDensity < transitionEnd)
    {
        float t = (baseDensity - TOP_SOIL_DEPTH_WORLD) /
                  TRANSITION_DEPTH_WORLD;
        float dirtProbability = 1.0 - clamp(t, 0.0, 1.0);
        if (hash01(voxelCoordinate) < dirtProbability)
        {
            return VOXEL_TYPE_DIRT;
        }
    }
    return VOXEL_TYPE_ROCK;
}

void markSolidWorkgroup(uint3 voxelPosition)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    uint workgroupIndex = solidWorkgroupIndexFromVoxel(
        voxelPosition, uint3(width, height, depth));
    uint flagWord = workgroupIndex >> 5u;
    uint flagMask = 1u << (workgroupIndex & 31u);
    uint ignored;
    InterlockedOr(solid_workgroup_flags.data[flagWord], flagMask, ignored);
}

[shader("compute")]
[numthreads(4, 4, 4)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 local = dispatchThreadId;
    if (any(local >= region_info.dim))
    {
        return;
    }

    uint3 atlasPosition = region_info.offset + local;
    uint heightmapIndex = local.x * region_info.dim.z + local.z;
    float surfaceHeight = heightmap.data[heightmapIndex].surface_height;
    float baseShape = heightmap.data[heightmapIndex].base_shape;

    static const float VOXEL_SCALE = 1.0 / 256.0;
    float3 worldVoxelPosition =
        (float3(atlasPosition) + 0.5) * VOXEL_SCALE;
    float density = surfaceHeight - worldVoxelPosition.y;
    float baseDensity = baseShape - worldVoxelPosition.y;
    uint voxelType = getVoxelType(density, baseDensity, atlasPosition);

    chunk_atlas[atlasPosition] = packVoxelAtlasTypeClearState(voxelType);
    if (voxelType != VOXEL_TYPE_EMPTY)
    {
        markSolidWorkgroup(atlasPosition);
    }
}
