import flora_brush;
import flora_occupancy;
import flora_instance_placement;
import flora_surface_planting;
import flora_types;
import grass_growth_potential;
import hash;
import voxel_data;
import grass_survival;
import flora_lighting_resources;

static const uint OCCUPANCY_WORKGROUP_SIZE = 8u;
static const uint OCCUPANCY_MODE_REMOVE = 0u;
static const uint OCCUPANCY_MODE_ADD = 1u;
static const uint OCCUPANCY_MODE_TRIM = 2u;
static const uint OCCUPANCY_MODE_REFRESH_GROWTH_POTENTIAL = 3u;
static const uint OCCUPANCY_MODE_VALIDATE_TERRAIN = 4u;
static const uint OCCUPANCY_TRIM_MIN_GROWTH_PROGRESS = 128u;

[[vk::binding(0, 0)]]
ConstantBuffer<U_EditOccupancyInfo> edit_occupancy_info;

[[vk::binding(1, 0)]]
[[vk::image_format("r32ui")]]
RWTexture3D<uint> occupancy_data;

[[vk::binding(2, 0)]]
[[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> surface;

[[vk::binding(3, 0)]]
GLSLShaderStorageBuffer<B_OccupancyEditDispatch, Std430DataLayout> occupancy_edit_dispatch;

void markChanged()
{
    uint3 groups = (edit_occupancy_info.chunk_dim + uint3(7u)) / 8u;
    uint ignored;
    InterlockedMax(occupancy_edit_dispatch.x, groups.x, ignored);
    InterlockedMax(occupancy_edit_dispatch.y, groups.y, ignored);
    InterlockedMax(occupancy_edit_dispatch.z, groups.z, ignored);
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
void main(uint3 dispatchId : SV_DispatchThreadID)
{
    uint3 baseLocal = dispatchId;
    if (edit_occupancy_info.mode <= OCCUPANCY_MODE_TRIM)
        baseLocal += edit_occupancy_info.surface_offset;
    if (any(baseLocal >= edit_occupancy_info.chunk_dim))
    {
        return;
    }
    if (edit_occupancy_info.mode ==
        OCCUPANCY_MODE_REFRESH_GROWTH_POTENTIAL)
    {
        if (all(dispatchId == uint3(0u))) markChanged();
        return;
    }

    if (edit_occupancy_info.mode == OCCUPANCY_MODE_VALIDATE_TERRAIN)
    {
        if (occupancy_data[baseLocal] == 0u) return;
        uint3 root = edit_occupancy_info.chunk_world_offset + baseLocal;
        uint width,height,depth;
        chunk_atlas.GetDimensions(width,height,depth);
        if (any(root >= uint3(width,height,depth))) {
            occupancy_data[baseLocal] = 0u;
            markChanged();
            return;
        }
        // World-space atlas reads cross vertical chunk boundaries. Outside the
        // world is air; never sample beyond the image at the topmost root.
        uint cover[3];
        for(uint i=0u;i<3u;i++) {
            cover[i] = 0u;
            if(root.y+i+1u < height) cover[i] = chunk_atlas[root+uint3(0,i+1u,0)];
        }
        if (!grassSurvivesTerrain(chunk_atlas[root],cover[0],cover[1],cover[2])) {
            occupancy_data[baseLocal] = 0u;
            markChanged();
        }
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
        if (occupancy_data[baseLocal] != 0u) {
            occupancy_data[baseLocal] = 0u;
            markChanged();
        }
        return;
    }

    if (edit_occupancy_info.mode == OCCUPANCY_MODE_ADD)
    {
        if (occupancy_data[baseLocal] != 0u) return;
        uint floraType;
        if (selectPaintedSurfaceFlora(surface[baseLocal], edit_occupancy_info.paint_selection,
                int3(edit_occupancy_info.chunk_world_offset + baseLocal), floraType))
        {
            uint growthProgress = grassGrowthProgressLimit(
                baseLocal, edit_occupancy_info.chunk_dim);
            uint occupancyValue = packFloraOccupancyWithSpawnAge(
                edit_occupancy_info.paint_selection, growthProgress, 0u);
            occupancy_data[baseLocal] = occupancyValue;
            markChanged();
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
                selection, trimmedGrowthProgress) | (currentOccupancy & FLORA_OCCUPANCY_EXISTING_BIT);
            markChanged();
        }
    }
}
