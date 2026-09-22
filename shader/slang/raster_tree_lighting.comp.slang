import contree_node;
import local_lighting;
import terrain_ray_origin;
import packer;
import ddgi_query;
import raster_tree_shading;
import tree_scene;
import tree_cells;
[[vk::binding(7,0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(8,0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(9,0)]] [[vk::image_format("rg32ui")]] readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(16,0)]] [[vk::image_format("r8ui")]] readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(42,0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43,0)]] StructuredBuffer<U_LightGpu> local_lights;
// XYZ = weighted local light + fallback environment/sun; W = normal confidence.
[[vk::binding(48,0)]] RWStructuredBuffer<float4> raster_tree_light_cache;

float3 localIrradiance(float3 position, float3 normal, uint localIndex)
{
    return evaluateVoxelGlassVisibleLocalLightIrradiance(local_light_info, local_lights,
        contree_node_data, contree_leaf_data, scene_tex, chunk_atlas, position, normal,
        gui_input.terrain_ray_origin_offset_world, localIndex);
}

// Small-surface quadrature, not a fabricated smooth normal. All visible pixels
// of a weak-normal voxel share this response, like flora. Geometry still owns
// occlusion. Use actual skinned triangles (also available with wind disabled),
// never a rest-space box or an offset from the centre of a deformed voxel.
float3 fallbackIrradiance(uint slot, uint exposedFaces, uint localIndex)
{
    // One surface sample per exposed quad, in CUBE_INDICES face order. Triangle
    // centroids remain on the surface even when skinning makes a quad nonplanar.
    static const uint3 FACE_TRIANGLES[6] = {
        uint3(0,1,2), uint3(4,6,5), uint3(1,0,4),
        uint3(2,7,3), uint3(0,3,7), uint3(1,6,2)
    };
    uint base = treeCellVertexBase(slot);
    float3 diffuse = float3(0);
    float totalArea = 0.0;
    float visibleSun = 0.0;
    float sunSupport = 0.0;
    float3 sunDirection = normalize(sun_info.sun_dir);
    for (uint face = 0u; face < 6u; ++face)
    {
        if ((exposedFaces & (1u << face)) == 0u) continue;
        uint3 ids = FACE_TRIANGLES[face] + base;
        float3 a = treeScenePosition(ids.x);
        float3 b = treeScenePosition(ids.y);
        float3 c = treeScenePosition(ids.z);
        float3 areaNormal = cross(b-a, c-a);
        float area = length(areaNormal);
        if (area <= 1.0e-12) continue;
        float3 normal = areaNormal / area;
        float3 position = (a+b+c) / 3.0;
        diffuse += area * (sampleDdgiTerrainSmoothEnvironment(
            shading_info, position, position, normal).irradiance
            + localIrradiance(position, normal, localIndex));
        totalArea += area;
        float cosineArea = area * max(0.0, dot(normal, sunDirection));
        if (cosineArea > 0.0)
        {
            float3 receiver = terrainRayOriginFromPosition(position, normal,
                gui_input.terrain_ray_origin_offset_world);
            visibleSun += cosineArea * rasterTreeSunShadow(receiver, receiver, normal);
            sunSupport += cosineArea;
        }
    }
    // Area-averaged diffuse irradiance has no preferred voxel shading direction.
    // Sun uses a flora-like soft response but preserves all shadow factors. No
    // light-facing exposed support means no sun, rather than an unoccluded fill.
    float3 environmentAndLocal = totalArea > 0.0 ? diffuse / totalArea : float3(0);
    float solarVisibility = sunSupport > 0.0 ? visibleSun / sunSupport : 0.0;
    return environmentAndLocal + rasterTreeSunIrradiance() * (0.75 * solarVisibility);
}

[numthreads(64,1,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID, uint localIndex : SV_GroupIndex)
{
    uint count, stride;
    raster_tree_cells.GetDimensions(count, stride);
    if (id.x >= count) return;
    uint4 cell = raster_tree_cells[id.x];
    float3 irradiance = float3(0);
    float confidence = 1.0;
    if (cell.w != 0u)
    {
        if (gui_input.raster_tree_hybrid_lighting != 0u)
        {
            confidence = treeCellNormalConfidence(id.x);
        }
        if (confidence > 0.0)
        {
            float3 normal = unpackNormalOct16(treeCellNormalBits(cell.w));
            float3 center = (float3(cell.xyz)+0.5)/256.0;
            treeCellReceiver(id.x, center, normal);
            float3 receiver = terrainVoxelSurfacePositionAlongNormal(center, normal);
            irradiance = confidence * localIrradiance(receiver, normal, localIndex);
        }
        if (confidence < 1.0)
            irradiance += (1.0-confidence) * fallbackIrradiance(
                id.x, treeCellExposedFaces(cell.w), localIndex);
    }
    raster_tree_light_cache[id.x] = float4(irradiance, confidence);
}
