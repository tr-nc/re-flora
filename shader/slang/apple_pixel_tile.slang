module apple_pixel_tile;
import apple_pixel_frame;
import butterfly_mesh_types;
import color;
import flora_shadow;
import model_pixel_surface;
import tracer_types;

[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;
[[vk::binding(5,0)]] Sampler2D shadow_map_tex_for_vsm_ping;
[[vk::binding(9,0)]] Sampler2D leaf_shadow_opacity_blended_tex;
[[vk::binding(10,0)]] Sampler2D leaf_shadow_mask_tex;
[[vk::binding(11,0)]] Sampler2D cloud_shadow_tex;
[[vk::binding(76,0)]] StructuredBuffer<ButterflyMeshTriangle> apple_pixel_triangles;

public float4 sampleAppleModel(ApplePixelVertex input,uint2 pixel) {
    ModelPixelFrame frame;
    frame.center=input.center;frame.scale=input.scale;
    frame.axisX=input.axisX;frame.axisY=input.axisY;frame.axisZ=input.axisZ;
    uint count,stride;apple_pixel_triangles.GetDimensions(count,stride);
    float4 bounds=applePixelBounds(input,camera_info);
    ModelPixelHit hit=sampleModelPixelGeometry(apple_pixel_triangles,0u,count,frame,
        bounds,pixel,input.resolution,camera_info);
    if(hit.triangle==0xffffffffu)return float4(0,0,0,1);
    ButterflyMeshTriangle tri=apple_pixel_triangles[hit.triangle];
    float3 bary=float3(1.0-hit.bary.x-hit.bary.y,hit.bary);
    float3 n=normalize(tri.normal0.xyz*bary.x+tri.normal1.xyz*bary.y+tri.normal2.xyz*bary.z);
    float3 normal=normalize(input.axisX*n.x+input.axisY*n.y+input.axisZ*n.z);
    if(dot(normal,hit.direction)>0.0)normal=-normal;
    uint material=uint(tri.uv2.z+0.5);
    float3 srgb=material==0u?float3(203,48,47)/255.0:
        material==1u?float3(116,80,46)/255.0:float3(77,145,62)/255.0;
    StylizedVoxelShadowSample shadow=sampleStylizedVoxelShadow(gui_input,sun_info,
        shadow_camera_info,shadow_map_tex_for_vsm_ping,
        leaf_shadow_opacity_blended_tex,leaf_shadow_mask_tex,
        cloud_shadow_tex,hit.position,normal);
    float3 color=applyStylizedVoxelLighting(gui_input,sun_info,shading_info,
        srgbToLinear(srgb),shadow.weight,hit.position,normal);
    return float4(color,hit.depth);
}
