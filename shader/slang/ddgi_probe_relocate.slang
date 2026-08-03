import ddgi_types;
import voxel_data;
import voxel_types;

static const uint DDGI_RELOCATION_WORKGROUP_SIZE = 64u;
static const uint DDGI_INVALID_DISTANCE = 0xffffffffu;
static const uint DDGI_RELOCATION_PATH_FAST = 0u;
static const uint DDGI_RELOCATION_PATH_LOCAL = 1u;
static const uint DDGI_RELOCATION_PATH_OUTER = 2u;
static const uint DDGI_RELOCATION_PATH_FULL = 3u;

[[vk::binding(16, 0)]] [[vk::image_format("r8ui")]]
readonly RWTexture3D<uint> chunk_atlas;

[[vk::binding(24, 0)]]
GLSLShaderStorageBuffer<B_DdgiProbeMetadata, Std430DataLayout>
    ddgi_probe_metadata;

[[vk::binding(37, 0)]]
GLSLShaderStorageBuffer<B_DdgiRelocationStats, Std430DataLayout>
    ddgi_relocation_stats;

struct DdgiProbeRelocationPushConstants
{
    uint3 grid_dimensions;
    uint spacing_voxels;
    float3 voxels_per_world_unit;
    uint terrain_revision;
};

[[vk::push_constant]]
ConstantBuffer<DdgiProbeRelocationPushConstants> pc;

uint preferredClearanceVoxels()
{
    return max(pc.spacing_voxels / 4u, 1u);
}

groupshared uint shared_distance[DDGI_RELOCATION_WORKGROUP_SIZE];
groupshared uint shared_clearance[DDGI_RELOCATION_WORKGROUP_SIZE];
groupshared uint shared_coordinate[DDGI_RELOCATION_WORKGROUP_SIZE];

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

uint candidateClearance(int3 position, int3 dimensions)
{
    if (!voxelIsEmpty(position, dimensions)) return 0u;

    // The first ring is deliberately conservative: every face, edge, and corner
    // neighbour must be empty. This is the explicit relocation safety cage.
    for (int z = -1; z <= 1; ++z)
    {
        for (int y = -1; y <= 1; ++y)
        {
            for (int x = -1; x <= 1; ++x)
            {
                if ((x != 0 || y != 0 || z != 0) &&
                    !voxelIsEmpty(position + int3(x, y, z), dimensions))
                {
                    return 0u;
                }
            }
        }
    }

    uint clearance = 1u;
    static const int3 AXES[6] =
    {
        int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
        int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1),
    };
    for (uint distance = 2u; distance <= preferredClearanceVoxels();
         ++distance)
    {
        [unroll]
        for (uint axis = 0u; axis < 6u; ++axis)
        {
            if (!voxelIsEmpty(position + AXES[axis] * int(distance),
                              dimensions))
            {
                return clearance;
            }
        }
        clearance = distance;
    }
    return clearance;
}

uint flattenVoxel(int3 position, int3 dimensions)
{
    return uint(position.x + dimensions.x *
               (position.y + dimensions.y * position.z));
}

bool candidateIsBetter(uint distance, uint clearance, uint coordinate,
                       uint bestDistance, uint bestClearance,
                       uint bestCoordinate)
{
    if (distance == DDGI_INVALID_DISTANCE) return false;
    if (bestDistance == DDGI_INVALID_DISTANCE) return true;

    uint preferredClearance = preferredClearanceVoxels();
    bool reachesPreferredClearance = clearance >= preferredClearance;
    bool bestReachesPreferredClearance = bestClearance >= preferredClearance;
    if (reachesPreferredClearance != bestReachesPreferredClearance)
    {
        return reachesPreferredClearance;
    }

    // Once the quality target is reached, extra clearance has no value: keep
    // the grid as regular as possible. Below the target, make the best effort
    // to improve clearance before minimizing displacement.
    if (reachesPreferredClearance)
    {
        return distance < bestDistance ||
               (distance == bestDistance && clearance > bestClearance) ||
               (distance == bestDistance && clearance == bestClearance &&
                coordinate < bestCoordinate);
    }
    return clearance > bestClearance ||
           (clearance == bestClearance && distance < bestDistance) ||
           (clearance == bestClearance && distance == bestDistance &&
            coordinate < bestCoordinate);
}

