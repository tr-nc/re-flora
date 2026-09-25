import apple_pixel_frame;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct VertexInput {
    [[vk::location(0)]] float3 position:POSITION0;
    [[vk::location(1)]] float3 voxel_center:TEXCOORD0;
    [[vk::location(2)]] float3 shading_normal:NORMAL0;
    [[vk::location(3)]] float3 color_srgb:COLOR0;
    [[vk::location(4)]] float3 base_position:TEXCOORD1;
    [[vk::location(5)]] float4 tint:COLOR1;
    [[vk::location(6)]] float4 rotation:TEXCOORD2;
};
float3 rotateApple(float3 p,float4 q) {return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p);}
[shader("vertex")]
ApplePixelVertex main(VertexInput input) {
    ApplePixelVertex output;
    output.center=input.base_position;
    output.axisX=rotateApple(float3(1.0,0.0,0.0),input.rotation);
    output.axisY=rotateApple(float3(0.0,1.0,0.0),input.rotation);
    output.axisZ=rotateApple(float3(0.0,0.0,1.0),input.rotation);
    output.scale=(2.0/256.0)*input.tint.a;
    output.resolution=clamp(gui_input.apple_pixel_resolution,8u,64u);
    setApplePixelQuad(output,input.position.xy,camera_info);
    return output;
}
