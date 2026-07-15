module contree_marching;

import aabb;
import contree_node;

static const uint CONTREE_WORKGROUP_SIZE = 64u;
static const uint CONTREE_STACK_DEPTH = 11u;

groupshared uint contree_stack[CONTREE_WORKGROUP_SIZE][CONTREE_STACK_DEPTH];

public struct B_ContreeNodeData
{
    ContreeNode data[];
};

[[vk::binding(1, 0)]]
readonly GLSLShaderStorageBuffer<B_ContreeNodeData, Std430DataLayout> contree_node_data;

public struct ContreeMarchingResult
{
    public bool is_hit;
    public float3 position;
    public float3 center_position;
    public uint voxel_address;
};

float3 getMirroredPosition(float3 position, float3 direction, bool rangeCheck)
{
    uint3 positionBits = asuint(position);
    uint3 flippedBits = positionBits ^ uint3(0x7FFFFFu);
    float3 mirrored = asfloat(flippedBits);

    if (rangeCheck && (any(position < float3(1.0)) || any(position >= float3(2.0))))
    {
        mirrored = float3(3.0) - position;
    }
    return select(direction > float3(0.0), mirrored, position);
}

int getNodeCellIndex(float3 position, int scaleExponent)
{
    uint3 cellPosition = (asuint(position) >> uint(scaleExponent)) & 3u;
    return int(cellPosition.x + cellPosition.z * 4u + cellPosition.y * 16u);
}

float3 floorToScale(float3 position, int scaleExponent)
{
    uint mask = ~0u << uint(scaleExponent);
    return asfloat(asuint(position) & uint3(mask));
}

ContreeMarchingResult marchLocalContree(float3 origin, float3 direction, bool coarse,
                                        uint nodeOffset, uint leafOffset,
                                        uint localInvocationIndex)
{
    for (uint stackIndex = 0u; stackIndex < CONTREE_STACK_DEPTH; ++stackIndex)
    {
        contree_stack[localInvocationIndex][stackIndex] = 0u;
    }

    int scaleExponent = 21;
    uint nodeIndex = 0u;
    ContreeNode node = contree_node_data.data[nodeOffset + nodeIndex];

    ContreeMarchingResult result;
    result.is_hit = false;
    result.voxel_address = 0u;
    result.position = float3(0.0);
    result.center_position = float3(0.0);

    float2 slabDistance = intersectSlabs(float3(1.0), float3(1.9999999), origin,
                                         1.0 / direction);
    if (slabDistance.x > slabDistance.y || slabDistance.y < 0.0)
    {
        return result;
    }
    origin += max(slabDistance.x, 0.0) * direction;

    uint mirrorMask = 0u;
    if (direction.x > 0.0) mirrorMask |= 3u << 0u;
    if (direction.y > 0.0) mirrorMask |= 3u << 4u;
    if (direction.z > 0.0) mirrorMask |= 3u << 2u;

    origin = getMirroredPosition(origin, direction, true);
    float3 position = clamp(origin, 1.0, 1.9999999);
    float3 inverseDirection = 1.0 / -abs(direction);

    for (int iteration = 0; iteration < 1024; ++iteration)
    {
        if (coarse && iteration > 20 && (node.packed_0 & 1u) != 0u)
        {
            break;
        }

        uint childIndex = uint(getNodeCellIndex(position, scaleExponent)) ^ mirrorMask;

        while (childMaskTest(node, childIndex) && (node.packed_0 & 1u) == 0u)
        {
            uint stackIndex = uint(scaleExponent >> 1);
            contree_stack[localInvocationIndex][stackIndex] = nodeIndex;

            uint childCountBelow = childMaskBitCountBelow(node, childIndex);
            nodeIndex = (node.packed_0 >> 1u) + childCountBelow;
            node = contree_node_data.data[nodeOffset + nodeIndex];

            scaleExponent -= 2;
            childIndex = uint(getNodeCellIndex(position, scaleExponent)) ^ mirrorMask;
        }

        if (childMaskTest(node, childIndex) && (node.packed_0 & 1u) != 0u)
        {
            break;
        }

        int advanceScaleExponent = scaleExponent;
        uint shiftedIndex = childIndex & 0x2Au;
        bool hasNeighbor;
        if (shiftedIndex < 32u)
        {
            hasNeighbor = ((node.child_mask_lo >> shiftedIndex) & 0x00330033u) != 0u;
        }
        else
        {
            hasNeighbor =
                ((node.child_mask_hi >> (shiftedIndex - 32u)) & 0x00330033u) != 0u;
        }
        if (!hasNeighbor)
        {
            ++advanceScaleExponent;
        }

        float3 cellMinimum = floorToScale(position, advanceScaleExponent);
        float3 sideDistance = (cellMinimum - origin) * inverseDirection;
        float maximumDistance = min(min(sideDistance.x, sideDistance.y), sideDistance.z);

        bool3 sideMask = maximumDistance >= sideDistance;
        int3 base = asint(cellMinimum);
        int3 offset = int3((1 << advanceScaleExponent) - 1);
        int3 neighborMaximum = base + select(sideMask, int3(-1), offset);
        position = min(origin - abs(direction) * maximumDistance, asfloat(neighborMaximum));

        uint3 differentPositionBits = asuint(position) ^ asuint(cellMinimum);
        uint combinedDifference = (differentPositionBits.x | differentPositionBits.y |
                                   differentPositionBits.z) & 0xFFAAAAAAu;
        int differenceExponent = combinedDifference == 0u
            ? -1
            : int(firstbithigh(combinedDifference));
        if (differenceExponent > scaleExponent)
        {
            scaleExponent = differenceExponent;
            if (differenceExponent > 21)
            {
                break;
            }
            uint stackIndex = uint(scaleExponent >> 1);
            nodeIndex = contree_stack[localInvocationIndex][stackIndex];
            node = contree_node_data.data[nodeOffset + nodeIndex];
        }
    }

    if ((node.packed_0 & 1u) != 0u && scaleExponent <= 21)
    {
        result.is_hit = true;

        float3 centeredPosition = floorToScale(position, scaleExponent);
        float centerOffset = asfloat(0x3f800000u | (1u << uint(scaleExponent - 1))) - 1.0;
        centeredPosition += centerOffset;

        bool3 flip = direction > float3(0.0);
        position = getMirroredPosition(position, direction, false);
        centeredPosition = select(flip, 3.0 - centeredPosition, centeredPosition);

        uint childIndex = uint(getNodeCellIndex(position, scaleExponent));
        uint childCountBelow = childMaskBitCountBelow(node, childIndex);

        result.position = position;
        result.center_position = centeredPosition;
        result.voxel_address = leafOffset + (node.packed_0 >> 1u) + childCountBelow;
    }

    return result;
}

public ContreeMarchingResult marchContree(float3 origin, float3 direction,
                                          float3 chunkPosition, float3 chunkScaling,
                                          bool coarse, uint nodeOffset, uint leafOffset,
                                          uint localInvocationIndex)
{
    float3 localOrigin = (origin - chunkPosition) / chunkScaling + 1.0;
    float3 localDirection = direction / chunkScaling;

    ContreeMarchingResult result = marchLocalContree(
        localOrigin, localDirection, coarse, nodeOffset, leafOffset, localInvocationIndex);
    result.position = (result.position - 1.0) * chunkScaling + chunkPosition;
    result.center_position =
        (result.center_position - 1.0) * chunkScaling + chunkPosition;
    return result;
}
