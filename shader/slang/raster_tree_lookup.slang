module raster_tree_lookup;
import contree_node;
import marching_result;
import scene_marching;

[[vk::binding(47, 0)]] StructuredBuffer<uint4> raster_tree_cells;

bool isRasterTreeCell(float3 center)
{
    uint3 p = uint3(floor(center * 256.0));
    uint hash = p.x * 73856093u ^ p.y * 19349663u ^ p.z * 83492791u;
    hash ^= hash >> 16u;
    uint count, stride;
    raster_tree_cells.GetDimensions(count, stride);
    for (uint i = 0u; i < 64u; ++i) {
        uint4 cell = raster_tree_cells[(hash + i) & (count - 1u)];
        if (cell.w == 0u) return false;
        if (all(cell.xyz == p)) return true;
    }
    return false;
}

// Only camera and direct-shadow visibility use this static comparison filter.
// Secondary transport and collision still see the identical published rest geometry.
public MarchingResult skipRasterTreeSurfaces(
    MarchingResult hit, StructuredBuffer<ContreeNode> nodes,
    StructuredBuffer<uint> leaves, readonly RWTexture3D<uint2> scene,
    float3 origin, float3 direction, float3 inverseDirection, uint invocation)
{
    // Every iteration advances beyond one voxel. Bound by the scene's voxel-grid diagonal
    // in L1, so even a ray crossing every cell cannot exhaust the loop before leaving it.
    uint w,h,d; scene.GetDimensions(w,h,d);
    uint limit = (w+h+d) * 256u + 1u;
    for (uint step = 0u; step < limit && hit.is_hit; ++step) {
        if (hit.voxel_type != 5u || !isRasterTreeCell(hit.center_position)) break;
        float3 exitFace = hit.center_position + select(direction >= float3(0.0), float3(0.5/256.0), float3(-0.5/256.0));
        float3 distances = select(abs(direction) > float3(1.0e-8), (exitFace - hit.position) * inverseDirection, float3(1.0e20));
        float exitDistance = max(0.0, min(distances.x, min(distances.y,distances.z)));
        float3 next = hit.position + direction * (exitDistance + 1.0e-6);
        hit = marchScene(nodes, leaves, scene, next, direction, inverseDirection, invocation);
    }
    if (hit.is_hit) hit.distance = length(hit.position - origin);
    return hit;
}
