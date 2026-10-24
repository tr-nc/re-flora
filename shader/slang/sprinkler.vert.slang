import color;
import flora_shadow;
import flora_types;
import model_mesh_geometry;
import model_mesh_lighting;
import model_mesh_types;
import model_mesh_view;
import model_view_quantization;
import model_surface_cache;
import model_pixel_types;
import model_bake_triangle;
import sprinkler_animation;
import terrain_edit_preview;
import tracer_types;

[[vk::binding(6,0)]] ConstantBuffer<U_FloraGrowthInfo> flora_growth_info;
[[vk::binding(15,0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[[vk::binding(11,1)]] StructuredBuffer<float4> sprinkler_instances;
[[vk::binding(18,1)]] StructuredBuffer<float4> sprinkler_model_colors;
struct SprinklerPart { uint4 range; };
[[vk::binding(31,1)]] StructuredBuffer<SprinklerPart> sprinkler_parts;

[shader("vertex")]
ModelMeshOutput main(uint vertex:SV_VulkanVertexID,uint instance:SV_VulkanInstanceID)
{
    uint count, stride;
    sprinkler_parts.GetDimensions(count,stride);
    float4 pose=sprinkler_instances[instance];
    float angle=gui_input.sprinkler_rotor_angle+frac(pose.w)*6.28318530718;
    uint design=min(gui_input.sprinkler_design,count/SPRINKLER_ROTARY_FRAMES-1u);
    SprinklerPart part=sprinkler_parts[design*SPRINKLER_ROTARY_FRAMES+sprinklerRotaryFrame(angle)];
    ModelMeshOutput output; output.position=float4(2,2,2,1);output.color=float4(0);
    // One fixed assembly frame selects the same camera view/grid for all discrete
    // internal poses. Do not rotate this frame with the nozzle.
    ModelPixelFrame physical=sprinklerHardwareFrame(pose.xyz,gui_input.sprinkler_size_scale);
    float3 position,normal; uint material;
    CachedModelVertex cached;
    if(cachedModelVertex(vertex,part.range.z,5u,physical,float3(0),cached)) {
        if(!cached.visible)return output;
        position=cached.position;normal=cached.normal;material=cached.material;
        output.position=cached.clip;
    } else {
        if(vertex>=part.range.y*3u)return output;
        ButterflyMeshTriangle t=model_mesh_triangles[part.range.x+vertex/3u];
        ModelPixelFrame rendered=modelMeshViewFrame(physical,float3(0));
        position=modelViewWorldPoint(rendered,meshPoint(t,vertex%3u));
        normal=normalize(modelViewWorldVector(rendered,meshNormal(t,vertex%3u)));
        material=uint(t.uv2.z+0.5);
        output.position=mul(camera_info.view_proj_mat,float4(position,1));
    }
    float3 baseColor=srgbToLinear(sprinkler_model_colors[material].xyz);
    float shadow=stylizedVoxelShadowWeight(gui_input,sun_info,shadow_camera_info,position,normal);
    float3 color=applyStylizedVoxelLighting(
        gui_input,sun_info,shading_info,baseColor,shadow,position,normal);
    output.color=float4(applyTerrainEditPreviewTint(terrain_edit_preview,color,position),1);
    return output;
}
