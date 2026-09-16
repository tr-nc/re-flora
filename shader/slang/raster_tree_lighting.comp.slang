import tracer_types;
import contree_node;
import local_lighting;
import terrain_ray_origin;
import packer;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(7,0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(8,0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(9,0)]] [[vk::image_format("rg32ui")]] readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(16,0)]] [[vk::image_format("r8ui")]] readonly RWTexture3D<uint> chunk_atlas;
[[vk::binding(42,0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43,0)]] StructuredBuffer<U_LightGpu> local_lights;
import tree_scene;
import tree_cells;
[[vk::binding(48,0)]] RWStructuredBuffer<float4> raster_tree_local_light_cache;
[numthreads(64,1,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID, uint localIndex : SV_GroupIndex) {
    uint count,stride; raster_tree_cells.GetDimensions(count,stride);
    if (id.x >= count) return;
    uint4 cell = raster_tree_cells[id.x];
    float3 irradiance = float3(0.0);
    if (cell.w != 0u) {
        float3 normal = unpackNormalOct16(cell.w-1u);
        float3 center = (float3(cell.xyz)+0.5)/256.0;
        treeCellReceiver(id.x,center,normal);
        float3 receiver = terrainVoxelSurfacePositionAlongNormal(center,normal);
        irradiance = evaluateVoxelGlassVisibleLocalLightIrradiance(local_light_info,local_lights,
            contree_node_data,contree_leaf_data,scene_tex,chunk_atlas,receiver,normal,
            gui_input.terrain_ray_origin_offset_world,localIndex);
    }
    raster_tree_local_light_cache[id.x] = float4(irradiance,1.0);
}
