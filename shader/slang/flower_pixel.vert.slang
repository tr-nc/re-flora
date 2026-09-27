import flower_model;
import flower_pixel_types;
import model_pixel_types;
import model_pixel_projection;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
import model_pixel_view_data;
struct VertexInput {
    [[vk::location(0)]] uint corner:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
[shader("vertex")]
FlowerPixelVertex main(VertexInput input) {
    FlowerPixelVertex output;
    ModelPixelFrame frame=flowerPlantFrame(input.instance/flowerPartCount());
    FlowerModelPart part=flowerTilePart(input.instance);
    output.center=modelPixelWorld(frame,part.center_radius.xyz);
    output.radius=part.center_radius.w*frame.scale;
    output.resolution=flower_pc.resolution;
    output.tileIndex=input.instance;
    output.offset=(input.instance-flower_pc.tile_first)*output.resolution*output.resolution;
    output.uv=float2(input.corner&1u,(input.corner>>1u)&1u);
    output.position=modelOrthographicQuad(readModelPixelView(input.instance),output.center,output.radius,output.uv,
            gui_input.model_pixel_screen_grid!=0u,camera_info);
    if(frame.scale<=1e-7)output.position=float4(2,2,2,1);
    return output;
}
