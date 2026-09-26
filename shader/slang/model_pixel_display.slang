// Shared rotating-pixel display. One canonical tile lookup; no resampling or
// triangle traversal. Canonical depth is reconstructed into scene depth.
module model_pixel_display;
import model_pixel_projection;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(3,1)]] StructuredBuffer<float4> model_pixel_tiles;
public struct ModelPixelOutput {
    public float4 color:SV_Target0;
    public float depth:SV_Depth;
};
public ModelPixelOutput displayModelPixel(float2 uv,uint resolution,uint offset,float opacity,
    float3 pivot,float radius) {
    uint2 pixel=min(uint2(saturate(uv)*float(resolution)),uint2(resolution-1u));
    float4 value=model_pixel_tiles[offset+pixel.y*resolution+pixel.x];
    float depth=value.w;
    if(gui_input.model_pixel_view_count!=0u && depth<1.0)
        depth=modelOrthographicDepth(depth,pivot,radius,camera_info);
    bool visible=value.w<1.0 && depth>=0.0 && depth<1.0;
    ModelPixelOutput output;
    output.color=visible?float4(value.rgb*opacity,opacity):float4(0.0);
    output.depth=visible?depth:1.0;
    return output;
}
