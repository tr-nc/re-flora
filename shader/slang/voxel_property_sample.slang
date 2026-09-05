import chunk_writer_types;
import voxel_data;

static const uint VOXEL_PROPERTY_MOISTURE = 1u;

groupshared uint workgroup_count;
groupshared uint workgroup_sum;

[[vk::binding(0, 0)]]
ConstantBuffer<U_VoxelPropertySampleInfo> voxel_property_sample_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(2, 0)]]
GLSLShaderStorageBuffer<B_VoxelPropertySampleResult, Std430DataLayout>
    voxel_property_sample_result;

bool targetMaskContains(uint voxelType, uint targetMask)
{
    return voxelType < 32u && ((targetMask & (1u << voxelType)) != 0u);
}

bool sampleVoxelProperty(uint atlasData, uint propertyId, out uint value)
{
    if (propertyId == VOXEL_PROPERTY_MOISTURE)
    {
        value = voxelMoistureFromAtlasData(atlasData);
        return true;
    }
    value = 0u;
    return false;
}

[shader("compute")]
[numthreads(8, 8, 8)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint groupIndex : SV_GroupIndex)
{
    if (groupIndex == 0u)
    {
        workgroup_count = 0u;
        workgroup_sum = 0u;
    }
    GroupMemoryBarrierWithGroupSync();

    uint3 local = dispatchThreadId;
    uint3 dimension = voxel_property_sample_info.atlas_dim_target_mask.xyz;
    if (!any(local >= dimension))
    {
        uint3 atlasPosition =
            voxel_property_sample_info.atlas_offset_property_id.xyz + local;
        float3 centerVoxel = voxel_property_sample_info.center_radius.xyz;
        float radiusVoxel = max(
            voxel_property_sample_info.center_radius.w, 0.0);
        float3 voxelCenter = float3(atlasPosition) + float3(0.5);

        if (radiusVoxel > 0.0 &&
            distance(voxelCenter, centerVoxel) <= radiusVoxel)
        {
            uint atlasData = chunk_atlas[atlasPosition];
            uint voxelType = voxelTypeFromAtlasData(atlasData);
            uint targetMask =
                voxel_property_sample_info.atlas_dim_target_mask.w;

            if (targetMaskContains(voxelType, targetMask))
            {
                uint propertyValue;
                uint propertyId =
                    voxel_property_sample_info.atlas_offset_property_id.w;
                if (sampleVoxelProperty(
                    atlasData, propertyId, propertyValue))
                {
                    uint ignored;
                    InterlockedAdd(workgroup_count, 1u, ignored);
                    InterlockedAdd(workgroup_sum, propertyValue, ignored);
                }
            }
        }
    }

    GroupMemoryBarrierWithGroupSync();
    if (groupIndex == 0u && workgroup_count > 0u)
    {
        uint ignored;
        InterlockedAdd(
            voxel_property_sample_result.stats.x, workgroup_count, ignored);
        InterlockedAdd(
            voxel_property_sample_result.stats.y, workgroup_sum, ignored);
    }
}
