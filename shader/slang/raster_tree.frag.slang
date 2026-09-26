import tracer_types;
import contree_node;
import ddgi_query;
import terrain_ray_origin;
import terrain_edit_preview;
import tracer_material;
import voxel_types;
import tree_scene;
import tree_cells;
import raster_tree_shading;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(15,0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(48,0)]] StructuredBuffer<float4> raster_tree_light_cache;

float4 treeCachedLighting(float3 center) {
    uint3 p = uint3(floor(center*256.0));
    uint hash = p.x*73856093u ^ p.y*19349663u ^ p.z*83492791u;
    hash ^= hash>>16u;
    uint count,stride; raster_tree_cells.GetDimensions(count,stride);
    for (uint i=0u;i<64u;++i) {
        uint slot = (hash+i)&(count-1u);
        uint4 cell = raster_tree_cells[slot];
        if (cell.w==0u) break;
        if (all(cell.xyz==p)) return raster_tree_light_cache[slot];
    }
    return float4(0,0,0,1);
}
struct FragmentInput {
    [[vk::location(0)]] float3 world_position : TEXCOORD0;
    [[vk::location(1)]] nointerpolation float3 center : TEXCOORD1;
    [[vk::location(2)]] float3 normal : NORMAL0;
};
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0 {
    float3 receiverCenter = treeSceneActive() ? input.world_position : input.center;
    float4 cached = treeCachedLighting(input.center);
    float3 irradiance = cached.xyz;
    // Off and fully reliable cells retain the original shading. Fully degenerate
    // cells skip the smooth-normal DDGI query altogether; only the transition
    // band blends it with the once-per-voxel cached surface average.
    if (cached.w > 0.0) {
        float3 n = normalize(input.normal);
        float shadow = rasterTreeSunShadow(receiverCenter, input.world_position, n);
        float3 direct = rasterTreeSunIrradiance()
            * max(0.0,dot(normalize(sun_info.sun_dir),n)) * shadow;
        float3 ddgiPosition = terrainVoxelSurfacePositionAlongNormal(receiverCenter,n);
        float3 environment = sampleDdgiTerrainSmoothEnvironment(shading_info,ddgiPosition,input.world_position,n).irradiance;
        irradiance += cached.w * (environment + direct);
    }
    float3 color = voxelColorByType(voxel_colors,VOXEL_TYPE_CHERRY_WOOD) * irradiance;
    color = applyTerrainEditPreviewTint(terrain_edit_preview,color,receiverCenter);
    return float4(color,1.0);
}
