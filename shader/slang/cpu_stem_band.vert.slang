import color;
import flora_shadow;
import terrain_edit_preview;
import tracer_types;
import stem_band_geometry;
import cpu_stem_band_input;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(15,0)]] ConstantBuffer<U_TerrainEditPreview> terrain_edit_preview;
[shader("vertex")]
StemBandVertex main(CpuStemBandInput input) {
    StemBandVertex output;
    float3 world=cpuStemBandPosition(input);
    output.position=mul(camera_info.view_proj_mat,float4(world,1));
    float3 normal=stemBandDirection(input.shading_normal);
    float3 shadowPoint=input.shading_position-normalize(sun_info.sun_dir)*(0.55/256.0);
    float shadow=stylizedVoxelShadowWeight(gui_input,sun_info,shadow_camera_info,shadowPoint,normal);
    float3 lit=applyStylizedVoxelLighting(gui_input,sun_info,shading_info,
        srgbToLinear(input.color_srgb),shadow,input.shading_position,normal);
    output.color=applyTerrainEditPreviewTint(terrain_edit_preview,lit,input.shading_position);
    return output;
}
