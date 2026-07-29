import tracer_types;
import voxel_data;
import voxel_types;

struct EnvironmentProbeSummary
{
    float4 original_position_visibility;
    float4 sample_position_confidence;
    uint4 revisions_state;
    uint4 update_info;
};

struct B_EnvironmentProbeSummaries
{
    EnvironmentProbeSummary data[];
};

[[vk::binding(2, 0)]]
ConstantBuffer<U_ShadingInfo> shading_info;

[[vk::binding(16, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[[vk::binding(19, 0)]]
GLSLShaderStorageBuffer<B_EnvironmentProbeSummaries, Std430DataLayout>
    environment_probe_summaries;

struct EnvironmentProbeClassificationPushConstants
{
    uint terrain_revision;
    uint3 padding;
};

[[vk::push_constant]]
ConstantBuffer<EnvironmentProbeClassificationPushConstants> pc;

static const uint PROBE_STATE_INACTIVE = 0u;
static const uint PROBE_STATE_DIRTY = 4u;
static const uint PROBE_STATE_RELOCATION_FAILED = 6u;
static const uint PROBE_PACKED_AXIAL_MISS_DISTANCES = 0x3fffffffu;

static const int3 RELOCATION_DIRECTIONS[6] =
{
    int3(0, 1, 0),
    int3(1, 0, 0),
    int3(-1, 0, 0),
    int3(0, 0, 1),
    int3(0, 0, -1),
    int3(0, -1, 0),
};

static const int3 CLEARANCE_NEIGHBOURS[6] =
{
    int3(1, 0, 0),
    int3(-1, 0, 0),
    int3(0, 1, 0),
    int3(0, -1, 0),
    int3(0, 0, 1),
    int3(0, 0, -1),
};

bool voxelIsInside(int3 position, int3 dimensions)
{
    return all(position >= int3(0)) && all(position < dimensions);
}

bool voxelIsEmpty(int3 position, int3 dimensions)
{
    return voxelIsInside(position, dimensions) &&
           voxelTypeFromAtlasData(chunk_atlas[uint3(position)]) ==
               VOXEL_TYPE_EMPTY;
}

bool voxelHasRelocationClearance(int3 position, int3 dimensions)
{
    if (!voxelIsEmpty(position, dimensions)) return false;

    [unroll]
    for (uint neighbourIndex = 0u; neighbourIndex < 6u; ++neighbourIndex)
    {
        if (!voxelIsEmpty(position + CLEARANCE_NEIGHBOURS[neighbourIndex],
                          dimensions))
        {
            return false;
        }
    }
    return true;
}

float3 voxelCenterToWorld(int3 voxelPosition, float3 voxelsPerWorldUnit)
{
    return (float3(voxelPosition) + 0.5) / voxelsPerWorldUnit;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint3 gridDimensions = shading_info.environment_probe_grid_dimensions;
    uint probeCount = gridDimensions.x * gridDimensions.y * gridDimensions.z;
    uint probeIndex = dispatchThreadId.x;
    if (probeIndex >= probeCount || any(gridDimensions < uint3(2u))) return;

    uint gridPlane = gridDimensions.x * gridDimensions.y;
    uint gridZ = probeIndex / gridPlane;
    uint gridRemainder = probeIndex % gridPlane;
    uint gridY = gridRemainder / gridDimensions.x;
    uint gridX = gridRemainder % gridDimensions.x;
    uint3 gridCoordinate = uint3(gridX, gridY, gridZ);

    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    uint3 atlasDimensions = uint3(atlasWidth, atlasHeight, atlasDepth);
    uint3 gridIntervals = gridDimensions - uint3(1u);
    uint3 spacingVoxels = atlasDimensions / gridIntervals;
    uint3 originalVoxel = gridCoordinate * spacingVoxels;

    EnvironmentProbeSummary summary =
        environment_probe_summaries.data[probeIndex];
    summary.original_position_visibility.w = 0.0;
    summary.sample_position_confidence =
        float4(summary.original_position_visibility.xyz, 0.0);
    summary.revisions_state =
        uint4(pc.terrain_revision, shading_info.environment_revision,
              PROBE_STATE_INACTIVE, probeIndex);
    summary.update_info =
        uint4(PROBE_PACKED_AXIAL_MISS_DISTANCES,
              PROBE_PACKED_AXIAL_MISS_DISTANCES, 1u, 0u);

    if (any(originalVoxel >= atlasDimensions))
    {
        environment_probe_summaries.data[probeIndex] = summary;
        return;
    }

    float3 voxelsPerWorldUnit =
        shading_info.environment_probe_world_to_grid_scale *
        float3(spacingVoxels);
    int3 originalVoxelSigned = int3(originalVoxel);
    if (voxelIsEmpty(originalVoxelSigned, int3(atlasDimensions)))
    {
        summary.sample_position_confidence.xyz =
            voxelCenterToWorld(originalVoxelSigned, voxelsPerWorldUnit);
        summary.revisions_state.z = PROBE_STATE_DIRTY;
        environment_probe_summaries.data[probeIndex] = summary;
        return;
    }

    uint maximumRelocationDistance =
        max(min(spacingVoxels.x,
                min(spacingVoxels.y, spacingVoxels.z)) / 2u, 1u);
    for (uint distanceVoxels = 1u;
         distanceVoxels <= maximumRelocationDistance;
         ++distanceVoxels)
    {
        [unroll]
        for (uint directionIndex = 0u; directionIndex < 6u; ++directionIndex)
        {
            int3 candidateVoxel =
                originalVoxelSigned +
                RELOCATION_DIRECTIONS[directionIndex] *
                    int(distanceVoxels);
            if (!voxelHasRelocationClearance(
                    candidateVoxel, int3(atlasDimensions)))
            {
                continue;
            }

            summary.sample_position_confidence.xyz =
                voxelCenterToWorld(candidateVoxel, voxelsPerWorldUnit);
            summary.revisions_state.z = PROBE_STATE_DIRTY;
            summary.update_info.w = distanceVoxels;
            environment_probe_summaries.data[probeIndex] = summary;
            return;
        }
    }

    summary.revisions_state.z = PROBE_STATE_RELOCATION_FAILED;
    environment_probe_summaries.data[probeIndex] = summary;
}
