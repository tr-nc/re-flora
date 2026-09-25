// All model screen fragments are lookups. No ray traversal, coverage or shading.
module model_pixel_display;
[[vk::binding(3,1)]] StructuredBuffer<float4> model_pixel_tiles;
public struct ModelPixelOutput {
    public float4 color:SV_Target0;
    public float depth:SV_Depth;
};
public ModelPixelOutput displayModelPixel(float2 uv,uint resolution,uint offset,float opacity) {
    uint2 pixel=min(uint2(saturate(uv)*float(resolution)),uint2(resolution-1u));
    float4 value=model_pixel_tiles[offset+pixel.y*resolution+pixel.x];
    ModelPixelOutput output;
    output.color=value.w<1.0?float4(value.rgb*opacity,opacity):float4(0.0);
    output.depth=value.w;
    return output;
}
