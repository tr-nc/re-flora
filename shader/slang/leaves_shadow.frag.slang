struct FragmentInput
{
    [[vk::location(0)]] float opacity : TEXCOORD0;
    float4 position : SV_Position;
};

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    float opacity = input.opacity;
    float lightDepth = clamp(input.position.z, 0.0, 1.0);
    return float4(lightDepth * opacity, 0.0, 0.0, opacity);
}
