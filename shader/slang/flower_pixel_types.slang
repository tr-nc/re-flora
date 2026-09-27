module flower_pixel_types;
public struct FlowerPixelVertex {
    public float4 position:SV_Position;
    public [[vk::location(0)]] float2 uv:TEXCOORD0;
    public [[vk::location(1)]] nointerpolation float3 center:TEXCOORD1;
    public [[vk::location(2)]] nointerpolation float radius:TEXCOORD2;
    public [[vk::location(3)]] nointerpolation uint resolution:TEXCOORD3;
    public [[vk::location(4)]] nointerpolation uint tileIndex:TEXCOORD4;
    public [[vk::location(5)]] nointerpolation uint offset:TEXCOORD5;
};
