struct FragmentInput
{
    [[vk::location(0)]] float3 color : COLOR0;
};

[shader("fragment")]
float4 main(FragmentInput input) : SV_Target0
{
    return float4(input.color, 1.0);
}