void considerCandidate(int3 candidate, int3 nominalVoxel,
                       int3 voxelDimensions, uint maximumDistanceSquared,
                       inout uint bestDistance, inout uint bestClearance,
                       inout uint bestCoordinate)
{
    if (!voxelIsInside(candidate, voxelDimensions)) return;

    // Candidate positions are voxel centres. Using doubled coordinates keeps
    // the half-voxel offsets and all ordering comparisons exact integers.
    int3 deltaTwice = candidate * 2 + int3(1) - nominalVoxel * 2;
    uint distanceSquaredTwice = uint(deltaTwice.x * deltaTwice.x +
                                     deltaTwice.y * deltaTwice.y +
                                     deltaTwice.z * deltaTwice.z);
    if (distanceSquaredTwice > maximumDistanceSquared) return;

    uint clearance = candidateClearance(candidate, voxelDimensions);
    if (clearance == 0u) return;
    uint coordinate = flattenVoxel(candidate, voxelDimensions);
    if (candidateIsBetter(distanceSquaredTwice, clearance, coordinate,
                          bestDistance, bestClearance, bestCoordinate))
    {
        bestDistance = distanceSquaredTwice;
        bestClearance = clearance;
        bestCoordinate = coordinate;
    }
}

void reduceCandidates(uint localIndex)
{
    for (uint stride = DDGI_RELOCATION_WORKGROUP_SIZE / 2u; stride > 0u;
         stride /= 2u)
    {
        GroupMemoryBarrierWithGroupSync();
        if (localIndex < stride &&
            candidateIsBetter(shared_distance[localIndex + stride],
                              shared_clearance[localIndex + stride],
                              shared_coordinate[localIndex + stride],
                              shared_distance[localIndex],
                              shared_clearance[localIndex],
                              shared_coordinate[localIndex]))
        {
            shared_distance[localIndex] =
                shared_distance[localIndex + stride];
            shared_clearance[localIndex] =
                shared_clearance[localIndex + stride];
            shared_coordinate[localIndex] =
                shared_coordinate[localIndex + stride];
        }
    }
    GroupMemoryBarrierWithGroupSync();
}

int3 unflattenVoxel(uint coordinate, int3 dimensions)
{
    uint plane = uint(dimensions.x * dimensions.y);
    uint z = coordinate / plane;
    uint remainder = coordinate % plane;
    return int3(int(remainder % uint(dimensions.x)),
                int(remainder / uint(dimensions.x)), int(z));
}

void writeResult(uint probeIndex, int3 nominalVoxel, int3 voxelDimensions,
                 uint bestDistance, uint bestClearance, uint bestCoordinate,
                 uint searchPath)
{
    float minimumVoxelsPerWorldUnit =
        min(pc.voxels_per_world_unit.x,
            min(pc.voxels_per_world_unit.y, pc.voxels_per_world_unit.z));
    DdgiProbeMetadata metadata;
    metadata.nominal_position_and_min_clearance =
        float4(float3(nominalVoxel) / pc.voxels_per_world_unit,
               1.0 / minimumVoxelsPerWorldUnit);
    metadata.actual_position_and_clearance =
        float4(metadata.nominal_position_and_min_clearance.xyz, 0.0);
    metadata.state_and_reserved =
        uint4(DDGI_PROBE_STATE_RELOCATION_FAILED, pc.terrain_revision,
              DDGI_INVALID_DISTANCE, probeIndex);

    if (bestDistance != DDGI_INVALID_DISTANCE)
    {
        int3 candidate = unflattenVoxel(bestCoordinate, voxelDimensions);
        metadata.actual_position_and_clearance =
            float4((float3(candidate) + 0.5) / pc.voxels_per_world_unit,
                   float(bestClearance) / minimumVoxelsPerWorldUnit);
        metadata.state_and_reserved =
            uint4(DDGI_PROBE_STATE_VALID, pc.terrain_revision, bestDistance,
                  bestCoordinate);
    }
    ddgi_probe_metadata.data[probeIndex] = metadata;

    InterlockedAdd(ddgi_relocation_stats.data[0], 1u);
    if (bestDistance == DDGI_INVALID_DISTANCE)
    {
        InterlockedAdd(ddgi_relocation_stats.data[2], 1u);
        return;
    }

    InterlockedAdd(ddgi_relocation_stats.data[1], 1u);
    InterlockedAdd(ddgi_relocation_stats.data[8], bestClearance);
    InterlockedAdd(ddgi_relocation_stats.data[9], bestDistance);
    if (bestDistance > 3u)
    {
        InterlockedAdd(ddgi_relocation_stats.data[10], 1u);
    }
    uint preferredClearance = preferredClearanceVoxels();
    uint halfPreferredClearance = max(preferredClearance / 2u, 1u);
    if (bestClearance < halfPreferredClearance)
    {
        InterlockedAdd(ddgi_relocation_stats.data[11], 1u);
    }
    else if (bestClearance < preferredClearance)
    {
        InterlockedAdd(ddgi_relocation_stats.data[12], 1u);
    }
    else
    {
        InterlockedAdd(ddgi_relocation_stats.data[13], 1u);
    }

    if (searchPath == DDGI_RELOCATION_PATH_FAST)
    {
        InterlockedAdd(ddgi_relocation_stats.data[3], 1u);
    }
    else if (searchPath == DDGI_RELOCATION_PATH_LOCAL)
    {
        InterlockedAdd(ddgi_relocation_stats.data[4], 1u);
    }
    else if (searchPath == DDGI_RELOCATION_PATH_OUTER)
    {
        uint resultIndex =
            bestClearance >= preferredClearanceVoxels() ? 5u : 6u;
        InterlockedAdd(ddgi_relocation_stats.data[resultIndex], 1u);
    }
    else
    {
        InterlockedAdd(ddgi_relocation_stats.data[7], 1u);
    }
}

