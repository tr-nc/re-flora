import flower_model;
struct StemVertex {
    float4 position:SV_Position;
    [[vk::location(0)]] float3 world:TEXCOORD0;
    [[vk::location(1)]] float3 normal:TEXCOORD1;
    [[vk::location(2)]] float3 color:COLOR0;
};
[shader("fragment")]
float4 main(StemVertex input,bool front:SV_IsFrontFace):SV_Target0 {
    float3 normal=normalize(input.normal)*(front?1.0:-1.0);
    return float4(shadeFlower(input.color,input.world,normal),1);
}
