import flora_occupancy;
import flora_placement;
import flora_surface_planting;
import flora_types;
import grass_growth_potential;
import hash;
import voxel_data;

static const uint OCCUPANCY_WORKGROUP_SIZE = 8u;
static const uint OCCUPANCY_MODE_REMOVE = 0u;
static const uint OCCUPANCY_MODE_ADD = 1u;
static const uint OCCUPANCY_MODE_TRIM = 2u;
static const uint OCCUPANCY_MODE_REFRESH_GROWTH_POTENTIAL = 3u;
static const uint OCCUPANCY_TRIM_MIN_GROWTH_PROGRESS = 128u;

struct U_EditOccupancyInfo
{
    float4 edit_segment_start_radius_vox;
    float4 edit_segment_end_radius_vox;
    uint3 chunk_world_offset;
    uint3 chunk_dim;
    uint mode;
    uint flora_tick;
    uint target_age;
    uint paint_selection;
    uint4 paint_config;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_EditOccupancyInfo> edit_occupancy_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r32ui")]]
RWTexture3D<uint> occupancy_data;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> surface;

uint floraPaintPriority(uint selection)
{
    if (selection == FLORA_SPECIES_LAVENDER ||
        selection == FLORA_SPECIES_EMBER_BLOOM)
    {
        return 1u;
    }
    return 0u;
}

bool floraPaintCanReplaceCurrent(uint paintSelection, uint currentOccupancy)
{
    if (currentOccupancy == 0u)
    {
        return true;
    }
    uint currentSelection =
        unpackFloraOccupancySelection(currentOccupancy);
    return floraPaintPriority(paintSelection) >
           floraPaintPriority(currentSelection);
}

float distanceSquaredToEditSegment(float3 point)
{
    float3 startPosition =
        edit_occupancy_info.edit_segment_start_radius_vox.xyz;
    float3 endPosition =
        edit_occupancy_info.edit_segment_end_radius_vox.xyz;
    float3 segment = endPosition - startPosition;
    float segmentLengthSquared = max(dot(segment, segment), 1.0e-4);
    float segmentFraction = clamp(
        dot(point - startPosition, segment) / segmentLengthSquared,
        0.0, 1.0);
    float3 closest = startPosition + segment * segmentFraction;
    float3 delta = point - closest;
    return dot(delta, delta);
}

uint randomizedTrimGrowthProgress(uint3 stemWorldPosition,
                                  uint currentGrowthProgress)
{
    uint minimumProgress = max(INSTANCE_GROWTH_PROGRESS_REBIRTH,
                               OCCUPANCY_TRIM_MIN_GROWTH_PROGRESS);
    uint maximumProgress = min(
        edit_occupancy_info.target_age,
        INSTANCE_GROWTH_PROGRESS_MATURE - 1u);
    maximumProgress = max(maximumProgress, minimumProgress);

    uint seed = instanceSeed(stemWorldPosition) ^
                wellonsHash(edit_occupancy_info.flora_tick);
    uint span = maximumProgress - minimumProgress + 1u;
    uint randomizedProgress = minimumProgress +
        (wellonsHash(seed ^ 0x9e3779b9u) % span);
    return min(currentGrowthProgress, randomizedProgress);
}

[shader("compute")]
[numthreads(OCCUPANCY_WORKGROUP_SIZE,
            OCCUPANCY_WORKGROUP_SIZE,
            OCCUPANCY_WORKGROUP_SIZE)]
void main(uint3 baseLocal : SV_DispatchThreadID)
{
    if (any(baseLocal >= edit_occupancy_info.chunk_dim))
    {
        return;
    }
    if (edit_occupancy_info.mode ==
        OCCUPANCY_MODE_REFRESH_GROWTH_POTENTIAL)
    {
        return;
    }

    float3 baseWorldCenter = float3(
        edit_occupancy_info.chunk_world_offset + baseLocal) + float3(0.5);
    float radius =
        edit_occupancy_info.edit_segment_start_radius_vox.w;
    if (distanceSquaredToEditSegment(baseWorldCenter) > radius * radius)
    {
        return;
    }

    if (edit_occupancy_info.mode == OCCUPANCY_MODE_REMOVE)
    {
        occupancy_data[baseLocal] = 0u;
        return;
    }

    if (edit_occupancy_info.mode == OCCUPANCY_MODE_ADD)
    {
        uint surfaceData = surface[baseLocal];
        if (surfaceData == 0u)
        {
            return;
        }
        if (isFloraSurfacePlantable(voxelTypeFromData(surfaceData)))
        {
            uint currentOccupancy = occupancy_data[baseLocal];
            if (!floraPaintCanReplaceCurrent(
                    edit_occupancy_info.paint_selection, currentOccupancy))
            {
                return;
            }

            int3 worldBasePosition = int3(
                edit_occupancy_info.chunk_world_offset + baseLocal);
            if (!floraSparsePaintSelectionAllows(
                    edit_occupancy_info.paint_selection,
                    worldBasePosition,
                    edit_occupancy_info.paint_config))
            {
                return;
            }

            uint growthProgress = INSTANCE_GROWTH_PROGRESS_MATURE;
            if (floraPaintPriority(
                    edit_occupancy_info.paint_selection) == 0u)
            {
                growthProgress = grassGrowthProgressLimit(
                    baseLocal, edit_occupancy_info.chunk_dim);
            }
            uint occupancyValue = packFloraOccupancyWithSpawnAge(
                edit_occupancy_info.paint_selection, growthProgress, 0u);
            occupancy_data[baseLocal] = occupancyValue;
        }
    }

    if (edit_occupancy_info.mode == OCCUPANCY_MODE_TRIM)
    {
        uint currentOccupancy = occupancy_data[baseLocal];
        if (currentOccupancy == 0u)
        {
            return;
        }
        uint growthProgress =
            unpackFloraOccupancyGrowth(currentOccupancy);
        uint trimmedGrowthProgress = randomizedTrimGrowthProgress(
            edit_occupancy_info.chunk_world_offset + baseLocal +
                uint3(0u, 1u, 0u),
            growthProgress);
        if (growthProgress > trimmedGrowthProgress)
        {
            uint selection =
                unpackFloraOccupancySelection(currentOccupancy);
            occupancy_data[baseLocal] = packFloraOccupancy(
                selection, trimmedGrowthProgress);
        }
    }
}
