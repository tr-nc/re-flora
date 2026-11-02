module planar_surface_normal;

import planar_normal_fit;
import scene_marching;
import contree_node;
import marching_result;
import voxel_data;
import voxel_types;

// Presentation-only reconstruction for broad plaster surfaces. Read the
// published Contree, never the editable atlas (which may be ahead of visible
// publication). No material, voxel, depth, silhouette or collision is changed.
public float3 planarPlasterSurfaceNormal(
    StructuredBuffer<ContreeNode> nodes, StructuredBuffer<uint> leaves,
    readonly RWTexture3D<uint2> scene, MarchingResult surface, uint invocation)
{
    float confidence = voxelNormalConfidenceFromData(surface.surface_data);
    float3 fallback = surface.normal;
    // Wall bodies are vertical even when their tops follow sloping ground.
    // Keep floors, caps and ambiguous thin/corner geometry on their old path.
    if (surface.voxel_type != VOXEL_TYPE_STUCCO || confidence <= 0.0 ||
        abs(surface.normal.y) > 0.01)
        return fallback;
    float3 original = normalize(surface.normal);
    uint axis = abs(original.x) > abs(original.z) ? 0u : 2u;
    uint u = 2u - axis;
    uint v = 1u;
    float3 outward = float3(0);
    outward[axis] = original[axis] >= 0.0 ? 1.0 : -1.0;
    PlanarNormalFit fit = beginPlanarNormalFit();
    // More horizontal phases avoid resampling the same stair phase repeatedly;
    // three vertical rows establish planarity without reaching needlessly into
    // the roof and soil. No authored wall angle enters this estimator.
    for (int j = -1; j <= 1; ++j)
    {
        for (int i = -4; i <= 4; ++i)
        {
            float3 offset = float3(0);
            offset[u] = float(i * 4);
            offset[v] = float(j * 8);
            float3 origin = surface.center_position + (offset + outward * 32.0) / 256.0;
            float3 direction = -outward;
            // Zero components follow the same finite inverse convention as
            // ordinary primary/secondary terrain traversal.
            float3 inverseDirection = select(abs(direction) > float3(1.0e-8),
                1.0 / direction, float3(1.0e20));
            MarchingResult hit = marchTerrain(nodes, leaves, scene,
                origin, direction, inverseDirection, invocation);
            if (!hit.is_hit || hit.distance > 64.0 / 256.0 ||
                hit.voxel_type != surface.voxel_type ||
                dot(hit.normal, original) < 0.5)
                continue;
            float height = dot((hit.position - surface.center_position) * 256.0, outward);
            addPlanarNormalPoint(fit, float3(offset[u], offset[v], height));
        }
    }
    float2 slope;
    if (!finishPlanarNormalFit(fit, slope)) return fallback;
    float3 reconstructed = outward;
    reconstructed[u] = -slope.x;
    reconstructed[v] = -slope.y;
    reconstructed = normalize(reconstructed);
    return dot(reconstructed, original) >= 0.85 ? reconstructed : fallback;
}
