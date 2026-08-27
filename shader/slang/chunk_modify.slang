import chunk_writer_types;
import solid_workgroups;
import sparse_surface_build;
import voxel_data;
import voxel_types;

static const uint PRIMITIVE_KIND_ROUND_CONE = 0u;
static const uint PRIMITIVE_KIND_CUBOID = 1u;
static const uint PRIMITIVE_KIND_SPHERE = 2u;
static const uint PRIMITIVE_KIND_TORUS = 3u;
static const uint VOXEL_TYPE_ANY = 0xffffffffu;
static const uint WRITE_COUNT_UNLIMITED = 0xffffffffu;
static const uint EDIT_REMOVAL_CANDIDATE_CAPACITY = 65536u;
static const int3 VOXEL_NEIGHBOURS[6] =
{
    int3(1, 0, 0), int3(-1, 0, 0), int3(0, 1, 0),
    int3(0, -1, 0), int3(0, 0, 1), int3(0, 0, -1)
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_ChunkModifyInfo> chunk_modify_info;

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_BvhNodes, Std430DataLayout>
    trunk_bvh_nodes;

[[vk::binding(2, 0)]]
readonly GLSLShaderStorageBuffer<B_RoundCones, Std430DataLayout> round_cones;

[[vk::binding(3, 0)]]
readonly GLSLShaderStorageBuffer<B_Cuboids, Std430DataLayout> cuboids;

[[vk::binding(4, 0)]]
readonly GLSLShaderStorageBuffer<B_Spheres, Std430DataLayout> spheres;

[[vk::binding(5, 0)]]
readonly GLSLShaderStorageBuffer<B_Toruses, Std430DataLayout> toruses;

[[vk::binding(6, 0)]]
[[vk::image_format("r8ui")]]
RWTexture3D<uint> chunk_atlas;

[[vk::binding(7, 0)]]
GLSLShaderStorageBuffer<B_EditStats, Std430DataLayout> edit_stats;

[[vk::binding(8, 0)]]
GLSLShaderStorageBuffer<B_EditRemovalCandidates, Std430DataLayout>
    edit_removal_candidates;

[[vk::binding(9, 0)]]
GLSLShaderStorageBuffer<B_SolidWorkgroupFlags, Std430DataLayout>
    solid_workgroup_flags;

bool bvhNodeIsLeaf(BvhNode node)
{
    return (node.offset & 0x80000000u) != 0u;
}

uint bvhNodeData(BvhNode node)
{
    return node.offset & 0x7fffffffu;
}

bool pointInAabb(float3 point, float3 minimum, float3 maximum)
{
    return all(point >= minimum) && all(point < maximum);
}

float signedDistanceRoundCone(float3 point,
                              float3 centerA,
                              float3 centerB,
                              float radiusA,
                              float radiusB)
{
    float3 direction = centerB - centerA;
    float lengthSquared = dot(direction, direction);
    float radiusDifference = radiusA - radiusB;
    float aSquared = lengthSquared - radiusDifference * radiusDifference;
    float inverseLengthSquared = 1.0 / lengthSquared;

    float3 fromA = point - centerA;
    float y = dot(fromA, direction);
    float z = y - lengthSquared;
    float3 xVector = fromA * lengthSquared - direction * y;
    float xSquared = dot(xVector, xVector);
    float ySquared = y * y * lengthSquared;
    float zSquared = z * z * lengthSquared;
    float k = sign(radiusDifference) * radiusDifference *
              radiusDifference * xSquared;
    if (sign(z) * aSquared * zSquared > k)
    {
        return sqrt(xSquared + zSquared) * inverseLengthSquared - radiusB;
    }
    if (sign(y) * aSquared * ySquared < k)
    {
        return sqrt(xSquared + ySquared) * inverseLengthSquared - radiusA;
    }
    return (sqrt(xSquared * aSquared * inverseLengthSquared) +
            y * radiusDifference) * inverseLengthSquared - radiusA;
}

bool inAtlasBound(int3 position)
{
    uint width;
    uint height;
    uint depth;
    chunk_atlas.GetDimensions(width, height, depth);
    return all(position >= int3(0)) &&
           all(position < int3(width, height, depth));
}

uint loadVoxelType(int3 position)
{
    if (!inAtlasBound(position))
    {
        return VOXEL_TYPE_EMPTY;
    }
    return voxelTypeFromAtlasData(chunk_atlas[position]);
}

uint atlasDataForFill(uint oldVoxelData, uint fillVoxelType)
{
    if (chunk_modify_info.clear_fill_voxel_state != 0u)
    {
        return packVoxelAtlasData(fillVoxelType, 0u, 0u);
    }
    uint oldVoxelType = voxelTypeFromAtlasData(oldVoxelData);
    if ((fillVoxelType == VOXEL_TYPE_DIRT ||
         fillVoxelType == VOXEL_TYPE_SAND) &&
        oldVoxelType != VOXEL_TYPE_EMPTY)
    {
        return packVoxelAtlasTypePreserveState(oldVoxelData, fillVoxelType);
    }
    return packVoxelAtlasTypeClearState(fillVoxelType);
}

uint maxRemovedCountForType(uint voxelType)
{
    if (voxelType < 4u)
    {
        return chunk_modify_info.max_removed_counts_0_3[voxelType];
    }
    if (voxelType < 8u)
    {
        return chunk_modify_info.max_removed_counts_4_7[voxelType - 4u];
    }
    if (voxelType < EDIT_STATS_VOXEL_TYPE_COUNT)
    {
        return chunk_modify_info.max_removed_counts_8_11[voxelType - 8u];
    }
    return 0u;
}

bool reserveRemovedVoxel(uint voxelType)
{
    if (voxelType >= EDIT_STATS_VOXEL_TYPE_COUNT)
    {
        return false;
    }
    uint maximum = maxRemovedCountForType(voxelType);
    if (maximum == 0u)
    {
        return false;
    }

    uint removedIndex;
    InterlockedAdd(
        edit_stats.removed_counts[voxelType], 1u, removedIndex);
    if (maximum != WRITE_COUNT_UNLIMITED && removedIndex >= maximum)
    {
        uint ignored;
        InterlockedAdd(
            edit_stats.removed_counts[voxelType], 0xffffffffu, ignored);
        return false;
    }
    return true;
}

void releaseRemovedVoxel(uint voxelType)
{
    if (voxelType < EDIT_STATS_VOXEL_TYPE_COUNT)
    {
        uint ignored;
        InterlockedAdd(
            edit_stats.removed_counts[voxelType], 0xffffffffu, ignored);
    }
}

bool reserveAddedVoxel(uint voxelType, out uint addedIndex)
{
    if (voxelType >= EDIT_STATS_VOXEL_TYPE_COUNT)
    {
        return false;
    }
    InterlockedAdd(edit_stats.added_counts[voxelType], 1u, addedIndex);
    if (chunk_modify_info.max_write_count != 0u &&
        addedIndex >= chunk_modify_info.max_write_count)
    {
        uint ignored;
        InterlockedAdd(
            edit_stats.added_counts[voxelType], 0xffffffffu, ignored);
        return false;
    }
    return true;
}

bool isSolid(uint voxelType)
{
    return voxelType != VOXEL_TYPE_EMPTY;
}

bool isSurfaceVoxel(int3 position)
{
    if (!isSolid(loadVoxelType(position)))
    {
        return false;
    }
    for (int i = 0; i < 6; ++i)
    {
        if (!isSolid(loadVoxelType(position + VOXEL_NEIGHBOURS[i])))
        {
            return true;
        }
    }
    return false;
}

bool isSurfacePlacementVoxel(int3 position)
{
    if (isSolid(loadVoxelType(position)))
    {
        return false;
    }
    for (int i = 0; i < 6; ++i)
    {
        if (isSolid(loadVoxelType(position + VOXEL_NEIGHBOURS[i])))
        {
            return true;
        }
    }
    return false;
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
    uint3 local = dispatchThreadId;
    if (any(local >= chunk_modify_info.dim))
    {
        return;
    }

    int3 worldVoxelPosition = int3(local + chunk_modify_info.offset);
    if (!inAtlasBound(worldVoxelPosition))
    {
        return;
    }
    float3 worldVoxelCenter = float3(worldVoxelPosition) + float3(0.5);

    uint stack[64];
    int stackPointer = 0;
    stack[stackPointer++] = 0u;
    while (stackPointer > 0)
    {
        uint nodeIndex = stack[--stackPointer];
        BvhNode node = trunk_bvh_nodes.data[nodeIndex];
        if (!pointInAabb(worldVoxelCenter, node.aabb_min, node.aabb_max))
        {
            continue;
        }

        uint nodeData = bvhNodeData(node);
        if (bvhNodeIsLeaf(node))
        {
            uint primitiveIndex = nodeData;
            bool hit = false;
            if (chunk_modify_info.primitive_kind == PRIMITIVE_KIND_ROUND_CONE)
            {
                RoundCone primitive = round_cones.data[primitiveIndex];
                hit = signedDistanceRoundCone(
                    worldVoxelCenter,
                    primitive.center_a,
                    primitive.center_b,
                    primitive.radius_a,
                    primitive.radius_b) < 0.0;
            }
            else if (chunk_modify_info.primitive_kind == PRIMITIVE_KIND_CUBOID)
            {
                Cuboid primitive = cuboids.data[primitiveIndex];
                float3 localPoint = worldVoxelCenter - primitive.center;
                float3 qCross = cross(primitive.inverse_rotation.xyz, localPoint);
                localPoint += 2.0 * cross(
                    primitive.inverse_rotation.xyz,
                    qCross + primitive.inverse_rotation.w * localPoint);
                hit = all(abs(localPoint) <= primitive.half_size);
            }
            else if (chunk_modify_info.primitive_kind == PRIMITIVE_KIND_SPHERE)
            {
                Sphere primitive = spheres.data[primitiveIndex];
                hit = length(worldVoxelCenter - primitive.center) < primitive.radius;
            }
            else if (chunk_modify_info.primitive_kind == PRIMITIVE_KIND_TORUS)
            {
                Torus primitive = toruses.data[primitiveIndex];
                float3 localPoint = worldVoxelCenter - primitive.center;
                float3 qCross = cross(primitive.inverse_rotation.xyz, localPoint);
                localPoint += 2.0 * cross(
                    primitive.inverse_rotation.xyz,
                    qCross + primitive.inverse_rotation.w * localPoint);
                float2 ringDistance = float2(
                    length(localPoint.xy) - primitive.major_radius,
                    localPoint.z);
                hit = length(ringDistance) < primitive.tube_radius;
            }

            if (hit)
            {
                if (chunk_modify_info.surface_only != 0u)
                {
                    if (chunk_modify_info.fill_voxel_type == VOXEL_TYPE_EMPTY)
                    {
                        if (!isSurfaceVoxel(worldVoxelPosition))
                        {
                            return;
                        }
                    }
                    else if (!isSurfacePlacementVoxel(worldVoxelPosition))
                    {
                        return;
                    }
                }

                uint oldVoxelData = chunk_atlas[worldVoxelPosition];
                uint oldVoxelType = voxelTypeFromAtlasData(oldVoxelData);
                if (chunk_modify_info.target_voxel_type != VOXEL_TYPE_ANY &&
                    oldVoxelType != chunk_modify_info.target_voxel_type)
                {
                    return;
                }
                if (oldVoxelType == chunk_modify_info.fill_voxel_type)
                {
                    return;
                }
                if (!reserveRemovedVoxel(oldVoxelType))
                {
                    return;
                }

                uint addedIndex;
                if (!reserveAddedVoxel(
                    chunk_modify_info.fill_voxel_type, addedIndex))
                {
                    releaseRemovedVoxel(oldVoxelType);
                    return;
                }

                if (chunk_modify_info.fill_voxel_type == VOXEL_TYPE_EMPTY &&
                    addedIndex < EDIT_REMOVAL_CANDIDATE_CAPACITY)
                {
                    edit_removal_candidates.positions[addedIndex] =
                        float4(worldVoxelCenter / 256.0, 1.0);
                }
                chunk_atlas[worldVoxelPosition] = atlasDataForFill(
                    oldVoxelData, chunk_modify_info.fill_voxel_type);
                if (chunk_modify_info.fill_voxel_type != VOXEL_TYPE_EMPTY)
                {
                    markSolidWorkgroup(uint3(worldVoxelPosition));
                }
                return;
            }
        }
        else
        {
            stack[stackPointer++] = nodeData + 1u;
            stack[stackPointer++] = nodeData;
        }
    }
}
