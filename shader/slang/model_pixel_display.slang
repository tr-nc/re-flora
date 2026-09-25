// Shared A/B display: A rotates the canonical pixels; B resamples exactly the
// same tile onto an N x N screen-aligned grid. Neither path intersects geometry.
module model_pixel_display;
import model_pixel_projection;
import model_pixel_view_data;
import tracer_types;
[[vk::binding(0,0)]] ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(3,1)]] StructuredBuffer<float4> model_pixel_tiles;
public struct ModelPixelOutput {
    public float4 color:SV_Target0;
    public float depth:SV_Depth;
};
float4 modelReadPixel(int2 pixel,uint resolution,uint offset) {
    if(any(pixel<int2(0))||any(pixel>=int2(resolution)))return float4(0,0,0,1);
    return model_pixel_tiles[offset+uint(pixel.y)*resolution+uint(pixel.x)];
}
float4 modelResamplePixel(float2 uv,uint resolution,uint offset,uint index) {
    float2 roll=readModelPixelView(index).roll,inverseRoll=float2(roll.x,-roll.y);
    float2 destination=(floor(saturate(uv)*float(resolution))+0.5)/float(resolution);
    float2 source=(modelRotatePixel(destination*2.0-1.0,inverseRoll)*0.5+0.5)*float(resolution);
    int2 center=int2(floor(source));
    float4 value=modelReadPixel(center,resolution,offset);
    if(value.w<1.0)return value;
    // Rotating a source grid must not drop thin stems/wing tips. A bounded 3x3
    // square-overlap gather supplies conservative coverage only in empty cells.
    for(int y=-1;y<=1;y++)for(int x=-1;x<=1;x++) {
        int2 pixel=center+int2(x,y);
        if(!modelPixelCellOverlap(float2(pixel)+0.5,source,inverseRoll))continue;
        float4 candidate=modelReadPixel(pixel,resolution,offset);
        if(candidate.w<value.w)value=candidate;
    }
    return value;
}
public ModelPixelOutput displayModelPixel(float2 uv,uint resolution,uint offset,float opacity,
    uint index,float3 pivot,float radius) {
    bool orthographic=gui_input.model_pixel_view_count!=0u;
    uint2 pixel=min(uint2(saturate(uv)*float(resolution)),uint2(resolution-1u));
    float4 value=orthographic && gui_input.model_pixel_screen_grid!=0u
        ?modelResamplePixel(uv,resolution,offset,index)
        :model_pixel_tiles[offset+pixel.y*resolution+pixel.x];
    float depth=value.w;
    if(orthographic && depth<1.0)depth=modelOrthographicDepth(depth,pivot,radius,camera_info);
    bool visible=value.w<1.0 && depth>=0.0 && depth<1.0;
    ModelPixelOutput output;
    output.color=visible?float4(value.rgb*opacity,opacity):float4(0.0);
    output.depth=visible?depth:1.0;
    return output;
}
