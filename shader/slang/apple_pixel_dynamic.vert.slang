import apple_pixel_frame;
import apple_pixel_dynamic_pose;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
struct VertexInput {
    uint instance_index:SV_VulkanInstanceID;
    [[vk::location(0)]] float3 position:POSITION0;
    [[vk::location(1)]] float3 voxel_center:TEXCOORD0;
    [[vk::location(2)]] float3 shading_normal:NORMAL0;
    [[vk::location(3)]] float3 color_srgb:COLOR0;
    [[vk::location(4)]] float3 base_position:TEXCOORD1;
    [[vk::location(5)]] float4 tint:COLOR1;
    [[vk::location(6)]] float4 rotation:TEXCOORD2;
};
[shader("vertex")]
ApplePixelVertex main(VertexInput input) {
    ApplePixelVertex output=appleDynamicPose(input.base_position,input.rotation,input.tint.a,
        clamp(gui_input.apple_pixel_resolution,8u,64u),input.instance_index);
    setApplePixelQuad(output,input.position.xy,camera_info);
    return output;
}
