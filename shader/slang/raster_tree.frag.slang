import tracer_types;
import contree_node;
import ddgi_query;

import terrain_ray_origin;
import terrain_edit_preview;
import tracer_material;
import tracer_shadowing;
import voxel_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(5,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(12,0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(13,0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(14,0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(15,0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(17,0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(47,0)]] StructuredBuffer<uint4> raster_tree_cells;
[[vk::binding(48,0)]] StructuredBuffer<float4> raster_tree_local_light_cache;
float3 treeLocalIrradiance(float3 center) {
    uint3 p = uint3(floor(center*256.0));
    uint hash = p.x*73856093u ^ p.y*19349663u ^ p.z*83492791u;
    hash ^= hash>>16u;
    uint count,stride; raster_tree_cells.GetDimensions(count,stride);
    for (uint i=0u;i<64u;++i) {
        uint slot = (hash+i)&(count-1u);
        uint4 cell = raster_tree_cells[slot];
        if (cell.w==0u) break;
        if (all(cell.xyz==p)) return raster_tree_local_light_cache[slot].xyz;
    }
    return float3(0.0);
}
struct FragmentInput {
    [[vk::location(0)]] float3 world_position : TEXCOORD0;
    [[vk::location(1)]] nointerpolation float3 center : TEXCOORD1;
    [[vk::location(2)]] nointerpolation float3 normal : NORMAL0;
};
[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0 {
    float3 n = input.normal;
    float3 leafReceiver = terrainRayOriginAlongNormal(input.center,n,gui_input.terrain_ray_origin_offset_world);
    float3 cloudReceiver = terrainRayOriginFromPosition(input.world_position,n,gui_input.terrain_ray_origin_offset_world);
    DirectSunShadowReceiver receiver = makeDirectSunShadowReceiver(
        float4(leafReceiver,1),float4(leafReceiver,1),float4(cloudReceiver,1),int3(0),
        DIRECT_SUN_SHADOW_SOURCE_ALL,DIRECT_TERRAIN_SHADOW_VSM,DIRECT_SUN_SHADOW_SOURCE_ALL,
        DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE);
    receiver = withReceiverPlane(receiver,n);
    float shadow = directSunShadowTransmittancesVsmLeafCloud(receiver,gui_input,
        shadow_camera_info.view_proj_mat,shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,cloud_shadow_tex).combined;
    float3 direct = sun_info.sun_color * sun_info.sun_luminance * step(0.0,sun_info.sun_dir.y)
        * max(0.0,dot(normalize(sun_info.sun_dir),n)) * shadow;
    direct += treeLocalIrradiance(input.center);
    float3 ddgiPosition = terrainVoxelSurfacePositionAlongNormal(input.center,n);
    float3 environment = sampleDdgiTerrainSmoothEnvironment(shading_info,ddgiPosition,input.world_position,n).irradiance;
    float3 color = voxelColorByType(voxel_colors,VOXEL_TYPE_CHERRY_WOOD) * (environment + direct);
    color = applyTerrainEditPreviewTint(terrain_edit_preview,color,input.center);
    return float4(color,1.0);
}