[shader("compute")]
[numthreads(DDGI_RELOCATION_WORKGROUP_SIZE, 1, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint probeIndex = groupId.x;
    uint probeCount = pc.grid_dimensions.x * pc.grid_dimensions.y *
                      pc.grid_dimensions.z;
    if (probeIndex >= probeCount) return;

    uint gridPlane = pc.grid_dimensions.x * pc.grid_dimensions.y;
    uint gridZ = probeIndex / gridPlane;
    uint gridRemainder = probeIndex % gridPlane;
    uint gridY = gridRemainder / pc.grid_dimensions.x;
    uint gridX = gridRemainder % pc.grid_dimensions.x;
    int3 nominalVoxel = int3(uint3(gridX, gridY, gridZ) * pc.spacing_voxels);

    uint atlasWidth;
    uint atlasHeight;
    uint atlasDepth;
    chunk_atlas.GetDimensions(atlasWidth, atlasHeight, atlasDepth);
    int3 voxelDimensions = int3(atlasWidth, atlasHeight, atlasDepth);
    uint maximumDistanceSquared = pc.spacing_voxels * pc.spacing_voxels;

    uint localIndex = localInvocationId.x;
    uint bestDistance = DDGI_INVALID_DISTANCE;
    uint bestClearance = 0u;
    uint bestCoordinate = 0xffffffffu;

    // The eight voxel centres around a nominal grid intersection are the only
    // minimum-displacement candidates. Probes that already reach the preferred
    // clearance finish on this fast path.
    if (localIndex < 8u)
    {
        int3 candidate = nominalVoxel - int3(1) +
                         int3(int(localIndex & 1u),
                              int((localIndex >> 1u) & 1u),
                              int((localIndex >> 2u) & 1u));
        considerCandidate(candidate, nominalVoxel, voxelDimensions,
                          maximumDistanceSquared, bestDistance, bestClearance,
                          bestCoordinate);
    }
    shared_distance[localIndex] = bestDistance;
    shared_clearance[localIndex] = bestClearance;
    shared_coordinate[localIndex] = bestCoordinate;
    reduceCandidates(localIndex);

    if (shared_clearance[0] >= preferredClearanceVoxels())
    {
        if (localIndex == 0u)
        {
            writeResult(probeIndex, nominalVoxel, voxelDimensions,
                        shared_distance[0], shared_clearance[0],
                        shared_coordinate[0], DDGI_RELOCATION_PATH_FAST);
        }
        return;
    }

    // Valid probes that are merely close to geometry get a bounded quality
    // search instead of immediately accepting the nominal cage position. The
    // Scale the dense quality search with the interpolation cell. Probes that
    // remain under target continue into the sparse outer stage below.
    uint localRadius = preferredClearanceVoxels();
    uint localCubeSide = localRadius * 2u + 1u;
    uint localCandidateCount =
        localCubeSide * localCubeSide * localCubeSide;
    bestDistance = DDGI_INVALID_DISTANCE;
    bestClearance = 0u;
    bestCoordinate = 0xffffffffu;
    for (uint candidateIndex = localIndex;
         candidateIndex < localCandidateCount;
         candidateIndex += DDGI_RELOCATION_WORKGROUP_SIZE)
    {
        uint z = candidateIndex / (localCubeSide * localCubeSide);
        uint remainder = candidateIndex % (localCubeSide * localCubeSide);
        uint y = remainder / localCubeSide;
        uint x = remainder % localCubeSide;
        int3 candidate = nominalVoxel + int3(int(x), int(y), int(z)) -
                         int3(int(localRadius));
        considerCandidate(candidate, nominalVoxel, voxelDimensions,
                          maximumDistanceSquared, bestDistance, bestClearance,
                          bestCoordinate);
    }
    shared_distance[localIndex] = bestDistance;
    shared_clearance[localIndex] = bestClearance;
    shared_coordinate[localIndex] = bestCoordinate;
    reduceCandidates(localIndex);

    uint localBestDistance = shared_distance[0];
    uint localBestClearance = shared_clearance[0];
    uint localBestCoordinate = shared_coordinate[0];

    // A local result is final only after it reaches the spacing-relative
    // quality target. Safe-but-under-target candidates continue outward so
    // relocation remains genuinely best effort.
    if (localBestClearance >= preferredClearanceVoxels())
    {
        if (localIndex == 0u)
        {
            writeResult(probeIndex, nominalVoxel, voxelDimensions,
                        localBestDistance, localBestClearance,
                        localBestCoordinate, DDGI_RELOCATION_PATH_LOCAL);
        }
        return;
    }

    uint radius = pc.spacing_voxels / 2u;

    // A safe local candidate lets us search the remaining displacement budget
    // sparsely along the 26 face/edge/corner directions. This captures the
    // useful ways to move away from voxelized walls without paying for the
    // entire outer cube. The local best remains the fail-soft fallback.
    if (localBestDistance != DDGI_INVALID_DISTANCE)
    {
        uint outerDistanceCount = radius - localRadius;
        uint outerCandidateCount = outerDistanceCount * 26u;
        bestDistance = localIndex == 0u ? localBestDistance
                                        : DDGI_INVALID_DISTANCE;
        bestClearance = localIndex == 0u ? localBestClearance : 0u;
        bestCoordinate = localIndex == 0u ? localBestCoordinate : 0xffffffffu;
        for (uint candidateIndex = localIndex;
             candidateIndex < outerCandidateCount;
             candidateIndex += DDGI_RELOCATION_WORKGROUP_SIZE)
        {
            uint distance = localRadius + 1u + candidateIndex / 26u;
            uint directionCode = candidateIndex % 26u;
            if (directionCode >= 13u) directionCode += 1u;
            int3 direction =
                int3(int(directionCode % 3u) - 1,
                     int((directionCode / 3u) % 3u) - 1,
                     int(directionCode / 9u) - 1);
            int3 candidate = nominalVoxel + direction * int(distance);
            considerCandidate(candidate, nominalVoxel, voxelDimensions,
                              maximumDistanceSquared, bestDistance,
                              bestClearance, bestCoordinate);
        }
        shared_distance[localIndex] = bestDistance;
        shared_clearance[localIndex] = bestClearance;
        shared_coordinate[localIndex] = bestCoordinate;
        reduceCandidates(localIndex);

        if (localIndex == 0u)
        {
            writeResult(probeIndex, nominalVoxel, voxelDimensions,
                        shared_distance[0], shared_clearance[0],
                        shared_coordinate[0], DDGI_RELOCATION_PATH_OUTER);
        }
        return;
    }

    // No hard-safe candidate exists in the local cube. Preserve the original
    // exhaustive half-cell escape search for this rare correctness path.
    uint cubeSide = radius * 2u;
    uint candidateCount = cubeSide * cubeSide * cubeSide;
    bestDistance = DDGI_INVALID_DISTANCE;
    bestClearance = 0u;
    bestCoordinate = 0xffffffffu;
    for (uint candidateIndex = localIndex; candidateIndex < candidateCount;
         candidateIndex += DDGI_RELOCATION_WORKGROUP_SIZE)
    {
        uint z = candidateIndex / (cubeSide * cubeSide);
        uint remainder = candidateIndex % (cubeSide * cubeSide);
        uint y = remainder / cubeSide;
        uint x = remainder % cubeSide;
        int3 candidate = nominalVoxel + int3(int(x), int(y), int(z)) -
                         int3(int(radius));
        considerCandidate(candidate, nominalVoxel, voxelDimensions,
                          maximumDistanceSquared, bestDistance, bestClearance,
                          bestCoordinate);
    }
    shared_distance[localIndex] = bestDistance;
    shared_clearance[localIndex] = bestClearance;
    shared_coordinate[localIndex] = bestCoordinate;
    reduceCandidates(localIndex);

    if (localIndex == 0u)
    {
        writeResult(probeIndex, nominalVoxel, voxelDimensions,
                    shared_distance[0], shared_clearance[0],
                    shared_coordinate[0], DDGI_RELOCATION_PATH_FULL);
    }
}
