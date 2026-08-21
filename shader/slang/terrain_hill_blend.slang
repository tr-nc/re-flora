import gradient_noise;
import solid_workgroups;
import sparse_surface_build;
import terrain_height;
import terrain_implicit;
import voxel_data;
import voxel_types;

struct TerrainHillBlendPushConstants
{
    uint4 offset;
    uint4 dim;
    float4 center_base_blend;
    float4 radii_rise_noise;
    float4 noise_params;
    uint4 options;
};

[[vk::push_constant]]
ConstantBuffer<TerrainHillBlendPushConstants> pc;

[[vk::binding(0, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(1, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

float computeHillHeightVoxels(float2 voxelXZ)
{
    float2 center = pc.center_base_blend.xy;
    float2 radii = pc.radii_rise_noise.xy;
    float2 normalized = (voxelXZ - center) / radii;
    float radialSquared = dot(normalized, normalized);

    // Continue the paraboloid below its nominal footprint so the smooth-max
    // influence decays continuously back to the unmodified terrain.
    float profile = 1.0 - radialSquared;
    float supportFade = saturate(
        1.0 - radialSquared / (pc.noise_params.y * pc.noise_params.y));
    float2 worldXZ = voxelXZ * (1.0 / 256.0);
    float organicNoise = fbmClassicGradientNoise2D(
        worldXZ.x,
        worldXZ.y,
        pc.options.x,
        pc.noise_params.x,
        3,
        2.0,
        0.5);

    return pc.center_base_blend.z +
           pc.radii_rise_noise.z * profile +
           pc.radii_rise_noise.w * organicNoise * supportFade;
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
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    if (any(dispatchThreadId >= pc.dim.xyz))
    {
        return;
    }

    uint3 atlasPosition = pc.offset.xyz + dispatchThreadId;
    uint oldAtlasData = chunk_atlas[atlasPosition];
    if (voxelTypeFromAtlasData(oldAtlasData) != VOXEL_TYPE_EMPTY)
    {
        return;
    }

    float3 voxelCenter = float3(atlasPosition) + 0.5;
    if (voxelCenter.z > pc.noise_params.z)
    {
        return;
    }

    float2 worldXZ = voxelCenter.xz * (1.0 / 256.0);
    float naturalHeightVoxels = computeSurfaceHeight2D(worldXZ) * 256.0;
    float hillHeightVoxels = computeHillHeightVoxels(voxelCenter.xz);
    float coveredHeightVoxels = smoothMaximumQuadratic(
        naturalHeightVoxels,
        hillHeightVoxels,
        pc.center_base_blend.w);

    if (voxelCenter.y > coveredHeightVoxels)
    {
        return;
    }

    bool isFrontMaterial = voxelCenter.z >=
        pc.noise_params.z - pc.noise_params.w;
    uint voxelType = isFrontMaterial
        ? pc.options.y
        : VOXEL_TYPE_DIRT;
    chunk_atlas[atlasPosition] = packVoxelAtlasTypeClearState(voxelType);
    markSolidWorkgroup(atlasPosition);
}
