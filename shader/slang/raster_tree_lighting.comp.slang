// Spatial lighting belongs to the shared posed mesh, not one sample at the
// centre of an entire tree. Both display modes consume this same irradiance.
import contree_node;
import local_lighting;
import terrain_ray_origin;
import ddgi_query;
import raster_tree_shading;
import tree_scene;
[[vk::binding(7,0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(8,0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(9,0)]] [[vk::image_format("rg32ui")]] readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(16,0)]] [[vk::image_format("r8ui")]] readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(42,0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43,0)]] StructuredBuffer<U_LightGpu> local_lights;
[[vk::binding(48,0)]] RWStructuredBuffer<float4> raster_tree_light_cache;
[numthreads(64,1,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID,uint localIndex : SV_GroupIndex) {
    if(id.x>=treeSceneVertexCount())return;
    float3 position=treeScenePosition(id.x),normal=normalize(treeSceneNormal(id.x));
    float3 receiver=terrainRayOriginFromPosition(position,normal,gui_input.terrain_ray_origin_offset_world);
    float3 local=evaluateVoxelGlassVisibleLocalLightIrradiance(local_light_info,local_lights,
        contree_node_data,contree_leaf_data,scene_tex,chunk_atlas,position,normal,
        gui_input.terrain_ray_origin_offset_world,localIndex);
    float3 ambient=sampleDdgiTerrainSmoothEnvironment(shading_info,position,position,normal).irradiance;
    float3 sun=rasterTreeSunIrradiance()*max(0.,dot(normal,normalize(sun_info.sun_dir)))
        *rasterTreeSunShadow(receiver,receiver,normal);
    raster_tree_light_cache[id.x]=float4(ambient+local+sun,1);
}
