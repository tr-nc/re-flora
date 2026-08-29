module ddgi_voxel_visibility;

import ddgi_types;

public static const uint DDGI_VOXEL_VISIBILITY_MAX_STEPS = 2048u;
static const float DDGI_VOXEL_ENDPOINT_CLIP_GRACE = 0.5;
static const float DDGI_VOXEL_OPEN_SEGMENT_EPSILON = 1.0e-4;

[[vk::binding(35, 0)]] [[vk::image_format("r32ui")]]
readonly RWTexture3D<uint> ddgi_voxel_visibility_bits;

[[vk::binding(36, 0)]]
ConstantBuffer<U_DdgiVoxelVisibilityInfo> ddgi_voxel_visibility_info;

[[vk::binding(38, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> ddgi_voxel_visibility_blocks;

bool ddgiVoxelCoordinateInside(int3 coordinate, uint3 dimensions)
{
    return all(coordinate >= int3(0)) && all(coordinate < int3(dimensions));
}

public bool ddgiVoxelIsOccupied(int3 coordinate, uint3 dimensions)
{
    if (!ddgiVoxelCoordinateInside(coordinate, dimensions)) return true;
    uint3 voxel = uint3(coordinate);
    uint word = ddgi_voxel_visibility_bits[
        uint3(voxel.x >> 5u, voxel.y, voxel.z)];
    return (word & (1u << (voxel.x & 31u))) != 0u;
}

public bool ddgiPublishedVoxelIsOccupied(int3 coordinate,
                                         out uint geometryRevision,
                                         out uint glassMaterialRevision)
{
    U_DdgiVoxelVisibilityInfo info = ddgi_voxel_visibility_info;
    geometryRevision = info.geometry_revision;
    glassMaterialRevision = info.glass_material_revision;
    return info.ready != 0u &&
        ddgiVoxelIsOccupied(coordinate, info.voxel_dimensions);
}

int3 ddgiVoxelBlockCoordinate(int3 coordinate)
{
    return coordinate / int(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE);
}

bool ddgiVoxelBlockIsOccupied(int3 coordinate)
{
    return ddgi_voxel_visibility_blocks[uint3(ddgiVoxelBlockCoordinate(coordinate))] != 0u;
}

bool ddgiVoxelEndpointGrosslyOutside(float3 point, float3 dimensions)
{
    return any(point < float3(-DDGI_VOXEL_ENDPOINT_CLIP_GRACE)) ||
           any(point > dimensions + float3(DDGI_VOXEL_ENDPOINT_CLIP_GRACE));
}

bool ddgiVoxelClipAxis(float origin, float direction, float dimension,
                       inout float entry, inout float exit)
{
    if (abs(direction) <= 1.0e-20)
    {
        return origin >= 0.0 && origin <= dimension;
    }
    float first = -origin / direction;
    float second = (dimension - origin) / direction;
    entry = max(entry, min(first, second));
    exit = min(exit, max(first, second));
    return entry <= exit;
}

float ddgiVoxelAxisDelta(float direction)
{
    return abs(direction) <= 1.0e-20 ? 1.0e30 : rcp(abs(direction));
}

float ddgiVoxelAxisNext(float origin, float direction, int cell, int step)
{
    if (step == 0) return 1.0e30;
    float boundary = float(step > 0 ? cell + 1 : cell);
    return (boundary - origin) / direction;
}

int ddgiVoxelCellsToBlockExit(int cell, int step)
{
    if (step == 0) return 0;
    int blockStart = (cell / int(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE)) *
                     int(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE);
    return step > 0
        ? blockStart + int(DDGI_VOXEL_VISIBILITY_BLOCK_SIZE) - cell
        : cell - blockStart + 1;
}

float ddgiVoxelBlockAxisNext(float next, float delta, int cell, int step)
{
    int cells = ddgiVoxelCellsToBlockExit(cell, step);
    return cells == 0 ? 1.0e30 : next + delta * float(cells - 1);
}

void ddgiVoxelAdvanceAxisAcrossEmptyBlock(float crossing, int step, float delta,
                                          inout int cell, inout float next)
{
    if (step == 0 || next > crossing + DDGI_VOXEL_OPEN_SEGMENT_EPSILON) return;
    int advances = 1 + int(floor(max(
        0.0,
        (crossing + DDGI_VOXEL_OPEN_SEGMENT_EPSILON - next) / delta)));
    cell += step * advances;
    next += delta * float(advances);
}

// Tests the open segment between a surface-biased origin and a probe. Exact domain endpoints and
// the quarter-voxel surface-bias overshoot are clipped inward. Larger out-of-domain inputs are
// unknown geometry and therefore fail closed.
public float ddgiVoxelSegmentVisibility(float3 biasedWorldPosition,
                                        float3 probeWorldPosition,
                                        uint expectedGeometryRevision)
{
    U_DdgiVoxelVisibilityInfo info = ddgi_voxel_visibility_info;
    if (info.ready == 0u || info.geometry_revision != expectedGeometryRevision ||
        any(info.voxel_dimensions == uint3(0u)) ||
        any(info.packed_word_dimensions == uint3(0u)) ||
        any(info.world_to_voxel_scale <= float3(0.0)) ||
        info.max_steps == 0u || info.max_steps > DDGI_VOXEL_VISIBILITY_MAX_STEPS)
    {
        return 0.0;
    }

    float3 start = biasedWorldPosition * info.world_to_voxel_scale;
    float3 end = probeWorldPosition * info.world_to_voxel_scale;
    float3 dimensions = float3(info.voxel_dimensions);
    if (any(start != start) || any(end != end) ||
        any(abs(start) > float3(1.0e20)) || any(abs(end) > float3(1.0e20)) ||
        ddgiVoxelEndpointGrosslyOutside(start, dimensions) ||
        ddgiVoxelEndpointGrosslyOutside(end, dimensions))
    {
        return 0.0;
    }

    float3 segment = end - start;
    float segmentLength = length(segment);
    if (segmentLength <= DDGI_VOXEL_OPEN_SEGMENT_EPSILON) return 1.0;

    float entry = 0.0;
    float exit = 1.0;
    if (!ddgiVoxelClipAxis(start.x, segment.x, dimensions.x, entry, exit) ||
        !ddgiVoxelClipAxis(start.y, segment.y, dimensions.y, entry, exit) ||
        !ddgiVoxelClipAxis(start.z, segment.z, dimensions.z, entry, exit))
    {
        return 0.0;
    }
    entry = max(0.0, entry);
    exit = min(1.0, exit);
    float parameterEpsilon = min(
        DDGI_VOXEL_OPEN_SEGMENT_EPSILON / segmentLength,
        (exit - entry) * 0.25);
    entry += parameterEpsilon;
    exit -= parameterEpsilon;
    if (entry >= exit) return 1.0;

    float3 domainMax = dimensions - float3(DDGI_VOXEL_OPEN_SEGMENT_EPSILON);
    float3 clippedStart = clamp(start + segment * entry, float3(0.0), domainMax);
    float3 clippedEnd = clamp(start + segment * exit, float3(0.0), domainMax);
    float3 clippedSegment = clippedEnd - clippedStart;
    if (dot(clippedSegment, clippedSegment) <=
        DDGI_VOXEL_OPEN_SEGMENT_EPSILON * DDGI_VOXEL_OPEN_SEGMENT_EPSILON)
    {
        return 1.0;
    }

    int3 cell = int3(floor(clippedStart));
    int3 endCell = int3(floor(clippedEnd));

    int3 step = int3(sign(clippedSegment));
    float3 delta = float3(
        ddgiVoxelAxisDelta(clippedSegment.x),
        ddgiVoxelAxisDelta(clippedSegment.y),
        ddgiVoxelAxisDelta(clippedSegment.z));
    float3 next = float3(
        ddgiVoxelAxisNext(clippedStart.x, clippedSegment.x, cell.x, step.x),
        ddgiVoxelAxisNext(clippedStart.y, clippedSegment.y, cell.y, step.y),
        ddgiVoxelAxisNext(clippedStart.z, clippedSegment.z, cell.z, step.z));

    int3 cachedBlock = int3(-1);
    bool cachedBlockOccupied = true;
    for (uint stepIndex = 0u; stepIndex < DDGI_VOXEL_VISIBILITY_MAX_STEPS; ++stepIndex)
    {
        if (stepIndex >= info.max_steps) return 0.0;
        int3 block = ddgiVoxelBlockCoordinate(cell);
        if (any(block != cachedBlock))
        {
            cachedBlock = block;
            cachedBlockOccupied = ddgiVoxelBlockIsOccupied(cell);
        }

        if (cachedBlockOccupied)
        {
            if (ddgiVoxelIsOccupied(cell, info.voxel_dimensions)) return 0.0;
        }
        else if (all(block == ddgiVoxelBlockCoordinate(endCell)))
        {
            return 1.0;
        }
        if (all(cell == endCell)) return 1.0;

        float3 crossingByAxis = cachedBlockOccupied
            ? next
            : float3(
                ddgiVoxelBlockAxisNext(next.x, delta.x, cell.x, step.x),
                ddgiVoxelBlockAxisNext(next.y, delta.y, cell.y, step.y),
                ddgiVoxelBlockAxisNext(next.z, delta.z, cell.z, step.z));
        float crossing = min(crossingByAxis.x, min(crossingByAxis.y, crossingByAxis.z));
        if (crossing >= 1.0e29 || crossing > 1.0 + DDGI_VOXEL_OPEN_SEGMENT_EPSILON)
            return 0.0;
        if (cachedBlockOccupied)
        {
            bool3 tie = abs(next - float3(crossing)) <=
                        float3(DDGI_VOXEL_OPEN_SEGMENT_EPSILON);
            if (tie.x) { cell.x += step.x; next.x += delta.x; }
            if (tie.y) { cell.y += step.y; next.y += delta.y; }
            if (tie.z) { cell.z += step.z; next.z += delta.z; }
            // At an exact edge or corner crossing the ray enters only the cell
            // reached by advancing every tied axis. Side cells touched at a
            // zero-area boundary do not occlude an optical segment.
        }
        else
        {
            ddgiVoxelAdvanceAxisAcrossEmptyBlock(
                crossing, step.x, delta.x, cell.x, next.x);
            ddgiVoxelAdvanceAxisAcrossEmptyBlock(
                crossing, step.y, delta.y, cell.y, next.y);
            ddgiVoxelAdvanceAxisAcrossEmptyBlock(
                crossing, step.z, delta.z, cell.z, next.z);
        }
    }
    return 0.0;
}
