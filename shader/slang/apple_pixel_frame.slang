module apple_pixel_frame;
import model_pixel_grid;
import model_pixel_projection;
import tracer_types;

public struct ApplePixelVertex {
    public float4 position:SV_Position;
    public [[vk::location(0)]] float2 uv:TEXCOORD0;
    public [[vk::location(1)]] nointerpolation float3 center:TEXCOORD1;
    public [[vk::location(2)]] nointerpolation float3 axisX:TEXCOORD2;
    public [[vk::location(3)]] nointerpolation float3 axisY:TEXCOORD3;
    public [[vk::location(4)]] nointerpolation float3 axisZ:TEXCOORD4;
    public [[vk::location(5)]] nointerpolation float scale:TEXCOORD5;
    public [[vk::location(6)]] nointerpolation uint resolution:TEXCOORD6;
    public [[vk::location(7)]] nointerpolation uint tileIndex:TEXCOORD7;
};
public float3 applePixelWorld(ApplePixelVertex input,float3 local) {
    return input.center+input.scale*(input.axisX*local.x+input.axisY*local.y+input.axisZ*local.z);
}
public float4 applePixelBounds(ApplePixelVertex input,U_CameraInfo camera) {
    // Same approved preview framing (3.1 model units) at every distance.
    return modelPixelBounds(input.center,1.55*input.scale,camera);
}
public void setApplePixelQuad(inout ApplePixelVertex output,float2 uv,U_CameraInfo camera,
    uint viewCount,bool screenAligned,ModelPixelView view) {
    float4 bounds=applePixelBounds(output,camera);
    output.uv=uv;
    output.position=viewCount!=0u?modelOrthographicQuad(view,output.center,
        1.55*output.scale,uv,screenAligned,camera):float4(lerp(bounds.xy,bounds.zw,uv),0.0,1.0);
}
